//! `AircRemoteInferenceAdapter` — implements `AIProviderAdapter`
//! whose transport is airc instead of llama.cpp.
//!
//! Joel (2026-05-31): "grid inference and they're just the same
//! command just executed across the wire and airc substrate
//! delivered payloads."
//!
//! The adapter is intentionally thin: wrap an
//! `Arc<dyn AircInferenceTransport>`, on every `generate_text`
//! call serialize → send → await → deserialize. Everything
//! interesting (correlation, framing, peer discovery, retries,
//! timeouts) lives in the transport.

use continuum_base::time::EpochMs;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::ai::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, GenerationChunk, InferenceDevice,
};
use crate::ai::types::{
    HealthState, HealthStatus, ModelInfo, RoutingInfo, TextGenerationRequest,
    TextGenerationResponse,
};

use super::peer_breaker::{self, PeerBreaker, COLD_AFTER_DEADLINES, COLD_WINDOW};
use super::protocol::{RemoteInferenceError, RemoteInferenceRequest};
use super::transport::AircInferenceTransport;

/// Provider ID used to register + select this adapter from the
/// global AdapterRegistry. `Commands.execute('inference/llm/request',
/// { provider: AIRC_REMOTE_PROVIDER_ID, ... })` (or the coordinator's
/// lane open with the same provider) routes through here.
pub const AIRC_REMOTE_PROVIDER_ID: &str = "airc-remote";

/// Default model name — the adapter is model-agnostic; the actual
/// model that serves the request is whatever the remote peer's
/// local adapter picks. This field exists because the trait
/// requires `default_model()`; the value is just an identifier so
/// the registry has something to report. Callers should set
/// `model` on their TextGenerationRequest to communicate intent.
pub const AIRC_REMOTE_DEFAULT_MODEL: &str = "airc-remote/peer-resolved";

/// The remote adapter. Holds the transport Arc; the transport
/// holds everything else.
pub struct AircRemoteInferenceAdapter {
    transport: Arc<dyn AircInferenceTransport>,
    /// Optional peer hint to thread into every outgoing request.
    /// Useful when a caller explicitly wants this adapter routing
    /// to one specific peer; None = let the transport decide.
    default_target_peer: Option<String>,
    /// The model every request names when the caller names none. The
    /// persona's durable override carries it (it chose the PEER by it), and
    /// the receiver never picks a model for you — a request that crosses the
    /// wire without one is refused there (measured 2026-09-06, BigMama's
    /// `detail` field: "No provider or model specified").
    default_model: Option<String>,
    /// Which persona's brain rides this lane, for the breaker probes ONLY —
    /// never a routing input. The breaker counter is per-adapter and every
    /// remote lane is built per-persona (`remote_lane_factory`), so a per-PEER
    /// fault (a dead grid host) is measured once per persona and each counter
    /// can sit below `COLD_AFTER_DEADLINES` while the peer is dead for all of
    /// them. `remote_lane.failed` naming only the peer is exactly what kept that
    /// invisible (card ad96f5d1, 2026-09-17): four consecutive same-peer
    /// timeouts could not be told from four different citizens each timing out
    /// once. This label is that missing attribution.
    persona: Option<String>,
    /// Told the served window every time the peer stamps one on an answer.
    /// The persona's lane factory hangs her override's writer here so the
    /// window she budgets against follows the responder's (card 1ab60567).
    window_sink: Option<Arc<dyn Fn(u32) + Send + Sync>>,
    /// The window the RESPONDER last said it served (0 = not yet heard). Reported live
    /// through `live_served_window` so a bound mind's prompt budget follows the seat the
    /// moment its first answer arrives — not at her next re-host (the sink records it
    /// for that). Measured 2026-09-18: every bound mind budgeted to the 16,384 floor
    /// against a 67k–101k seat until re-hosted, and every large prompt came back empty.
    learned_window: AtomicU32,
    offer_at_feedback: std::sync::RwLock<Option<u32>>,
    window_changed: tokio::sync::watch::Sender<()>,
    /// Flipped to true the first time a `generate_text` round-trip
    /// succeeds. `health_check` returns `Unknown` while this is
    /// false (no observation yet) and `Healthy` once it's true.
    /// Per R1 BLOCK on PR #1560: a remote adapter that reports
    /// `Healthy` by construction lies to the AdapterRegistry's
    /// selector, which then routes traffic to a dead peer. The
    /// fix is no fallback to a false-positive default — admit
    /// "no signal" until traffic proves the peer is reachable.
    has_observed_success: AtomicBool,
    /// The peer-liveness breaker: deadline misses in a row and, once tripped, the
    /// cold window. SHARED across every adapter targeting this peer
    /// (`peer_breaker::breaker_for`, bound in `with_target_peer`) so a per-PEER
    /// fault is learned once for ALL its personas instead of once each (card
    /// ad96f5d1). A peer-less adapter (a dream, a test) holds a private one.
    breaker: Arc<PeerBreaker>,
    /// When (epoch ms, this node's clock; 0 = never) the seat refused this mind's last
    /// request because every lane it serves was busy (`SeatOverloaded`, card 03660a77).
    /// Per adapter, so per mind: only the refused mind is held back, never every mind on
    /// the peer. While set, the lane refuses without a round trip until the seat reopens
    /// ([`super::overload::seat_reopened`]); her placement switch moves her home on it
    /// and clears it once she has actually moved ([`Self::clear_seat_overloaded`]).
    overloaded_since_ms: AtomicU64,
}

