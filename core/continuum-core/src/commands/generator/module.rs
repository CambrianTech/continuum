//! `generate/module` — scaffold a fresh `ServiceModule` (mod.rs + types.rs +
//! DESIGN.md + README.md) into the continuum-core modules tree.
//!
//! Dep-holding: the scaffolding logic + the per-name concurrency guard live on the
//! shared [`GeneratorEngine`]; this command is the thin typed surface over it.

use std::sync::Arc;

use crate::modules::generator::types::{GenerateModuleParams, GenerateModuleResult};
use crate::modules::generator::GeneratorEngine;

crate::action_command! {
    /// Scaffold a fresh `ServiceModule` — a compiling `mod.rs` (typed-envelope
    /// dispatch over the declared commands), a `types.rs` (typed Params/Result pairs
    /// with ts-rs exports), a `DESIGN.md` skeleton, and an author-facing `README.md`.
    /// Concurrent calls targeting the same module name serialize (one wins without
    /// `force`; consistent final state with `force`); distinct names stay fully
    /// parallel. Returns the new directory, the files written, and the next manual
    /// wire-up step.
    pub struct GenerateModule { engine: Arc<GeneratorEngine> }
    name: "generate/module",
    access: Privileged,
    params: GenerateModuleParams,
    output: GenerateModuleResult,
    run(this, _ctx, p) => {
        // `generate_module_inner` returns `Result<_, String>`; `?` lifts a String
        // into `CommandError::Internal` (fail loud, naming the cause).
        let result = this.engine.generate_module_inner(&p)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::generator::types::PrioritySpec;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — generate/module writes Rust source into
    // the workspace tree, so it is Privileged, never AiSafe (not a persona toolbelt
    // surface).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GenerateModule::NAME, "generate/module");
        assert!(matches!(
            GenerateModule::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the typed command body runs the shared engine end-to-end —
    // scaffolds a module into a tempdir-rooted engine and returns the populated result
    // (module path + four files + a wire-up next step). Proves the dep-holding object
    // wires to the live engine, not a stub.
    #[tokio::test]
    async fn scaffolds_a_module_via_the_shared_engine() {
        let root = std::env::temp_dir().join(format!(
            "continuum-generate-module-cmd-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).expect("tempdir create");

        let cmd = GenerateModule {
            engine: Arc::new(GeneratorEngine::with_workspace_root(root.clone())),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                GenerateModuleParams {
                    name: "cmd_demo".into(),
                    description: "Generated via the typed generate/module command".into(),
                    commands: vec!["cmd_demo/ping".into()],
                    events_subscribed: vec![],
                    events_published: vec![],
                    priority: PrioritySpec::Normal,
                    force: false,
                    stateful: false,
                },
            )
            .await
            .expect("generate/module must succeed in an empty dir");

        assert_eq!(out.module_path, root.join("cmd_demo"));
        assert_eq!(
            out.files_created.len(),
            4,
            "mod.rs + types.rs + DESIGN.md + README.md"
        );
        assert!(
            out.next_step.contains("pub mod"),
            "next_step prompts the wire-up"
        );
    }
}
