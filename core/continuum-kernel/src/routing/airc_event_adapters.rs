//! Event control adapters accept only verified addressed commands. Their shared
//! policy and subscription state remain the owners of authorization and fan-out.
//! Call `on_addressed` from an addressed inbox dispatcher; these adapters cannot
//! register on the room transcript consumer surface.

use std::sync::Arc;

use airc_core::{Body, Headers};
use airc_lib::adapter::AdapterError;
use airc_lib::{AddressedKind, Airc, VerifiedAddressed};

use super::airc_event_publisher::{
    build_publish_ack, build_subscribe_ack, build_unsubscribe_ack, AircEventPublisher,
    EventPublisherState, ParsedPublish, ParsedSubscribe, ParsedUnsubscribe,
};
use super::{
    AllowAllPolicy, AuthPolicy, CallerIdentity, RouteDecision, Verdict, EVENT_PUBLISH_BODY_HINT,
    EVENT_SUBSCRIBE_BODY_HINT, EVENT_UNSUBSCRIBE_BODY_HINT,
};

/// Stable adapter name for the subscribe path.
pub const SUBSCRIBE_ADAPTER_NAME: &str = "continuum.event.subscribe";

/// Stable adapter name for the unsubscribe path.
pub const UNSUBSCRIBE_ADAPTER_NAME: &str = "continuum.event.unsubscribe";

/// Stable adapter name for the publish path.
pub const PUBLISH_ADAPTER_NAME: &str = "continuum.event.publish";

/// Verified addressed handler for the subscribe path, identified by
/// [`EVENT_SUBSCRIBE_BODY_HINT`].
///
/// Carries an `Arc<dyn AuthPolicy>` so every inbound subscribe is
/// gated through the same substrate-wide policy chokepoint the
/// command path uses. Per PR #1529 reviewer 2 BLOCK 1: without
/// this, any peer that can reach this airc node could subscribe
/// to ANY topic (including internal substrate signals like
/// `cognition/score/persona-scored`), silently leaking cognition
/// telemetry. Defaults to `AllowAllPolicy` so the wire surface
/// works out of the box; operators install an ORM-backed or
/// capability-backed impl at boot via `with_policy()`.
pub struct EventSubscribeAdapter {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
    policy: Arc<dyn AuthPolicy>,
}

/// Verified addressed handler for the unsubscribe path, identified by
/// [`EVENT_UNSUBSCRIBE_BODY_HINT`].
///
/// No policy on this side because unsubscribe is idempotent
/// (`closed: false` is the already-gone outcome) and we WANT
/// peers to be able to clean up their own subscriptions even if
/// the subscribe gate would refuse a fresh request. A peer
/// gaining temporary access then losing it must still be able to
/// stop publishing — gating unsubscribe would create stuck
/// registrations.
pub struct EventUnsubscribeAdapter {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
}

/// Verified addressed handler for the publish path — the `emit` half of the Event
/// primitive. Registered with the airc adapter registry as claiming
/// [`EVENT_PUBLISH_BODY_HINT`].
///
/// Holds the [`AircEventPublisher`] (the fan-out engine over the shared
/// subscription registry) so an inbound publish reaches every matching
/// subscriber, and an `Arc<dyn AuthPolicy>` so every inbound publish is
/// gated through the same substrate-wide chokepoint subscribe uses.
///
/// Publish is a WRITE: unlike subscribe (which only registers the
/// caller's own interest), a publish MUTATES every subscriber's stream
/// by fanning an event out to it. Without the gate, any peer that can
/// reach this node could inject events onto ANY topic — including
/// internal substrate signals other personas act on. The gate runs on
/// the synthetic URI `events/<topic>/publish`, so operators author
/// publish policy with the same path-prefix matching as every other
/// surface.
pub struct EventPublishAdapter {
    airc: Arc<Airc>,
    publisher: Arc<AircEventPublisher>,
    policy: Arc<dyn AuthPolicy>,
}