impl AircRemoteInferenceAdapter {
    fn advertised_window(&self) -> Option<u32> {
        let peer = uuid::Uuid::parse_str(self.default_target_peer.as_deref()?).ok()?;
        crate::capacity::gossip::global_ledger().served_window(
            peer,
            self.default_model.as_deref()?,
            continuum_base::time::EpochMs::now().millis(),
        )
    }

    pub fn new(transport: Arc<dyn AircInferenceTransport>) -> Self {
        Self {
            transport,
            default_target_peer: None,
            default_model: None,
            persona: None,
            window_sink: None,
            learned_window: AtomicU32::new(0),
            offer_at_feedback: std::sync::RwLock::new(None),
            window_changed: tokio::sync::watch::channel(()).0,
            has_observed_success: AtomicBool::new(false),
            breaker: Arc::new(PeerBreaker::default()),
            overloaded_since_ms: AtomicU64::new(0),
        }
    }

    /// Whether the seat refused this mind's last request as overloaded.
    pub fn seat_overloaded(&self) -> bool {
        self.overloaded_since_ms.load(Ordering::Relaxed) != 0
    }

    /// She has moved off the seat: a later return to it starts clean.
    pub fn clear_seat_overloaded(&self) {
        self.overloaded_since_ms.store(0, Ordering::Relaxed);
    }

    /// Refused as overloaded and the seat has not reopened since: asking again would be
    /// refused the same way. A reopened seat (a beacon heard after the refusal offering a
    /// free slot) clears the hold and is asked.
    fn held_by_overload(&self) -> bool {
        let refused_at_ms = self.overloaded_since_ms.load(Ordering::Relaxed);
        if refused_at_ms == 0 {
            return false;
        }
        let reopened = self
            .default_target_peer
            .as_deref()
            .and_then(|peer| uuid::Uuid::parse_str(peer).ok())
            .is_some_and(|seat| {
                super::overload::seat_reopened(
                    crate::capacity::gossip::global_ledger(),
                    seat,
                    refused_at_ms,
                    EpochMs::now_or_latest().millis(),
                )
            });
        if reopened {
            self.clear_seat_overloaded();
        }
        !reopened
    }

    /// Whether the lane currently refuses requests without a round trip.
    pub fn is_cold(&self) -> bool {
        self.breaker.is_cold(EpochMs::now_or_latest().millis())
    }

