//! `genome/training-trigger/<verb>` — the curriculum journal's surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! Accepting curated [`TrainingExample`](crate::genome::fine_tuning::types::TrainingExample)s
//! from curriculum producers (the teacher persona's synthesis, the hippocampus's noteworthy
//! drain, settled cards, operator submits) into the durable journal, per
//! `(persona_id, trait_kind, base_model)` bucket, where the training trigger learns them in
//! place; retiring submissions not yet learned; re-keying a bucket stored under a base spelling
//! that is not the catalog id; carrying a bucket's examples not learned yet to the base she runs;
//! and reporting each bucket's journal.
//!
//! ## Dep-holding family
//!
//! The five verbs ([`submit`], [`retire`], [`rekey`], [`transfer`], [`status`]) share the owning module's one
//! [`TrainingTriggerState`](crate::modules::training_trigger::TrainingTriggerState), so a
//! submit, a retirement and a learning pass on one bucket serialize on the same
//! [`PerKeyGate`](crate::runtime::PerKeyGate). They are assembled by [`command_objects`] and
//! contributed by [`TrainingTriggerModule`](crate::modules::training_trigger::TrainingTriggerModule)'s
//! `commands()`.
//!
//! ## Outcome-as-data
//!
//! Expected domain results come back as a typed `success` + discriminator outcome, NOT a
//! transport `Err`. `Err` is reserved for genuine caller mistakes (malformed params), per
//! `[[no-fallbacks-ever]]`.

use std::sync::Arc;

use crate::modules::training_trigger::TrainingTriggerState;
use crate::sdk_codegen::DynCommand;

pub mod rekey;
pub mod retire;
pub mod status;
pub mod submit;
pub mod transfer;

use rekey::TrainingTriggerRekey;
use retire::TrainingTriggerRetire;
use status::TrainingTriggerStatus;
use submit::TrainingTriggerSubmit;
use transfer::TrainingTriggerTransfer;

/// Build the dep-holding `genome/training-trigger/*` command objects over the
/// shared [`TrainingTriggerState`]. Called from `TrainingTriggerModule::commands`.
/// All of them bind the SAME state, so they serialize on the one
/// [`PerKeyGate`](crate::runtime::PerKeyGate) per bucket.
pub fn command_objects(state: Arc<TrainingTriggerState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(TrainingTriggerSubmit {
            state: state.clone(),
        }),
        Arc::new(TrainingTriggerRetire {
            state: state.clone(),
        }),
        Arc::new(TrainingTriggerRekey {
            state: state.clone(),
        }),
        Arc::new(TrainingTriggerTransfer {
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
            lived: None,
        }
    }

    pub(crate) fn submit_params(
        persona_id: Uuid,
        trait_kind: &str,
        examples: Vec<TrainingExample>,
    ) -> Value {
        json!({
            "personaId": persona_id,
            "personaName": "test-p",
            "baseModel": "synthetic",
            "traitKind": trait_kind,
            "examples": examples,
            "source": "operator_curated",
        })
    }

    /// An HF spelling of `fixture-base`, so a submit can name it another way and land in its bucket.
    pub(crate) const FIXTURE_HF: &str = "fixture/base-repo";
    /// The HF repo two fixture rows share, so a submit naming it is refused as ambiguous.
    pub(crate) const SHARED_HF: &str = "fixture/shared-base";

    /// The bases this binary's tests submit, as a live model catalog every trigger fixture is built
    /// on: submit resolves each base against it, so each fixture base needs a row. The live-engine
    /// fixture's base (`CONTINUUM_LEARN_BASE`) is one when set. Rows carry no artifact hint or local
    /// file, so resolution never reads this machine's models.
    pub(crate) fn fixture_catalog() -> Arc<crate::model_registry::live::ModelCatalog> {
        let template = crate::model_registry::catalog::models().into_iter().next().expect("test: the catalog has a row"); // test fixture
        let row = |id: &str, hf: Option<&str>| {
            let mut row = template.clone();
            row.id = id.to_owned();
            row.hf_source = hf.map(str::to_owned);
            row.gguf_hint = None;
            row.gguf_local_path = None;
            row.mmproj_local_path = None;
            row
        };
        let mut rows = vec![
            row("synthetic", None),
            row("qwen", None),
            row("test", None),
            row("fixture-base", Some(FIXTURE_HF)),
            row("fixture-twin-a", Some(SHARED_HF)),
            row("fixture-twin-b", Some(SHARED_HF)),
        ];
        if let Ok(live) = std::env::var("CONTINUUM_LEARN_BASE") {
            rows.push(row(&live, None));
        }
        let registry = crate::model_registry::Registry::from_catalog(rows, crate::model_registry::catalog::providers()).expect("test: the fixture rows load"); // test fixture
        Arc::new(crate::model_registry::live::ModelCatalog::from_registry(&registry))
    }

    pub(crate) async fn build_runtime_trigger_only() -> (
        Arc<TrainingTriggerModule>,
        Arc<CommandExecutor>,
        tempfile::TempDir,
    ) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter).await;
        (trigger, executor, dir)
    }

    /// The actual owner and command route, with a caller-owned durable fixture so tests can
    /// recreate the owner against the same database after a restart.
    pub(crate) async fn build_runtime(adapter: Arc<dyn crate::orm::StorageAdapter>) -> (Arc<TrainingTriggerModule>, Arc<CommandExecutor>) {
        build_runtime_under(adapter, crate::genome::learner::seat::Authority::isolated(1 << 30)).await
    }

    /// [`build_runtime`] answering to `authority`: a live engine's fixture leases what it really holds.
    pub(crate) async fn build_runtime_under(
        adapter: Arc<dyn crate::orm::StorageAdapter>,
        authority: crate::genome::learner::seat::Authority,
    ) -> (Arc<TrainingTriggerModule>, Arc<CommandExecutor>) {
        let registry = Arc::new(ModuleRegistry::new());
        let trigger = Arc::new(TrainingTriggerModule::new(authority, &fixture_catalog()));
        trigger.state.initialize_storage(adapter).await.unwrap();
        registry.register(trigger.clone());
        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        registry.install_executor_on_all(executor.clone());
        (trigger, executor)
    }

    /// The family's own check, nested in the file's one test module (one `#[cfg(test)] mod` per file).
    mod family {
        use super::super::*;
        use super::fixture_catalog;

        // what this catches: the dep-holding family wires its trigger-backed verbs
        // (submit/retire/rekey/transfer/status) over the one shared state. A regression that drops any of
        // them — or fails to share the state — is caught.
        #[tokio::test]
        async fn family_exposes_the_trigger_verbs() {
            let state = Arc::new(TrainingTriggerState::new(crate::genome::learner::seat::Authority::isolated(1 << 30), &fixture_catalog()));
            let objs = command_objects(state);
            let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
            assert_eq!(names.len(), 5);
            assert!(names.contains(&"genome/training-trigger/submit"));
            assert!(names.contains(&"genome/training-trigger/retire"));
            assert!(names.contains(&"genome/training-trigger/rekey"));
            assert!(names.contains(&"genome/training-trigger/transfer"));
            assert!(names.contains(&"genome/training-trigger/status"));
        }
    }
}