impl EventSubscribeAdapter {
    /// Build a subscribe adapter against an existing airc handle
    /// + shared state, with [`AllowAllPolicy`] as the default
    /// auth gate. Mirrors `CommandExecutor::new` shape so the
    /// substrate composition reads uniformly across commands +
    /// events. Builder-style `with_policy` swaps the gate.
    pub fn new(airc: Arc<Airc>, state: Arc<EventPublisherState>) -> Arc<Self> {
        Arc::new(Self {
            airc,
            state,
            policy: Arc::new(AllowAllPolicy),
        })
    }

    /// Replace the auth policy. Operators wire their substrate
    /// gate here at boot. Returns `Arc<Self>` for chaining with
    /// the airc adapter registry.
    pub fn with_policy(
        airc: Arc<Airc>,
        state: Arc<EventPublisherState>,
        policy: Arc<dyn AuthPolicy>,
    ) -> Arc<Self> {
        Arc::new(Self {
            airc,
            state,
            policy,
        })
    }

    /// Process a parsed subscribe envelope: GATE the caller via
    /// AuthPolicy, then register in state, build the ack. Pure
    /// function — exposed `pub` so tests can drive it without
    /// going through airc.
    ///
    /// Returns the ack envelope `(Headers, Body)` ready to send
    /// via `Airc::reply`, OR a typed `AdapterError::Consumer` if
    /// the policy refused (`Forbidden`/`Deferred`) or state
    /// registration refused (e.g. empty topic).
    pub fn process_subscribe(
        state: &EventPublisherState,
        policy: &dyn AuthPolicy,
        parsed: &ParsedSubscribe,
    ) -> Result<(Headers, Body), AdapterError> {
        // PR #1529 reviewer 2 BLOCK 1: thread the verified airc
        // sender into the substrate's auth chokepoint BEFORE we
        // touch state. The synthetic URI for the decision is
        // `events/<topic>/subscribe` — policies match on path
        // prefixes the same way they do for commands, keeping the
        // gate authoring uniform across the two surfaces.
        let caller = CallerIdentity::airc(parsed.caller_peer_id);
        let decision = RouteDecision::Local {
            path: format!("events/{}/subscribe", parsed.request.topic),
            query: None,
            fragment: None,
        };
        match policy.gate(&decision, Some(&caller)) {
            Verdict::Allowed => {}
            Verdict::Forbidden { reason } => {
                return Err(AdapterError::Consumer(format!(
                    "EventSubscribeAdapter: forbidden by policy ({reason:?}) — \
                     caller peer={} topic={:?}",
                    parsed.caller_peer_id.0, parsed.request.topic
                )));
            }
            Verdict::Deferred {
                reason,
                prompt_target_env,
            } => {
                return Err(AdapterError::Consumer(format!(
                    "EventSubscribeAdapter: deferred by policy ({reason:?}, prompt_target_env={prompt_target_env:?}) — \
                     caller peer={} topic={:?}",
                    parsed.caller_peer_id.0, parsed.request.topic
                )));
            }
        }

        let subscription_id = state
            .register(
                parsed.caller_peer_id,
                parsed.request.topic.clone(),
                parsed.request.filter.clone(),
            )
            .map_err(AdapterError::Consumer)?;

        build_subscribe_ack(subscription_id, &parsed.request.topic)
            .map_err(|e| AdapterError::Consumer(format!("build_subscribe_ack: {e}")))
    }
}

impl EventUnsubscribeAdapter {
    /// Build an unsubscribe adapter against an existing airc
    /// handle + shared state.
    pub fn new(airc: Arc<Airc>, state: Arc<EventPublisherState>) -> Arc<Self> {
        Arc::new(Self { airc, state })
    }