    /// Fold one transport outcome into the breaker. A deadline miss counts;
    /// any answer (even a refusal) proves the peer evaluates our mail and
    /// resets the count.
    fn observe(&self, outcome: &Result<(), &RemoteInferenceError>) {
        match outcome {
            Err(RemoteInferenceError::Timeout { .. }) => {
                // Fold this miss into the PEER's shared breaker: `Some(n)` means THIS
                // miss — summed across every persona on the peer — tripped it cold.
                if let Some(n) = self.breaker.observe_timeout(EpochMs::now_or_latest().millis()) {
                    continuum_base::probe!(
                        class = "remote_lane.cold",
                        peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer, never a routing decision
                        persona = %self.persona_label(),
                        deadlines = n,
                        cold_for_s = COLD_WINDOW.as_secs(),
                        "the remote peer let requests die at the deadline in a row \
                         (summed across every persona on it); refusing here for the \
                         window instead of burning turns"
                    );
                }
            }
            _ => self.breaker.observe_answer(),
        }
    }

    /// Pin every request to a specific peer. Use when the
    /// substrate's higher layer has decided this adapter
    /// instance is the dedicated route to one remote inference peer
    /// (e.g. the operator's GPU-rich grid host).
    pub fn with_target_peer(mut self, peer: impl Into<String>) -> Self {
        // Bind the PEER's shared breaker: from here this adapter folds into and
        // reads the one breaker every adapter on this peer shares (card ad96f5d1).
        let peer = peer.into();
        self.breaker = peer_breaker::breaker_for(&peer);
        self.default_target_peer = Some(peer);
        self
    }

    /// Name the model every request runs when the caller leaves `model`
    /// unset. The peer refuses a request that names none.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }

    /// Name the persona whose brain rides this lane, so the breaker probes can
    /// attribute a timeout to a citizen (card ad96f5d1). Observability only —
    /// never touches routing or the breaker count.
    pub fn with_persona(mut self, persona: impl Into<String>) -> Self {
        self.persona = Some(persona.into());
        self
    }

    /// The persona label for the breaker probes, `-` when this lane was not
    /// built per-persona (a dream, a test). Used by every `remote_lane.*` probe
    /// AND asserted in `with_persona_labels_the_breaker_probes`.
    pub(crate) fn persona_label(&self) -> &str {
        self.persona.as_deref().unwrap_or("-") // unwrap_or: probe label only — "-" = no persona bound to this lane, never a routing decision
    }

    /// Learn the responder's served window from every answer (see `window_sink`).
    pub fn with_window_sink(mut self, sink: Arc<dyn Fn(u32) + Send + Sync>) -> Self {
        self.window_sink = Some(sink);
        self
    }
}

#[async_trait]
impl AIProviderAdapter for AircRemoteInferenceAdapter {
    fn provider_id(&self) -> &str {
        AIRC_REMOTE_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "Airc Remote (grid-routed)"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // Capabilities depend on the REMOTE peer's adapter, which
        // this adapter doesn't introspect. Advertise the
        // intersection of what most modern transformer adapters
        // support; the substrate can refine via a future
        // capability-discovery handshake.
        // Text-only safe floor: without a capability-discovery handshake
        // (future card) this adapter doesn't introspect the remote peer's
        // adapter, so it advertises only what every transformer adapter does.
        // The substrate refines once the peer reports its real set.
        // Cloud-shaped from THIS host's perspective — no local hardware
        // footprint. Unknown context; defer to whatever the peer can do.
        AdapterCapabilities::builder()
            .remote()
            .context_window(u32::MAX)
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        // Treated as cloud-shaped: separate process, network-
        // shaped boundary, no local hardware. OpenAI/Anthropic-
        // tier from the caller's mental model.
        ApiStyle::OpenAI
    }

    fn default_model(&self) -> &str {
        self.default_model
            .as_deref()
            .unwrap_or(AIRC_REMOTE_DEFAULT_MODEL) // unwrap_or: the trait wants a name; the wire request itself is stamped only when with_model was set, never this placeholder
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Transport may want to do a discovery handshake here in a
        // future slice; for now the transport is stateless from
        // the adapter's perspective.
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        crate::ai::stream_sinks::drain_generation(|sink| self.generate_stream(request, sink)).await
    }

