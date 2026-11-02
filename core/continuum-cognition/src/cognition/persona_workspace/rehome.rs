//! Serving-edge model binding updates for registered residents: every LOCAL resident is
//! bound to exactly what serving reports (model AND window), corrected per persona.
use super::PersonaWorkspaceRegistry;
use crate::ai::adapter::AIProviderAdapter;
use crate::cognition::llm_deliberation_faculty::ModelBinding;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

enum Assignment {
    Local,
    Remote,
    Unreadable,
}

/// Why a local resident's binding moves to the served one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RehomeReason {
    /// Serving reports a different model: a new shared adapter is required.
    ModelChanged,
    /// Same model, a different served window (a resident materialized before the
    /// snapshot was ready keeps her provisional window until this corrects it).
    WindowChanged,
}

impl RehomeReason {
    fn of(binding: &ModelBinding, model: &str, context_window: u32) -> Option<Self> {
        if binding.requested_model() != model {
            Some(Self::ModelChanged)
        } else if binding.context_window != context_window {
            Some(Self::WindowChanged)
        } else {
            None
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ModelChanged => "model_changed",
            Self::WindowChanged => "window_changed",
        }
    }
}

impl PersonaWorkspaceRegistry {
    /// The assignment of every registered resident, read once. A citizen whose brain is
    /// assigned OFF-BOX keeps her remote binding: the local lane coming up is not her
    /// model changing (card 1d2f65e7 slice 2).
    fn assignments_at(&self, root: &std::path::Path) -> HashMap<Uuid, Assignment> {
        let templates = self.templates.lock();
        templates
            .iter()
            .map(|(id, cfg)| {
                let assignment = match crate::cognition::extensions::is_remote_bound(root, &cfg.persona_name) {
                    Ok(false) => Assignment::Local,
                    Ok(true) => Assignment::Remote,
                    Err(error) => {
                        tracing::error!(persona = %cfg.persona_name, %error, "re-home refused: preserving current binding");
                        Assignment::Unreadable
                    }
                };
                (*id, assignment)
            })
            .collect()
    }

    /// What the served `(model, context_window)` asks of the local residents: `None` when
    /// every one is already bound to it (the steady-state tick is free),
    /// `Some(ModelChanged)` when any needs the new model (the caller builds ONE shared
    /// adapter), else `Some(WindowChanged)` (each keeps her adapter).
    pub fn rehome_reason_at(
        &self,
        root: &std::path::Path,
        model: &str,
        context_window: u32,
    ) -> Option<RehomeReason> {
        let assignments = self.assignments_at(root);
        let cycles = self.cycles.lock();
        cycles
            .iter()
            .filter(|(id, _)| matches!(assignments.get(*id), Some(Assignment::Local)))
            .filter_map(|(_, entry)| entry.cycle.current_binding())
            .filter_map(|binding| RehomeReason::of(&binding, model, context_window))
            .max_by_key(|reason| *reason == RehomeReason::ModelChanged)
    }

    /// [`rehome_reason_at`](Self::rehome_reason_at) under this process's continuum root.
    pub fn rehome_reason(&self, model: &str, context_window: u32) -> Option<RehomeReason> {
        self.rehome_reason_at(&crate::paths::continuum_root(), model, context_window)
    }

    /// Bind EVERY local resident to the served `(model, context_window)` — atomically
    /// swapping the shared {adapter, model, context_window} binding on each cycle (see
    /// [`crate::cognition::workspace::WorkspaceCycle::rebind_model`]). A resident already
    /// bound to it is untouched. A model change rebinds through the ONE shared `adapter`
    /// (no per-persona HTTP init; INFERENCE-LANES-REALISTIC: one base model, N persona
    /// lanes); a window-only change keeps her own adapter and moves the window. Genome,
    /// working memory, admission and hippocampus are UNTOUCHED (flip-in-place). Returns
    /// how many minds were rebound. Driven by the serving-snapshot reconciler on every
    /// ready snapshot. The store is a wait-free `ArcSwap` under the cycles lock — NO
    /// await held across it. See [[seamless-persona-failover-model-and-genome]].
    pub fn re_home_all_at(
        &self,
        root: &std::path::Path,
        adapter: Option<Arc<dyn AIProviderAdapter>>,
        model: &str,
        context_window: u32,
    ) -> usize {
        let assignments = self.assignments_at(root);
        let cycles = self.cycles.lock();
        let mut rehomed = 0usize;
        for (id, entry) in cycles.iter() {
            let kept = match assignments.get(id) {
                Some(Assignment::Local) => None,
                Some(Assignment::Remote) => Some("remote"),
                Some(Assignment::Unreadable) => Some("unreadable"),
                None => Some("missing_template"),
            };
            if let Some(reason) = kept {
                continuum_base::probe!(
                    class = "persona.rehome.kept_binding",
                    persona_id = %id,
                    reason,
                    "re-home skipped: local assignment was not established"
                );
                continue;
            }
            let Some(current) = entry.cycle.current_binding() else {
                continue; // no deliberation faculty shared a binding in: nothing to rebind.
            };
            let Some(reason) = RehomeReason::of(&current, model, context_window) else {
                continue;
            };
            let next_adapter = match (reason, &adapter) {
                (RehomeReason::ModelChanged, Some(shared)) => Arc::clone(shared),
                (RehomeReason::ModelChanged, None) => {
                    continuum_base::probe!(
                        class = "persona.rehome.kept_binding",
                        persona_id = %id,
                        reason = "model_changed_without_adapter",
                        "re-home skipped: the served model changed after the adapter decision; the next tick rebuilds it"
                    );
                    continue;
                }
                (RehomeReason::WindowChanged, _) => Arc::clone(&current.adapter),
            };
            continuum_base::probe!(
                class = "persona.rehome",
                persona_id = %id,
                reason = reason.as_str(),
                old_model = %current.requested_model(),
                new_model = %model,
                old_window = current.context_window,
                new_window = context_window,
                "resident rebound to the served binding"
            );
            entry.cycle.rebind_model(ModelBinding {
                adapter: next_adapter,
                model: Some(model.to_string()),
                context_window,
            });
            rehomed += 1;
        }
        rehomed
    }

    /// [`re_home_all_at`](Self::re_home_all_at) under this process's continuum root.
    pub fn re_home_all(
        &self,
        adapter: Option<Arc<dyn AIProviderAdapter>>,
        model: &str,
        context_window: u32,
    ) -> usize {
        self.re_home_all_at(
            &crate::paths::continuum_root(),
            adapter,
            model,
            context_window,
        )
    }
}
