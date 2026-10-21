#![cfg(test)]
    use super::*;
    use tempfile::TempDir;

    // what this catches (card ed2c9417, Kimi 2026-10-06): a claim she holds on a room she
    // does not subscribe to, renewed by the followed path but invisible to the work gate,
    // so she read no_held_work and made a new review card every turn. Only a record whose
    // room she did NOT walk, and whose card the walk did not find, is read on its own board.
    #[test]
    fn a_followed_claim_outside_her_rooms_joins_the_owned_walk_once() {
        let home = TempDir::new().expect("tempdir");
        let room = |name: &str| airc_lib::Room::from_name(home.path(), name).expect("a room");
        let claim = |room_name: &str, card: u128| crate::persona::held_claims::HeldClaim {
            room: room(room_name),
            card_id: uuid::Uuid::from_u128(card),
            claim_id: uuid::Uuid::from_u128(card + 0x1000),
            recorded_at_ms: 1,
            refusals: 0,
        };
        let walked_rooms = vec![room("academy").channel.as_uuid()];
        let recorded = vec![
            claim("cambriantech", 1), // her review card's room, not walked: read it
            claim("academy", 2),      // a walked room: the walk already saw it
            claim("cambriantech", 3), // not walked, but the walk found the card: no second read
        ];
        let walked_cards = vec![uuid::Uuid::from_u128(3)];
        let beyond = followed_beyond_rooms(&walked_rooms, &walked_cards, &recorded);
        assert_eq!(beyond.len(), 1, "{beyond:?}");
        assert_eq!(beyond[0].card_id, uuid::Uuid::from_u128(1));
    }

    #[tokio::test]
    async fn bootstrap_resolves_home_under_personas_directory() {
        // This unit test verifies path layout only — actual daemon
        // attach lives in an integration test that needs a running
        // airc daemon. The layout assertion catches the
        // [[personas-are-citizens-airc-is-identity-provider]] memory's
        // "do NOT nest persona homes inside another scope's airc home"
        // rule at compile-ish time.
        let temp = TempDir::new().expect("tempdir");
        let expected_home = crate::context::citizen_home_path(
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
