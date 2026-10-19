//! Serving-edge model binding updates for registered residents.
use super::PersonaWorkspaceRegistry;
use crate::ai::adapter::AIProviderAdapter;
use std::sync::Arc;
use uuid::Uuid;

impl PersonaWorkspaceRegistry {
    /// Re-home EVERY resident persona onto a newly served model — atomically swap
    /// the shared {adapter, model, context_window} binding on each cycle (see
    /// [`crate::cognition::workspace::WorkspaceCycle::rebind_model`]). On a single-serve host ALL personas share
    /// the ONE served model (INFERENCE-LANES-REALISTIC: "one base model, N persona
    /// lanes"), so a served-model change re-homes them together through the ONE
    /// shared `adapter` — no per-persona HTTP init. Each mind's genome + working
    /// memory + admission + hippocampus are UNTOUCHED (flip-in-place, not rebuild):
    /// the same continuous personas now deliberate through the new model. Returns
    /// how many minds were re-homed. Driven by the serving-snapshot reconciler
    /// (`ipc/mod.rs`) ONLY on an actual model change. The store is a wait-free
    /// `ArcSwap` under the cycles lock — NO await held across it.
    /// See [[seamless-persona-failover-model-and-genome]].
    pub(super) fn re_home_all_at(
        &self,
        root: &std::path::Path,
        adapter: Arc<dyn AIProviderAdapter>,
        model: Option<String>,
        context_window: u32,
    ) -> usize {
        // A citizen whose brain is assigned OFF-BOX keeps her remote binding: the
        // local lane coming up is not her model changing (card 1d2f65e7 slice 2).
        let local: std::collections::HashSet<Uuid> = {
            let templates = self.templates.lock();
            templates
                .iter()
                .filter(|(_, cfg)| {
                    match crate::persona::remote_lane_factory::is_remote_bound(root, &cfg.persona_name) {
                        Ok(remote) => !remote,
                        Err(error) => {
                            tracing::error!(persona = %cfg.persona_name, %error, "re-home refused: preserving current binding");
                            false
                        }
                    }
                })
                .map(|(id, _)| *id)
                .collect()
        };
        let cycles = self.cycles.lock();
        let mut rehomed = 0usize;
        for (id, entry) in cycles.iter() {
            if !local.contains(id) {
                crate::probe!(
                    class = "persona.rehome.kept_binding",
                    persona_id = %id,
                    "re-home skipped: local assignment was not established"
                );
                continue;
            }
            entry
                .cycle
                .rebind_model(crate::cognition::llm_deliberation_faculty::ModelBinding {
                    adapter: Arc::clone(&adapter),
                    model: model.clone(),
                    context_window,
                });
            rehomed += 1;
        }
        rehomed
    }
}

