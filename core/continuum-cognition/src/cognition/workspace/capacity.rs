/// The required prompt input cannot be represented in this window.
/// This assembly was refused, not a failed inference or chosen silence.
/// Earlier receipts, if any, still describe work that actually happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptCapacityError {
    pub required_tokens: usize,
    pub budget_tokens: usize,
}

impl std::fmt::Display for PromptCapacityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "complete prompt input requires {} tokens, but only {} are available",
            self.required_tokens, self.budget_tokens
        )
    }
}

/// The refused assembly and the exact binding whose capacity was observed.
/// The readiness handle owns no task or scheduler; it is awaited by the existing
/// service loop and cancelled with that loop's other wake branches.
#[derive(Clone)]
pub struct CapacityRefusal {
    pub id: uuid::Uuid,
    receipt_state: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pub error: PromptCapacityError,
    pub readiness: Option<std::sync::Arc<CapacityReadiness>>,
}
impl From<PromptCapacityError> for CapacityRefusal {
    fn from(error: PromptCapacityError) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            receipt_state: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            error,
            readiness: None,
        }
    }
}
/// One park and one disposition per refusal, shared across settlement projections.
/// Readiness checks emit nothing; these receipts describe actual custody transitions.
impl CapacityRefusal {
    pub fn note_transition(&self, persona: Option<uuid::Uuid>, room: uuid::Uuid,
        phase: &'static str, reason: &'static str) -> bool {
        let bit = if phase == "parked" { 1 } else { 2 };
        if self.receipt_state.fetch_or(bit, std::sync::atomic::Ordering::Relaxed) & bit != 0 {
            return false;
        }
        // A loop that stops with this refusal parked asks nothing more: never a stall.
        if let (Some(persona), "stopped") = (persona, phase) {
            crate::cognition::resident_service::unpark(persona);
        }
        continuum_base::probe!(class = "persona.capacity.transition",
            persona = ?persona, room = %room, refusal = %self.id,
            phase = phase, reason = reason,
            required_tokens = self.error.required_tokens, budget_tokens = self.error.budget_tokens,
            "prompt capacity custody changed");
        true
    }
}
impl std::fmt::Debug for CapacityRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
impl std::fmt::Display for CapacityRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::ops::Deref for CapacityRefusal {
    type Target = PromptCapacityError;
    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

#[derive(Clone)]
pub struct CapacityReadiness {
    persona: uuid::Uuid,
    handle: crate::cognition::model_binding::ModelBindingHandle,
    observed: std::sync::Arc<crate::cognition::model_binding::ModelBinding>,
    target: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    window: u32,
    dynamic_window: bool,
    fitted_window: Option<u32>,
}
impl CapacityReadiness {
    /// The readiness of `persona`'s turn at `window`, the window [`mind_window`] decided for her
    /// (the caller waits out an ungranted window before building a turn, so there is always one).
    ///
    /// [`mind_window`]: crate::capacity::mind_window::mind_window
    pub fn new(
        persona: uuid::Uuid,
        handle: crate::cognition::model_binding::ModelBindingHandle,
        observed: std::sync::Arc<crate::cognition::model_binding::ModelBinding>,
        window: u32,
    ) -> Self {
        let target = observed
            .adapter
            .generation_target()
            .unwrap_or_else(|| observed.adapter.clone()); // No forwarding target means the bound adapter itself serves.
        // A granted or lane-reported window moves on its own; a bound one moves only with the binding.
        let dynamic_window = !matches!(
            crate::capacity::mind_window::mind_window(persona, observed.adapter.as_ref(), observed.context_window),
            crate::capacity::mind_window::MindWindow::Decided(crate::capacity::mind_window::Decided::Bound(_))
        );
        Self {
            persona,
            handle,
            observed,
            target,
            window,
            dynamic_window,
            fitted_window: None,
        }
    }
    pub fn observed_window(&self) -> u32 {
        self.window
    }
    /// Keep the dispatch observation immutable when provider feedback narrows
    /// the fitted view. Its unchanged, stale advertisement is not new capacity.
    pub fn with_fit_window(mut self, window: u32) -> Self {
        self.fitted_window = (window < self.window).then_some(window);
        self
    }
    fn window_changed(&self, current: u32) -> bool {
        self.fitted_window.map_or(current > self.window, |available| {
            current != self.window && current > available
        })
    }
    pub fn changed(&self) -> bool {
        self.change_reason().is_some()
    }
    pub fn change_reason(&self) -> Option<&'static str> {
        let current = self.handle.load();
        if !std::sync::Arc::ptr_eq(&self.observed.adapter, &current.adapter)
            || self.observed.requested_model() != current.requested_model() {
            return Some("binding");
        }
        let target = current.adapter.generation_target()
            .unwrap_or_else(|| current.adapter.clone()); // Direct adapters serve their own generations.
        if !std::sync::Arc::ptr_eq(&self.target, &target) {
            return Some("placement");
        }
        // Her window as the one determiner decides it now: ungranted is a change (her turn waits),
        // a bound window counts only when this observation was bound too.
        let now = crate::capacity::mind_window::mind_window(self.persona, current.adapter.as_ref(), current.context_window);
        match now {
            crate::capacity::mind_window::MindWindow::Ungranted => true,
            crate::capacity::mind_window::MindWindow::Decided(crate::capacity::mind_window::Decided::Bound(window)) => {
                !self.dynamic_window && self.window_changed(window)
            }
            crate::capacity::mind_window::MindWindow::Decided(decided) => self.window_changed(decided.tokens()),
        }
        .then_some("window")
    }
    pub async fn wait(&self) {
        let mut binding_changes = self.handle.subscribe();
        loop {
            let current = self.handle.load_full();
            let window = current.adapter.live_served_window();
            let adapter_change = current.adapter.capacity_changed(window);
            // A mind on the shared pool's window is her grant: a new grant is a capacity change. With no
            // grants watch in this process there is no such change to wait for, so that arm never
            // fires (a refused watch resolving at once would spin this loop; the turn's own
            // decide_window is where a missing watch is refused).
            // A mind on the shared pool's window is her grant: a NEWER grant is a capacity change.
            // The waiter is taken before the check below and marks the current grants seen, so it
            // parks on unchanged grants (a clone that did not spun this loop: 339% CPU on the M5).
            // With no grants watch there is no such change, and the arm never fires.
            let waiter = crate::capacity::mind_window::grants_waiter();
            let grants_change = async move {
                match waiter {
                    Ok(waiter) if waiter.changed().await.is_ok() => {}
                    _ => std::future::pending::<()>().await,
                }
            };
            if self.changed() {
                return;
            }
            // Adapter implementations subscribe then recheck this observation.
            // Binding changes were subscribed before the semantic read above.
            tokio::select! {
                result = binding_changes.changed() => {
                    if result.is_err() { return std::future::pending::<()>().await; }
                }
                _ = adapter_change => {}
                _ = grants_change => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A readiness at the window the one determiner decides for this binding (a test adapter is
    /// never on the shared pool, so it is always decided).
    fn readiness(handle: &crate::cognition::model_binding::ModelBindingHandle) -> CapacityReadiness {
        let persona = uuid::Uuid::from_u128(1);
        let observed = handle.load_full();
        let crate::capacity::mind_window::MindWindow::Decided(decided) =
            crate::capacity::mind_window::mind_window(persona, observed.adapter.as_ref(), observed.context_window)
        else {
            panic!("a test adapter is never on the shared pool")
        };
        let window = decided.tokens();
        CapacityReadiness::new(persona, handle.clone(), observed, window)
    }
    use crate::cognition::model_binding::{ModelBinding, ModelBindingState};
    use std::sync::Arc;

    #[test]
    fn capacity_receipts_are_once_per_refusal_across_cloned_projections() {
        let refusal = CapacityRefusal::from(PromptCapacityError { required_tokens: 40, budget_tokens: 20 });
        let projection = refusal.clone();
        let room = uuid::Uuid::new_v4();
        assert!(refusal.note_transition(None, room, "parked", "capacity"));
        assert!(!projection.note_transition(None, room, "parked", "capacity"));
        assert!(projection.note_transition(None, room, "resumed", "placement"));
        assert!(!refusal.note_transition(None, room, "resumed", "input"));
        assert!(!refusal.note_transition(None, room, "stopped", "stop"));
        let next = CapacityRefusal::from(refusal.error);
        assert_ne!(next.id, refusal.id);
        assert!(next.note_transition(None, room, "parked", "capacity"));
        assert!(next.note_transition(None, room, "stopped", "stop"));
    }

    // Independent parked consumers see publication even before their future is
    // first polled; rebinding equal facts never admits another generation.
    #[tokio::test(start_paused = true)]
    async fn semantic_binding_growth_wakes_all_parked_consumers() {
        let adapter = Arc::new(crate::ai::HeuristicInferenceAdapter::new());
        let handle = Arc::new(ModelBindingState::new(ModelBinding {
            adapter: adapter.clone(),
            model: Some("bound".into()),
            context_window: 4096,
        }));
        let one = readiness(&handle);
        let two = readiness(&handle);
        handle.store(Arc::new(ModelBinding {
            adapter: adapter.clone(),
            model: Some("bound".into()),
            context_window: 4096,
        }));
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), one.wait())
                .await
                .is_err()
        );
        let first = one.wait();
        let second = two.wait();
        handle.store(Arc::new(ModelBinding {
            adapter,
            model: Some("bound".into()),
            context_window: 8192,
        }));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(first, second);
        })
        .await
        .expect("both subscribers observe growth, including pre-poll publication");
    }
    // what this catches (#5752's first window: 199% CPU for 37 minutes): a readiness wait that
    // wakes without a change and spins inside one poll. An unchanged wait parks: its first poll
    // returns Pending. Run on its own thread so a spin fails this test in seconds, never hangs CI.
    #[test]
    fn an_unchanged_capacity_wait_parks_and_never_spins() {
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("test runtime"); // a test runtime always builds
            runtime.block_on(async {
                let adapter = Arc::new(crate::ai::HeuristicInferenceAdapter::new());
                let handle = Arc::new(ModelBindingState::new(ModelBinding {
                    adapter, model: Some("bound".into()), context_window: 32768,
                }));
                let unchanged = readiness(&handle);
                let mut wait = Box::pin(unchanged.wait());
                let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
                let parked = std::future::Future::poll(wait.as_mut(), &mut cx).is_pending();
                let _ = sent.send(parked); // the receiver may have timed out already; nothing to report then
            });
        });
        assert_eq!(
            received.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(true),
            "an unchanged wait parks on its first poll; no answer in 5 s means it spins"
        );
    }
    #[tokio::test(start_paused = true)]
    async fn refit_waits_for_changed_capacity_above_provider_bound() {
        let adapter = Arc::new(crate::ai::HeuristicInferenceAdapter::new());
        let handle = Arc::new(ModelBindingState::new(ModelBinding {
            adapter: adapter.clone(), model: Some("bound".into()), context_window: 32768,
        }));
        let refused = readiness(&handle)
            .with_fit_window(16384);
        assert!(tokio::time::timeout(std::time::Duration::from_secs(1), refused.wait()).await.is_err(),
            "unchanged stale nominal capacity is not growth");
        handle.store(Arc::new(ModelBinding {
            adapter, model: Some("bound".into()), context_window: 24576,
        }));
        tokio::time::timeout(std::time::Duration::from_secs(1), refused.wait()).await.unwrap();
        let refused_again = readiness(&handle)
            .with_fit_window(16384);
        assert!(tokio::time::timeout(std::time::Duration::from_secs(1), refused_again.wait()).await.is_err(),
            "a fresh refusal does not retry the same changed advertisement again");
    }

}
