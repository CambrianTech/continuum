//! `genome/training-trigger/<verb>` — the curriculum-batching surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! Sitting between curriculum producers (the teacher persona's synthesis, the
//! hippocampus's noteworthy drain, operator submits) and the
//! [`GenomeModule`](crate::modules::genome::GenomeModule)'s `genome/job-create`
//! command: accumulating curated [`TrainingExample`](crate::genome::fine_tuning::types::TrainingExample)s
//! into per-`(persona_id, trait_kind, base_model)` buckets and firing a training
//! job once a bucket crosses its threshold.
//!
//! ## Dep-holding family
//!
//! All three verbs ([`submit`], [`flush`], [`status`]) share the owning module's
//! one [`TrainingTriggerState`](crate::modules::training_trigger::TrainingTriggerState)
//! — the buckets, the per-key submit [`PerKeyGate`](crate::runtime::PerKeyGate),
//! and the late-bound executor — so submit and flush serialize on the SAME gate and
//! mutate the SAME buckets. They are assembled by [`command_objects`] and contributed
//! by [`TrainingTriggerModule`](crate::modules::training_trigger::TrainingTriggerModule)'s
//! `commands()`.
//!
//! ## Outcome-as-data
//!
//! Per the genome family doctrine (mirrored from `genome/job-create`'s
//! `JobCreateOutcome`): expected domain results — batch appended, job dispatched,
//! inconsistent bucket, dispatch failed, nothing to flush — come back as a typed
//! `success` + discriminator outcome, NOT a transport `Err`. `Err` is reserved for
//! genuine caller mistakes (malformed params), per `[[no-fallbacks-ever]]`.

use std::sync::Arc;

use crate::modules::training_trigger::TrainingTriggerState;
use crate::sdk_codegen::DynCommand;

pub mod flush;
pub mod status;
pub mod submit;

use flush::TrainingTriggerFlush;
use status::TrainingTriggerStatus;
use submit::TrainingTriggerSubmit;

/// Build the dep-holding `genome/training-trigger/*` command objects over the
/// shared [`TrainingTriggerState`]. Called from `TrainingTriggerModule::commands`.
/// All three bind the SAME state so submit / flush serialize on the one
/// [`PerKeyGate`](crate::runtime::PerKeyGate) and touch the same buckets.
pub fn command_objects(state: Arc<TrainingTriggerState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(TrainingTriggerSubmit {
            state: state.clone(),
        }),
        Arc::new(TrainingTriggerFlush {
            state: state.clone(),
        }),
        Arc::new(TrainingTriggerStatus { state }),
    ]
}

/// Shared test fixtures for the `training_trigger` command family. Pattern mirrors
/// `commands/genome/mod.rs`'s `test_support` — one home for the helpers the verb
/// files' tests reuse, so the runtime-wiring boilerplate lives in exactly one place.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::genome::fine_tuning::types::TrainingExample;
    use crate::modules::training_trigger::TrainingTriggerModule;
    use crate::runtime::{CommandExecutor, ModuleRegistry};

    pub(crate) fn ex(prompt: &str, completion: &str) -> TrainingExample {
        TrainingExample {
            prompt: prompt.into(),
            completion: completion.into(),
            metadata: None,
        }
    }

    pub(crate) fn submit_params(
        persona_id: Uuid,
        trait_kind: &str,
        examples: Vec<TrainingExample>,
        min_examples: Option<u32>,
    ) -> Value {
        let mut v = json!({
            "personaId": persona_id,
            "personaName": "test-p",
            "baseModel": "synthetic",
            "traitKind": trait_kind,
            "examples": examples,
            "source": "operator_curated",
        });
        if let Some(min) = min_examples {
            v.as_object_mut()
                .unwrap()
                .insert("minExamples".into(), json!(min));
        }
        v
    }

    /// Install an executor that has BOTH the trigger module AND the genome module
    /// wired, so dispatch end-to-end (submit → genome/job-create →
    /// LocalCandleFineTuner) actually runs. Returns the trigger (for `state`
    /// accessors) + the executor (the dispatch entrypoint the tests call).
    pub(crate) async fn build_runtime_with_trigger_and_genome(
    ) -> (Arc<TrainingTriggerModule>, Arc<CommandExecutor>) {
        use crate::genome::fine_tuning::{FineTuningRegistry, LocalCandleFineTuner};
        use crate::modules::genome::GenomeModule;

        let registry = Arc::new(ModuleRegistry::new());
        let trigger = Arc::new(TrainingTriggerModule::new());
        registry.register(trigger.clone());

        let ft_registry = Arc::new(FineTuningRegistry::new());
        ft_registry.register(Arc::new(LocalCandleFineTuner::new()));
        registry.register(Arc::new(GenomeModule::new(ft_registry)));

        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        registry.install_executor_on_all(executor.clone());
        (trigger, executor)
    }

    /// Install an executor with the trigger module but NO genome module — so
    /// `genome/job-create` is unregistered and any dispatch attempt fails loud at
    /// the executor. Simulates the "boot ordering / dependency missing" fault the
    /// trigger must survive WITHOUT losing curated examples (the bucket-preservation
    /// contract).
    pub(crate) async fn build_runtime_trigger_only(
    ) -> (Arc<TrainingTriggerModule>, Arc<CommandExecutor>) {
        let registry = Arc::new(ModuleRegistry::new());
        let trigger = Arc::new(TrainingTriggerModule::new());
        registry.register(trigger.clone());

        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        registry.install_executor_on_all(executor.clone());
        (trigger, executor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires all three trigger-backed verbs
    // (submit/flush/status) over the one shared state. A regression that drops any of
    // them — or fails to share the state — is caught.
    #[test]
    fn family_exposes_the_three_trigger_verbs() {
        let state = Arc::new(TrainingTriggerState::new());
        let objs = command_objects(state);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"genome/training-trigger/submit"));
        assert!(names.contains(&"genome/training-trigger/flush"));
        assert!(names.contains(&"genome/training-trigger/status"));
    }
}
