//! Serving intent and the launch verified under it: the revisioned pin/suppression
//! authority (`ServingIntent`) every command and pressure path mutates through, and
//! the `VerifiedServingCapture` receipt — the exact owned launch acknowledged under one
//! intent revision, invalidated the moment the revision moves.

use super::*;

/// A coherent process-local intent snapshot. Revision is not persisted across boots.
#[derive(Clone, Debug)]
pub struct ServingIntentSnapshot {
    pub revision: u64,
    pub pinned: Option<String>,
    pub suppressed: Arc<HashSet<String>>,
}

/// Mutation capability for the serving daemon's single intent watch.
#[derive(Clone)]
pub struct ServingIntent {
    pub(super) state: watch::Sender<ServingIntentSnapshot>,
}

impl ServingIntent {
    pub fn new(pinned: Option<String>) -> Self {
        let (state, _) = watch::channel(ServingIntentSnapshot {
            revision: 0,
            pinned,
            suppressed: Arc::new(HashSet::new()),
        });
        Self { state }
    }

    pub fn snapshot(&self) -> ServingIntentSnapshot {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<ServingIntentSnapshot> {
        self.state.subscribe()
    }

    /// Explicit accepted requests count even when reaffirming the current value.
    /// Pressure retries only count when their effective value changes.
    pub fn set_pin(&self, pinned: Option<String>, explicit: bool) -> Option<String> {
        let mut previous = None;
        self.state.send_if_modified(|state| {
            previous = state.pinned.clone();
            if !explicit && previous == pinned {
                return false;
            }
            state.revision = state
                .revision
                .checked_add(1)
                .expect("serving intent revision exhausted"); // JUSTIFIED: refuse overflow rather than authorize an old revision again.
            state.pinned = pinned;
            true
        });
        previous
    }

    pub fn set_suppressed(&self, model: &str, suppressed: bool, explicit: bool) -> bool {
        let mut previous = false;
        self.state.send_if_modified(|state| {
            previous = state.suppressed.contains(model);
            if !explicit && previous == suppressed {
                return false;
            }
            state.revision = state
                .revision
                .checked_add(1)
                .expect("serving intent revision exhausted"); // JUSTIFIED: refuse overflow rather than authorize an old revision again.
            if suppressed {
                Arc::make_mut(&mut state.suppressed).insert(model.to_string());
            } else {
                Arc::make_mut(&mut state.suppressed).remove(model);
            }
            true
        });
        previous
    }
}

/// Exact local launch provenance acknowledged under one serving intent revision.
/// Eligibility is queried from the daemon; this receipt alone is not a loan or lease.
#[derive(Clone)]
pub struct VerifiedServingCapture {
    pub launch: crate::inference::llama_server::OwnedServingTarget,
    pub intent_revision: u64,
}

pub(super) fn verified_capture(
    server: &dyn LlamaServerControl,
    snapshot: &ServingSnapshot,
    intent_revision: u64,
) -> Option<VerifiedServingCapture> {
    if !snapshot.ready || server.paging_recovery_required() {
        return None;
    }
    let launch = server.owned_serving_target()?;
    if snapshot.active_model.as_deref() != Some(launch.target.model.id.as_str())
        || snapshot.adapters != launch.target.adapter_paths()
        || snapshot.served_context_window != launch.observed_context_window
        || snapshot.lanes != launch.observed_lanes
        || snapshot.host_prompt_cache_mib != launch.launched_host_prompt_cache_mib
        || server.owned_engine()? != launch.identity
    {
        return None;
    }
    Some(VerifiedServingCapture {
        launch,
        intent_revision,
    })
}

impl ServingDaemonModule {
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
}