    /// Process a parsed unsubscribe envelope: tear down in state,
    /// build the ack. Pure function — exposed `pub` so tests can
    /// drive it without going through airc.
    ///
    /// Returns the ack envelope `(Headers, Body)` ready to send
    /// via `Airc::reply`. Never errors — `unregister` returns
    /// `bool` (closed=true means active, false means already
    /// gone, both are valid idempotent outcomes).
    pub fn process_unsubscribe(
        state: &EventPublisherState,
        parsed: &ParsedUnsubscribe,
    ) -> Result<(Headers, Body), AdapterError> {
        let closed = state.unregister(parsed.request.subscription_id);
        build_unsubscribe_ack(parsed.request.subscription_id, closed)
            .map_err(|e| AdapterError::Consumer(format!("build_unsubscribe_ack: {e}")))
    }
}

impl EventPublishAdapter {
    /// Build a publish adapter against an existing airc handle +
    /// publisher, with [`AllowAllPolicy`] as the default gate. The
    /// publisher carries the shared `EventPublisherState`, so a publish
    /// fans out to exactly the subscriptions the subscribe adapter
    /// registered. Builder-style `with_policy` swaps the gate.
    pub fn new(airc: Arc<Airc>, publisher: Arc<AircEventPublisher>) -> Arc<Self> {
        Arc::new(Self {
            airc,
            publisher,
            policy: Arc::new(AllowAllPolicy),
        })
    }

    /// Replace the auth policy. Operators wire their substrate gate here
    /// at boot — the same `Arc<dyn AuthPolicy>` instance shared with the
    /// subscribe adapter, so read (subscribe) and write (publish) are
    /// governed by one policy.
    pub fn with_policy(
        airc: Arc<Airc>,
        publisher: Arc<AircEventPublisher>,
        policy: Arc<dyn AuthPolicy>,
    ) -> Arc<Self> {
        Arc::new(Self {
            airc,
            publisher,
            policy,
        })
    }

    /// Gate an inbound publish — the security seam, pure and exposed
    /// `pub` so the WRITE contract is testable without airc.
    ///
    /// Refuses an empty topic upfront (it would fan out to nothing —
    /// silent; refused per `[[no-fallbacks-ever]]`), then runs the
    /// caller through the AuthPolicy on the synthetic URI
    /// `events/<topic>/publish`. Returns `Ok(())` when allowed, or a
    /// typed `AdapterError::Consumer` when the topic is empty or the
    /// policy refuses (`Forbidden`/`Deferred`).
    pub fn gate_publish(
        policy: &dyn AuthPolicy,
        parsed: &ParsedPublish,
    ) -> Result<(), AdapterError> {
        if parsed.request.topic.is_empty() {
            return Err(AdapterError::Consumer(
                "EventPublishAdapter: topic must not be empty — an empty topic fans \
                 out to nothing; refuse upfront per [[no-fallbacks-ever]]"
                    .to_string(),
            ));
        }

        let caller = CallerIdentity::airc(parsed.caller_peer_id);
        let decision = RouteDecision::Local {
            path: format!("events/{}/publish", parsed.request.topic),
            query: None,
            fragment: None,
        };
        match policy.gate(&decision, Some(&caller)) {
            Verdict::Allowed => Ok(()),
            Verdict::Forbidden { reason } => Err(AdapterError::Consumer(format!(
                "EventPublishAdapter: forbidden by policy ({reason:?}) — \
                 caller peer={} topic={:?}",
                parsed.caller_peer_id.0, parsed.request.topic
            ))),
            Verdict::Deferred {
                reason,
                prompt_target_env,
            } => Err(AdapterError::Consumer(format!(
                "EventPublishAdapter: deferred by policy ({reason:?}, prompt_target_env={prompt_target_env:?}) — \
                 caller peer={} topic={:?}",
                parsed.caller_peer_id.0, parsed.request.topic
            ))),
        }
    }

    /// Gate, fan out, ack. Gated FIRST (no fan-out on a refused publish),
    /// then the payload is fanned to every matching subscriber, and the
    /// ack carries the delivered count back to the caller's `emit()`.
    async fn process_publish(
        &self,
        parsed: &ParsedPublish,
    ) -> Result<(Headers, Body), AdapterError> {
        Self::gate_publish(&*self.policy, parsed)?;

        let delivered = self
            .publisher
            .publish(&parsed.request.topic, parsed.request.payload.clone())
            .await
            .map_err(AdapterError::Consumer)?;

        build_publish_ack(&parsed.request.topic, delivered as u64)
            .map_err(|e| AdapterError::Consumer(format!("build_publish_ack: {e}")))
    }
}