    /// The streaming primitive on the remote lane: every chunk the peer publishes
    /// reaches `sink` as it arrives — a remote-bound mind renders her answer as it
    /// forms exactly like a local one, and the lane's liveness is the next chunk.
    async fn generate_stream(
        &self,
        mut request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, String> {
        // The lane serves ONE model and the peer refuses any other: a request
        // that names the caller's local model is refused there after the full
        // wait (2026-09-07 02:01Z, +186 s: "model 'Ornith…' is not the active
        // served model (serving: Qwen3.8-27B)"). The lane's model is the
        // truth for every request on it; a caller's different name is noted.
        if let Some(lane_model) = &self.default_model {
            if let Some(requested) = &request.model {
                if requested != lane_model {
                    continuum_base::probe!(
                        class = "remote_lane.model_overridden",
                        peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer
                        requested = %requested,
                        served = %lane_model,
                        "the caller named a model the remote lane does not serve; the lane's model rides the wire"
                    );
                }
            }
            request.model = Some(lane_model.clone());
        }
        if self.is_cold() {
            continuum_base::probe!(
                class = "remote_lane.refused_cold",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer, never a routing decision
                persona = %self.persona_label(),
                "remote lane is cold; request refused without a round trip"
            );
            return Err(format!(
                "remote lane to {} is cold: {} deadline misses in a row; retry after the {} s window",
                self.default_target_peer.as_deref().unwrap_or("the default peer"),  // unwrap_or: error text only — names the transport default when no peer is pinned
                COLD_AFTER_DEADLINES,
                COLD_WINDOW.as_secs()
            ));
        }
        if self.held_by_overload() {
            continuum_base::probe!(
                class = "remote_lane.refused_overloaded",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer, never a routing decision
                persona = %self.persona_label(),
                "the seat refused her as overloaded and has not offered a slot since; not asked again"
            );
            return Err(RemoteInferenceError::SeatOverloaded {
                message: "refused as overloaded; waiting for the seat's beacon to offer a free slot"
                    .to_string(),
            }
            .to_string());
        }
        let mut envelope = RemoteInferenceRequest::new(request);
        if let Some(peer) = &self.default_target_peer {
            envelope = envelope.with_target_peer(peer.clone());
        }
        let started = std::time::Instant::now();
        let sent = self.transport.send_request_streaming(envelope, sink).await;
        self.observe(&sent.as_ref().map(|_| ()));
        let refused_at_ms = match sent {
            Err(RemoteInferenceError::SeatOverloaded { .. }) => EpochMs::now_or_latest().millis().max(1),
            _ => 0,
        };
        self.overloaded_since_ms.store(refused_at_ms, Ordering::Relaxed);
        // Every request ends in exactly one row. Before this the only remote
        // rows were the breaker's — a turn that timed out, errored, or answered
        // was invisible here, and 9 misses in 200 s could not be told from 3
        // ten-minute waits (2026-09-07 01:59Z, the first read after #3818).
        match &sent {
            Ok(r) => continuum_base::probe!(
                class = "remote_lane.answered",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer
                persona = %self.persona_label(),
                served_by = %r.served_by,
                elapsed_ms = started.elapsed().as_millis() as u64,
                out_tokens = r.text_response.usage.output_tokens,
                finish = ?r.text_response.finish_reason,
                served_window = ?r.text_response.routing.as_ref().and_then(|x| x.served_context_window),
                "remote inference answered"
            ),
            Err(e) => continuum_base::probe!(
                class = "remote_lane.failed",
                peer = %self.default_target_peer.as_deref().unwrap_or("-"),  // unwrap_or: probe label only — "-" = no pinned peer
                persona = %self.persona_label(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                error = %e,
                "remote inference failed"
            ),
        }
        let response = sent.map_err(|e| e.to_string())?;
        if let Some(window) = response
            .text_response
            .routing
            .as_ref()
            .and_then(|x| x.served_context_window)
            .filter(|w| *w > 0)
        {
            *self.offer_at_feedback.write().expect("remote offer lock") = self.advertised_window(); // Poisoning invalidates the feedback witness; fail loudly rather than invent capacity.
            self.learned_window.store(window, Ordering::Relaxed);
            self.window_changed.send_replace(());
            if let Some(sink) = self.window_sink.as_ref() {
                sink(window);
            }
        }
        // First successful round-trip flips the health observation
        // bit so subsequent health_check calls can report Healthy.
        // Relaxed is fine: a stale Unknown -> Healthy transition is
        // a one-way edge; readers tolerate either value.
        self.has_observed_success.store(true, Ordering::Relaxed);
        // The transport stamps the wire receipt; preserve it and the serving
        // metadata while identifying this caller's route as remote.
        let mut text = response.text_response;
        // Preserve whatever routing info the peer's adapter set;
        // we add ours on top.
        text.provider = AIRC_REMOTE_PROVIDER_ID.to_string();
        RoutingInfo::stamp(
            &mut text.routing,
            AIRC_REMOTE_PROVIDER_ID,
            false,
            "remote_inference",
        );
        Ok(text)
    }

    async fn health_check(&self) -> HealthStatus {
        // No active probe today — a future slice can add a
        // periodic ping. For now report based on whether any
        // generate_text round-trip has succeeded:
        //   - No observation yet → Unhealthy. HealthState has no
        //     Unknown variant today and adding one is its own slice
        //     (wire change). Per [[no-fallbacks-ever]] the worse lie
        //     is reporting Healthy on an unproven peer (sends real
        //     traffic to a dead remote); reporting Unhealthy until
        //     proven is pessimistic-but-safe — the registry's
        //     selector won't pick this adapter, which is the right
        //     default for "we don't know yet."
        //   - ≥1 round-trip succeeded → Healthy. Stays Healthy
        //     until process restart; a future slice adds decay on
        //     subsequent failures.
        let observed = self.has_observed_success.load(Ordering::Relaxed);
        let (state, available, message) = if observed {
            (
                HealthState::Healthy,
                true,
                Some("airc-remote: ≥1 successful round-trip observed".to_string()),
            )
        } else {
            (
                HealthState::Unhealthy,
                false,
                Some(
                    "airc-remote: no observed round-trip yet — pessimistic until proven \
                     (better to refuse than to route traffic to a possibly-dead peer)"
                        .to_string(),
                ),
            )
        };
        HealthStatus {
            status: state,
            api_available: available,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: 0,
            message,
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // Future slice: discover peer's models via airc handshake.
        Vec::new()
    }

    /// The responder's served slot, once heard (see `learned_window`); `None` until the
    /// first answer — the binding window stands until the seat has spoken for itself.
    fn live_served_window(&self) -> Option<u32> {
        let advertised = self.advertised_window();
        let learned = self.learned_window.load(Ordering::Relaxed);
        if advertised.is_some()
            && (learned == 0
                || advertised != *self.offer_at_feedback.read().expect("remote offer lock")) // A poisoned witness cannot safely authorize another generation.
        {
            return advertised;
        }
        match learned {
            0 => None,
            w => Some(w),
        }
    }

    fn capacity_changed(
        &self,
        observed: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        let mut changes = self.window_changed.subscribe();
        let mut offers = crate::capacity::gossip::global_ledger().subscribe();
        Box::pin(async move {
            loop {
                if self.live_served_window() != observed {
                    return;
                }
                tokio::select! {
                    result = changes.changed() => if result.is_err() { return std::future::pending::<()>().await; },
                    result = offers.changed() => if result.is_err() { return std::future::pending::<()>().await; },
                }
            }
        })
    }

    fn device_type(&self) -> InferenceDevice {
        // From this host's perspective, the actual compute is
        // remote. The substrate's per-tier scheduler treats this
        // as a non-local lane.
        InferenceDevice::Cpu
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // No name-based auto-routing — the substrate's coordinator
        // explicitly selects this adapter when grid routing is
        // desired.
        vec![]
    }

    fn supports_model(&self, model: &str) -> bool {
        // A lane pinned to a model serves ONLY that model — the peer refuses
        // any other after the full wait (#3822 measured it: +186 s). The
        // selector must filter here rather than route on a claim the adapter
        // no longer honours (card 0b55ea79). An unpinned adapter still
        // accepts any name: the peer's own catalog decides.
        self.default_model
            .as_deref()
            .is_none_or(|lane| lane == model)
    }
}

#[cfg(test)]
mod tests;
