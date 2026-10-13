//! INTENT: the verified serving target, lifecycle admission, and the serving events subscribers read.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

impl ServingDaemonModule {
    /// Emit the live snapshot on the bus. Routed by topic name (cheap match, no
    /// body parse in middleware); the payload fans out as a shared pointer. A
    /// no-op when the bus isn't set (tests). The watch is the in-process
    /// materialized view; THIS is the canonical fan-out that reaches every
    /// subscriber, including a grid allocator on a remote node.
    pub(super) fn emit_serving(bus: Option<&Arc<MessageBus>>, snapshot: &ServingSnapshot) {
        if let Some(bus) = bus {
            if let Ok(payload) = serde_json::to_value(snapshot) { // boundary: the bus event reaches remote subscribers (a grid allocator on another node)
                bus.publish_async_only(SERVING_SNAPSHOT_EVENT, payload);
            }
        }
    }

    /// A current, ready, owned launch, preserving its original requested target.
    /// Unknown adopted origins and superseded/in-flight operations are ineligible.
    pub fn verified_serving_target(&self) -> Option<VerifiedServingCapture> {
        if self.reconciling.load(Ordering::Acquire) {
            return None;
        }
        let capture = self.verified_target.borrow().clone()?;
        if capture.intent_revision != self.intent.snapshot().revision {
            return None;
        }
        let current = verified_capture(
            self.server.as_ref(),
            &self.serving_tx.borrow(),
            capture.intent_revision,
        )?;
        (current.launch.identity == capture.launch.identity).then_some(capture)
    }

    pub(super) fn acknowledge_verified_target(&self, live: &ServingSnapshot, intent_revision: u64) {
        if self.intent.snapshot().revision == intent_revision {
            let _ = self.verified_target.send_replace(verified_capture(
                self.server.as_ref(),
                live,
                intent_revision,
            ));
        }
    }

    /// Admission commits physical unavailability before the first lifecycle await.
    /// Keeping the incumbent/loading identities preserves resource attribution;
    /// cancellation can no longer leave an old ready snapshot blocking recovery.
    pub(super) fn publish_lifecycle_admission(
        serving: &watch::Sender<ServingSnapshot>,
        verified: &watch::Sender<Option<VerifiedServingCapture>>,
        bus: Option<&Arc<MessageBus>>,
        loading: Option<&str>,
    ) {
        let mut snapshot = serving.borrow().clone();
        snapshot.ready = false;
        snapshot.ready_verified_at_ms = None;
        snapshot.loading_model = loading.map(str::to_string);
        snapshot.degraded_reason = Some("serving lifecycle transition admitted".into());
        let _ = verified.send_replace(None);
        Self::emit_serving(bus, &snapshot);
        let _ = serving.send_replace(snapshot);
    }

    /// Subscribe to the published serving SNAPSHOT — the live `(active_model,
    /// ready, base_url)` state. Inference adapters hold this and point at
    /// `base_url` once `ready`; a grid allocator holds it to contract leases.
    pub fn subscribe_serving(&self) -> watch::Receiver<ServingSnapshot> {
        self.serving_tx.subscribe()
    }

    /// The daemon-owned mutation capability; raw watch writers never escape.
    pub fn intent(&self) -> ServingIntent {
        self.intent.clone()
    }
}
