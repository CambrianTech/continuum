//! Intent ownership for the serving daemon.

use super::*;

/// The verdict for force-serving one specific model on this host RIGHT NOW —
/// what [`ServingDaemonModule::pin_fit_checker`] returns and `serving/pin` gates
/// on. Carries the numbers so the command can fail loud with a NAMED shortfall
/// ("needs ~XGB, host budget is ~YGB") rather than a bare refusal.
pub struct PinFit {
    /// The host-fit plan for the model alone. `None` when the model has no GGUF
    /// on disk (not downloaded → not servable). `Some` with `fits_on_gpu = false`
    /// when it is on disk but won't fit a lane in the current budget.
    pub plan: Option<ServingPlan>,
    /// The model's on-disk weight bytes (0 when not downloaded).
    pub weights_bytes: u64,
    /// The host's usable serving budget right now, in bytes.
    pub budget_bytes: u64,
}

/// The cloneable synchronous fit-gate the `serving/pin` command holds — given a
/// candidate [`Model`], can this host force-serve it right now. Built by
/// [`ServingDaemonModule::pin_fit_checker`] over the live budget so it agrees
/// with the autonomic planner by construction.
pub type PinFitChecker = Arc<dyn Fn(&Model) -> PinFit + Send + Sync>;

/// A coherent process-local intent snapshot. Revision is not persisted across boots.
#[derive(Clone, Debug)]
pub struct ServingIntentSnapshot {
    pub revision: u64,
    pub pinned: Option<String>,
    pub suppressed: Arc<HashSet<String>>,
}

/// No fresh revision can be issued; the current intent remains unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentRevisionExhausted;

impl std::fmt::Display for IntentRevisionExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("serving intent revision exhausted; request was not accepted")
    }
}
impl std::error::Error for IntentRevisionExhausted {}

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

    #[cfg(test)]
    pub(crate) fn exhaust_revision_for_test(&self) {
        self.state.send_modify(|state| state.revision = u64::MAX);
    }

    pub fn snapshot(&self) -> ServingIntentSnapshot {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<ServingIntentSnapshot> {
        self.state.subscribe()
    }

    /// Explicit accepted requests count even when reaffirming the current value.
    /// Pressure retries only count when their effective value changes.
    pub fn set_pin(
        &self,
        pinned: Option<String>,
        explicit: bool,
    ) -> Result<Option<String>, IntentRevisionExhausted> {
        let mut exhausted = false;
        let mut previous = None;
        self.state.send_if_modified(|state| {
            previous = state.pinned.clone();
            if !explicit && previous == pinned {
                return false;
            }
            let Some(revision) = state.revision.checked_add(1) else {
                exhausted = true;
                return false;
            };
            state.revision = revision;
            state.pinned = pinned;
            true
        });
        if exhausted {
            Err(IntentRevisionExhausted)
        } else {
            Ok(previous)
        }
    }

    pub fn set_suppressed(
        &self,
        model: &str,
        suppressed: bool,
        explicit: bool,
    ) -> Result<bool, IntentRevisionExhausted> {
        let mut exhausted = false;
        let mut previous = false;
        self.state.send_if_modified(|state| {
            previous = state.suppressed.contains(model);
            if !explicit && previous == suppressed {
                return false;
            }
            let Some(revision) = state.revision.checked_add(1) else {
                exhausted = true;
                return false;
            };
            state.revision = revision;
            if suppressed {
                Arc::make_mut(&mut state.suppressed).insert(model.to_string());
            } else {
                Arc::make_mut(&mut state.suppressed).remove(model);
            }
            true
        });
        if exhausted {
            Err(IntentRevisionExhausted)
        } else {
            Ok(previous)
        }
    }
}

/// Internal provenance for the plan. The command still returns only `plan`.
#[derive(Clone, Debug)]
pub(super) struct ServingPlanSnapshot {
    pub intent_revision: u64,
    pub plan: Option<ServingPlan>,
}
#[cfg(test)]
mod revision_tests {
    use super::*;
    #[test]
    fn exhausted_intent_refuses_changes_without_publication() {
        let intent = ServingIntent::new(Some("held".into()));
        intent.state.send_modify(|s| s.revision = u64::MAX);
        let rx = intent.subscribe();
        assert_eq!(
            intent.set_pin(Some("other".into()), true),
            Err(IntentRevisionExhausted)
        );
        assert_eq!(
            intent.set_suppressed("other", true, true),
            Err(IntentRevisionExhausted)
        );
        let snapshot = intent.snapshot();
        assert_eq!(snapshot.revision, u64::MAX);
        assert_eq!(snapshot.pinned.as_deref(), Some("held"));
        assert!(snapshot.suppressed.is_empty());
        assert!(matches!(rx.has_changed(), Ok(false)));
        assert_eq!(
            intent.set_pin(Some("held".into()), false),
            Ok(Some("held".into()))
        );
        assert_eq!(intent.set_suppressed("other", false, false), Ok(false));
        assert!(matches!(rx.has_changed(), Ok(false)));
    }
    #[test]
    fn last_revision_is_published_once_and_never_reused() {
        let intent = ServingIntent::new(None);
        intent.state.send_modify(|s| s.revision = u64::MAX - 1);
        let mut rx = intent.subscribe();
        assert_eq!(intent.set_suppressed("held", true, true), Ok(false));
        assert!(matches!(rx.has_changed(), Ok(true)));
        assert_eq!(rx.borrow_and_update().revision, u64::MAX);
        assert_eq!(
            intent.set_suppressed("held", false, true),
            Err(IntentRevisionExhausted)
        );
        assert!(intent.snapshot().suppressed.contains("held"));
        assert!(matches!(rx.has_changed(), Ok(false)));
    }
}
