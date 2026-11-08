#![cfg(test)]
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn bootstrap_resolves_home_under_personas_directory() {
        // This unit test verifies path layout only — actual daemon
        // attach lives in an integration test that needs a running
        // airc daemon. The layout assertion catches the
        // [[personas-are-citizens-airc-is-identity-provider]] memory's
        // "do NOT nest persona homes inside another scope's airc home"
        // rule at compile-ish time.
        let temp = TempDir::new().expect("tempdir");
        let expected_home = crate::identity::citizen_path::citizen_home_path(
            temp.path(),
            crate::identity::IdentityKind::Persona,
            None,
            "helper-ai-test",
        );
        // Slice 4 of #142: symmetric layout under
        // citizens/personas/<name>/airc/.
        assert_eq!(
            expected_home,
            temp.path()
                .join("citizens")
                .join("personas")
                .join("helper-ai-test")
                .join("airc")
        );
        assert!(!expected_home.exists());
    }

    /// Slice 4 of #142 migration safety claim: when bootstrap is
    /// called for a persona whose pre-Slice-4 home
    /// (`<root>/personas/<name>/airc/`) exists on disk, the
    /// substrate REFUSES to silently use it and returns
    /// `LegacyLayoutDetected` with the exact `mv` paths. This test
    /// fires the detection branch via a TempDir-seeded legacy path.
    ///
    /// Per [[no-fallbacks-ever]] + [[every-error-is-an-opportunity-
    /// to-battle-harden]]: the message is load-bearing safety; this
    /// is the rigging that catches a regression flipping the path
    /// components.
    #[tokio::test]
    async fn legacy_persona_layout_hard_errors_with_mv_command() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();

        // Seed the pre-Slice-4 legacy path for a persona named "maya".
        let legacy = root.join("personas").join("maya").join("airc");
        tokio::fs::create_dir_all(&legacy)
            .await
            .expect("seed legacy persona path");

        // Bootstrap a persona named "maya". The legacy detection
        // fires BEFORE airc-lib attach, so the test doesn't need a
        // running daemon.
        // Dummy executor — bootstrap fires the legacy-path detection
        // BEFORE pump install, so the executor is never touched.
        // Registry audit (mind cut): stays in the mind: the executor is never touched.
        let dummy_registry = std::sync::Arc::new(crate::runtime::ModuleRegistry::new());
        let dummy_executor = std::sync::Arc::new(
            crate::runtime::command_executor::CommandExecutor::new(dummy_registry),
        );

        let result = PersonaAircRuntime::bootstrap(
            Uuid::new_v4(),
            "maya",
            root,
            PathBuf::from("/nonexistent/daemon.sock"),
            crate::persona::identity_provider::PersonaIdentitySource::FreshlyMinted,
            dummy_executor,
        )
        .await;

        // `PersonaAircRuntime` doesn't impl Debug (Arc<dyn ...>
        // fields), so we destructure without {:?} on the Ok variant.
        match result {
            Err(PersonaAircRuntimeError::LegacyLayoutDetected {
                legacy: l,
                new,
                new_parent,
            }) => {
                assert_eq!(l, legacy);
                assert_eq!(
                    new,
                    root.join("citizens")
                        .join("personas")
                        .join("maya")
                        .join("airc")
                );
                assert_eq!(
                    new_parent,
                    root.join("citizens").join("personas").join("maya")
                );
            }
            Err(other) => panic!(
                "expected LegacyLayoutDetected, got different error: {other} \
                 — legacy detection is broken"
            ),
            Ok(_) => panic!(
                "expected LegacyLayoutDetected, bootstrap succeeded — \
                 legacy detection is broken"
            ),
        }
    }