fn decode_control<T: serde::de::DeserializeOwned>(
    request: &VerifiedAddressed,
    me: airc_core::PeerId,
    hint: &str,
) -> Result<T, AdapterError> {
    if request.kind() != AddressedKind::Command || request.to() != me {
        return Err(AdapterError::Consumer(
            "event control requires a command addressed to this identity".into(),
        ));
    }
    if tokio::time::Instant::now() >= request.deadline() {
        return Err(AdapterError::Consumer(
            "event control deadline elapsed".into(),
        ));
    }
    if request
        .headers()
        .get(super::HEADER_CONTINUUM_BODY_HINT)
        .map(String::as_str)
        != Some(hint)
    {
        return Err(AdapterError::Consumer("event control kind mismatch".into()));
    }
    let Body::Json(value) = request.body() else {
        return Err(AdapterError::Consumer("event control requires JSON".into()));
    };
    serde_json::from_value(value.clone()) // airc wire: decode the authenticated event-control payload.
        .map_err(|error| AdapterError::Consumer(format!("event control payload: {error}")))
}

impl EventSubscribeAdapter {
    pub async fn on_addressed(&self, request: &VerifiedAddressed) -> Result<(), AdapterError> {
        let parsed = ParsedSubscribe {
            caller_peer_id: request.from(),
            reply_to: request.from(),
            correlation_id: request.correlation(),
            request: decode_control(request, self.airc.peer_id(), EVENT_SUBSCRIBE_BODY_HINT)?,
        };
        let work = async {
            let (headers, body) = Self::process_subscribe(&self.state, &*self.policy, &parsed)?;
            self.airc
                .reply(request, headers, body)
                .await
                .map_err(|error| AdapterError::Io(format!("event control reply: {error}")))
        };
        tokio::time::timeout_at(request.deadline(), work)
            .await
            .map_err(|_| AdapterError::Consumer("event control deadline elapsed".into()))?
    }
}

impl EventUnsubscribeAdapter {
    pub async fn on_addressed(&self, request: &VerifiedAddressed) -> Result<(), AdapterError> {
        let parsed = ParsedUnsubscribe {
            caller_peer_id: request.from(),
            reply_to: request.from(),
            correlation_id: request.correlation(),
            request: decode_control(request, self.airc.peer_id(), EVENT_UNSUBSCRIBE_BODY_HINT)?,
        };
        let work = async {
            let (headers, body) = Self::process_unsubscribe(&self.state, &parsed)?;
            self.airc
                .reply(request, headers, body)
                .await
                .map_err(|error| AdapterError::Io(format!("event control reply: {error}")))
        };
        tokio::time::timeout_at(request.deadline(), work)
            .await
            .map_err(|_| AdapterError::Consumer("event control deadline elapsed".into()))?
    }
}

impl EventPublishAdapter {
    pub async fn on_addressed(&self, request: &VerifiedAddressed) -> Result<(), AdapterError> {
        let parsed = ParsedPublish {
            caller_peer_id: request.from(),
            reply_to: request.from(),
            correlation_id: request.correlation(),
            request: decode_control(request, self.airc.peer_id(), EVENT_PUBLISH_BODY_HINT)?,
        };
        let work = async {
            let (headers, body) = self.process_publish(&parsed).await?;
            self.airc
                .reply(request, headers, body)
                .await
                .map_err(|error| AdapterError::Io(format!("event control reply: {error}")))
        };
        tokio::time::timeout_at(request.deadline(), work)
            .await
            .map_err(|_| AdapterError::Consumer("event control deadline elapsed".into()))?
    }
}

#[cfg(test)]
mod tests;
