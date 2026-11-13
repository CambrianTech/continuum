#![cfg(test)]
    use super::*;
    use crate::persona::instance_manager::PersonaInstanceInfo;
    use crate::persona::airc_citizen::StubAircCitizen;
    use crate::persona::hw_tier_descriptor::HwTierCategory;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
    use crate::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;
    use std::path::PathBuf;
    use uuid::Uuid;

    // Bespoke `FakeAdapter` / `OkFactory` / `ErrFactory` /
    // `WarmupFailingFactory` / `WarmupFailingAdapter` deleted per
    // [[test-fixtures-are-system-primitives]] — every test below
    // leases `ScriptedPersonaAdapterFactory` (built on the
    // production-runnable `HeuristicInferenceAdapter`) plus
    // `StubAircCitizen::fresh_lookup`. Adapter behaviors (warmup
    // success/failure, factory rejection, counter observation) come
    // from the system primitives' builder methods.

    fn fake_instance(name: &str) -> PersonaInstanceInfo {
        // Honor the Slice-1B-of-#142 invariant
        // (persona_id == peer_id) even in test fixtures so they
        // exercise the same identity shape production sees. Per the
        // PersonaInstanceInfo doc: fixtures that bypass the runtime
        // constructor MUST keep both fields equal.
        // Each fixture its OWN home (card ee7753e7): materialize_adapters opens the persona's
        // admission store before the factory, and a fixed per-name path let parallel tests
        // race one store, refusing a seat at random (seen on #5413: build_count 0, not 1).
        let peer_id = crate::identity::PeerId::new();
        let home = std::env::temp_dir().join("fake-supervisor-test").join(format!("{name}-{}", peer_id.as_uuid()));
        PersonaInstanceInfo {
            agent_name: name.to_string(),
            peer_id,
            home,
            default_room: Uuid::nil(),
            source: PersonaIdentitySource::FreshlyMinted,
        }
    }

    /// what this catches: the Slice-1B identity projection. The
    /// pre-Slice-1B divergence (a separate `persona_id` Uuid vs the
    /// airc `peer_id`) was the regression class PR #1522's reviewer
    /// caught after the fact. Step 4b made that class IMPOSSIBLE by
    /// collapsing the twin fields into the single canonical
    /// [`crate::identity::PeerId`] — so the remaining seam this pins is
    /// [`PersonaInstanceInfo::persona_identity`] projecting that one
    /// `peer_id` down to the bare `Uuid` the `(id, name)` dispatch pair
    /// carries (the registry key path). If that projection ever stopped
    /// reading `peer_id`, dispatch would key on the wrong id.
    #[test]
    fn persona_identity_projects_canonical_peer_id() {
        let instance = fake_instance("Maya");
        let dispatch = instance.persona_identity();
        assert_eq!(
            dispatch.id,
            instance.peer_id.as_uuid(),
            "persona_identity() must project the canonical peer_id into the dispatch (id, name) pair"
        );
    }

    fn fake_profile(persona_name: &str, model_id: &str) -> PersonaInferenceProfile {
        PersonaInferenceProfile {
            persona_id: Uuid::new_v4(),
            persona_name: persona_name.to_string(),
            model_id: model_id.to_string(),
            gguf_local_path: Some(PathBuf::from("/tmp/fake.gguf")),
            tier_category: HwTierCategory::Compat,
            tier_id: "mac_intel_metal_discrete".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 512,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        }
    }

    /// Idempotent init of the process-global model registry.
    ///
    /// what this catches: every test that builds a *successful* HostedPersona
    /// reaches `model_registry::global()`, which panics if `init_global()` was
    /// never called. In a full-suite run some other test inits it first, so
    /// these passed by accident; run in isolation (or single-threaded) they
    /// panicked on the ordering. Calling this idempotent init at the top of
    /// each such test removes the cross-test ordering dependency.
    fn init_test_registry() {
        let _ = crate::model_registry::init_global();
    }

    /// Happy path: two materialized plans → two hosted personas. Each
    /// adapter's `provider_id` matches the profile's model_id, proving
    /// the factory ran once per persona (not once with shared state).
    #[tokio::test]
    async fn materializes_one_adapter_per_persona_via_factory() {
        init_test_registry();
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 2);
        assert_eq!(factory.build_count(), 2);

        let helper = hosted[0].as_ref().expect("Helper hosted");
        assert_eq!(helper.role, RoleId::Helper);
        assert_eq!(helper.identity.agent_name, "Paige");
        assert_eq!(
            helper.adapter.provider_id(),
            crate::ai::HEURISTIC_PROVIDER_ID
        );

        let coder = hosted[1].as_ref().expect("Coder hosted");
        assert_eq!(coder.role, RoleId::Coder);
        assert_eq!(coder.identity.agent_name, "Pax");
        assert_eq!(
            coder.adapter.provider_id(),
            crate::ai::HEURISTIC_PROVIDER_ID
        );
    }

    /// A row that arrives with `Err(profile)` from slice 8 passes
    /// through as `SupervisorError::Profile` — the factory is NOT
    /// called for it (sibling rows still materialize normally).
    // what this catches: the DECISION, not the serialization. A remote-bound
    // persona's window comes from her recorded responder slot (or the floor when
    // none is recorded) and never from the local serving snapshot — reordering
    // the remote branch against the local-slot clamp would size every off-box
    // brain to a lane it never runs in (35–43k prompts at a 24,832 slot, 2026-09-07).
    #[tokio::test]
    async fn a_remote_override_wins_over_the_local_slot_at_spawn() {
        init_test_registry();
        let homes = tempfile::tempdir().expect("tempdir");
        let mut with_window = fake_instance("Rin");
        with_window.home = homes.path().join("rin");
        std::fs::create_dir_all(&with_window.home).expect("home dir");
        crate::persona::model_override::PersonaModelOverride::new_remote(
            "model-a", None, 1, "peer",
        )
        .with_context_window(24_832)
        .write(&crate::cognition::home::PersonaHome::from_root(
            with_window.home.clone(),
        ))
        .expect("write override");
        let mut without_window = fake_instance("Sol");
        without_window.home = homes.path().join("sol");
        std::fs::create_dir_all(&without_window.home).expect("home dir");
        crate::persona::model_override::PersonaModelOverride::new_remote(
            "model-b", None, 1, "peer",
        )
        .write(&crate::cognition::home::PersonaHome::from_root(
            without_window.home.clone(),
        ))
        .expect("write override");
        let mut rin = fake_profile("Rin", "model-a");
        rin.context_length = 50_944;
        let mut sol = fake_profile("Sol", "model-b");
        sol.context_length = 50_944;
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: with_window,
                profile: Ok(rin),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: without_window,
                profile: Ok(sol),
            },
        ];
        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;
        let rin = hosted[0].as_ref().expect("Rin hosted");
        assert_eq!(
            rin.profile.context_length, 24_832,
            "the recorded responder window wins"
        );
        let sol = hosted[1].as_ref().expect("Sol hosted");
        assert_eq!(
            sol.profile.context_length,
            crate::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET,
            "unknown responder → the planner's floor, never the local lane"
        );
    }

    #[tokio::test]
    async fn forwards_profile_errors_without_calling_factory() {
        init_test_registry();
        let bad_profile_err = InferenceProfileError::UnknownModel {
            model_id: "nonexistent/sentinel".to_string(),
            role_id: "coder".to_string(),
        };
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Err(bad_profile_err),
            },
        ];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 2);
        // Factory called exactly once — for the Ok row only.
        assert_eq!(factory.build_count(), 1);
        assert!(hosted[0].is_ok(), "Helper still materializes");
        match &hosted[1] {
            Err(SupervisorError::Profile {
                slot_index,
                role,
                source,
            }) => {
                assert_eq!(*slot_index, 1);
                assert_eq!(*role, RoleId::Coder);
                assert!(matches!(source, InferenceProfileError::UnknownModel { .. }));
            }
            Err(other) => panic!("expected Profile error at slot 1, got {other:?}"),
            Ok(_) => panic!("expected Profile error at slot 1, got Ok"),
        }
    }

    /// Factory rejection surfaces as `SupervisorError::AdapterFactory`
    /// with the slot index + role tagged. Sibling rows don't get
    /// affected when only one fails — the loop continues.
    #[tokio::test]
    async fn factory_rejection_surfaces_as_adapter_factory_error() {
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory = ScriptedPersonaAdapterFactory::always_fails("simulated factory rejection");
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 1);
        match &hosted[0] {
            Err(SupervisorError::AdapterFactory {
                slot_index,
                role,
                message,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert!(message.contains("simulated factory rejection"));
            }
            Err(other) => panic!("expected AdapterFactory error, got {other:?}"),
            Ok(_) => panic!("expected AdapterFactory error, got Ok"),
        }
    }

    /// Empty input → empty output. The Vec allocation is sized but
    /// no factory calls fire.
    #[tokio::test]
    async fn empty_plans_yields_empty_hosted() {
        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted = materialize_adapters(vec![], &factory, |_| None, |_| None).await;
        assert!(hosted.is_empty());
        assert_eq!(factory.build_count(), 0);
    }

    /// Missing-runtime path: when `runtime_lookup` returns `None` for
    /// a real plan, the substrate surfaces `SupervisorError::RuntimeMissing`
    /// with the slot index, role, and persona_id tagged. Per
    /// [[no-fallbacks-ever]]: the supervisor never fabricates a runtime
    /// when the registry lookup fails — a missing slot means the
    /// bootstrap chain skipped a step and the operator needs to see it.
    ///
    /// Locks in the contract for the slice-13.5 trait-extraction:
    /// `runtime_lookup`'s `Option<Arc<dyn AircCitizen>>` return shape
    /// is honored by `materialize_adapters` as a structured failure,
    /// NOT as a silent skip or fall-through to a default citizen.
    #[tokio::test]
    async fn runtime_lookup_none_surfaces_as_runtime_missing() {
        let instance = fake_instance("Paige");
        let expected_persona_id = instance.peer_id.as_uuid();
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance,
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        // `|_| None` here is the substrate-bug shape we're locking in:
        // the registry exists but doesn't contain this persona_id.
        let hosted = materialize_adapters(plans, &factory, |_| None, |_| None).await;

        assert_eq!(hosted.len(), 1);
        // Factory MUST NOT be called when the runtime lookup fails —
        // adapter construction is expensive (model load), the
        // substrate refuses early.
        assert_eq!(
            factory.build_count(),
            0,
            "factory must not run when runtime lookup fails"
        );
        match &hosted[0] {
            Err(SupervisorError::RuntimeMissing {
                slot_index,
                role,
                persona_id,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert_eq!(*persona_id, expected_persona_id);
            }
            Err(other) => panic!("expected RuntimeMissing error, got {other:?}"),
            Ok(_) => panic!("expected RuntimeMissing error, got Ok"),
        }
    }

    /// Mixed: slot 0 has a runtime (citizen-stub lookup succeeds),
    /// slot 1 doesn't (lookup returns None). The supervisor materializes
    /// the first cleanly and surfaces `RuntimeMissing` for the second —
    /// proving sibling slots don't cross-affect, matching the
    /// per-slot error semantics of `Profile` and `AdapterFactory`.
    #[tokio::test]
    async fn runtime_missing_only_affects_its_own_slot() {
        init_test_registry();
        let paige = fake_instance("Paige");
        let pax = fake_instance("Pax");
        let pax_persona_id = pax.peer_id.as_uuid();
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: paige,
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: pax,
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let factory = ScriptedPersonaAdapterFactory::heuristic();
        // Lookup returns Some only for Paige; Pax goes RuntimeMissing.
        let lookup =
            move |pid: Uuid| -> Option<Arc<dyn crate::persona::airc_citizen::AircCitizen>> {
                if pid == pax_persona_id {
                    None
                } else {
                    Some(Arc::new(StubAircCitizen::new(Uuid::new_v4()))
                        as Arc<dyn crate::persona::airc_citizen::AircCitizen>)
                }
            };
        let hosted = materialize_adapters(plans, &factory, lookup, |_| None).await;

        assert_eq!(hosted.len(), 2);
        // Factory ran exactly once — for Paige, not Pax.
        assert_eq!(factory.build_count(), 1);
        assert!(hosted[0].is_ok(), "Paige materializes cleanly");
        match &hosted[1] {
            Err(SupervisorError::RuntimeMissing {
                slot_index,
                role,
                persona_id,
            }) => {
                assert_eq!(*slot_index, 1);
                assert_eq!(*role, RoleId::Coder);
                assert_eq!(*persona_id, pax_persona_id);
            }
            Err(other) => panic!("expected RuntimeMissing at slot 1, got {other:?}"),
            Ok(_) => panic!("expected RuntimeMissing at slot 1, got Ok"),
        }
    }

    /// Warmup is called for every successfully-materialized adapter.
    /// Per [[init-once-handle-then-lease-zero-copy-refs]] the substrate
    /// pays init costs at boot, not on the user's first message;
    /// `materialize_adapters` is where that contract gets enforced.
    /// If a future refactor forgets the warmup call, this test fails
    /// because the shared counter stays at 0.
    #[tokio::test]
    async fn warmup_called_once_per_materialized_adapter() {
        init_test_registry();
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let (factory, counts) = ScriptedPersonaAdapterFactory::heuristic_with_counters();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        // Both slots materialize cleanly.
        assert_eq!(hosted.len(), 2);
        assert!(hosted.iter().all(|r| r.is_ok()));
        // Every adapter was built AND warmed.
        assert_eq!(factory.build_count(), 2);
        assert_eq!(
            counts.warmups(),
            2,
            "warmup() must be called once per successfully-materialized adapter"
        );
    }

    // 6d17695c: a real SQLite initialization failure must not host a blank
    // admission store or replace an existing resident. Healthy siblings still
    // materialize through the same factory/runtime/registration boundary.
    #[tokio::test(flavor = "current_thread")]
    async fn admission_restore_failure_refuses_hosting_and_preserves_existing_residents() {
        init_test_registry();
        for already_registered in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let _native = crate::paths::NativeHomeOverride::install(home.path());
            let factory = ScriptedPersonaAdapterFactory::heuristic();
            let registry = crate::cognition::persona_workspace::global();
            let mut failed = fake_instance("admission-refused");
            failed.home = home.path().join("refused-persona");
            let failed_id = failed.peer_id.as_uuid();
            let previous = if already_registered {
                // An already-running life remains valid even if a subsequent
                // bootstrap points at storage it cannot open. Its own DB is
                // left untouched; only the candidate home is broken below.
                let mut previous_identity = failed.clone();
                previous_identity.home = home.path().join("previous-persona");
                let previous_plan = MaterializedPersonaPlan {
                    role: RoleId::Helper,
                    instance: previous_identity,
                    profile: Ok(fake_profile("admission-refused", "model-a")),
                };
                let hosted = materialize_adapters(
                    vec![previous_plan],
                    &factory,
                    StubAircCitizen::fresh_lookup(),
                    |_| None,
                )
                .await;
                assert!(
                    hosted[0].is_ok(),
                    "test: the original resident must be healthy"
                );
                Some(
                    registry
                        .get(&failed_id)
                        .expect("test: original resident registered"),
                )
            } else {
                assert!(registry.get(&failed_id).is_none());
                None
            };
            let failed_home = crate::cognition::home::PersonaHome::from_root(failed.home.clone());
            failed_home.ensure_exists().unwrap();
            let broken = failed_home.engrams_db();
            let original = b"existing engram database is not valid SQLite";
            std::fs::write(&broken, original).unwrap();
            let mut healthy = fake_instance("admission-healthy");
            healthy.home = home.path().join("healthy-persona");
            let healthy_id = healthy.peer_id.as_uuid();
            let healthy_home = crate::cognition::home::PersonaHome::from_root(healthy.home.clone());
            let plans = vec![
                MaterializedPersonaPlan {
                    role: RoleId::Helper,
                    instance: failed,
                    profile: Ok(fake_profile("admission-refused", "model-a")),
                },
                MaterializedPersonaPlan {
                    role: RoleId::Coder,
                    instance: healthy,
                    profile: Ok(fake_profile("admission-healthy", "model-b")),
                },
            ];
            let hosted =
                materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None)
                    .await;
            assert_eq!(hosted.len(), 2);
            match &hosted[0] {
                Err(SupervisorError::AdmissionRestore {
                    slot_index,
                    role,
                    persona_id,
                    source,
                }) => {
                    assert_eq!(
                        (*slot_index, *role, *persona_id),
                        (0, RoleId::Helper, failed_id)
                    );
                    assert!(
                        matches!(source, crate::orm::OrmStoreError::AdapterFailed {
                        operation: "initialize", collection, ..
                    } if collection == "engrams"),
                        "test: preserve the actual SQLite initialization error: {source}"
                    );
                }
                Err(other) => panic!("test: expected admission restore failure, got {other:?}"),
                Ok(_) => {
                    panic!("test: failed persistent admission must not produce a hosted persona")
                }
            }
            assert!(
                hosted[1].is_ok(),
                "a new empty home must still initialize persistent admission"
            );
            assert!(registry.get(&healthy_id).is_some());
            assert!(healthy_home.engrams_db().is_file());
            if let Some(previous) = previous {
                assert!(Arc::ptr_eq(&previous, &registry.get(&failed_id).unwrap()));
            } else {
                assert!(registry.get(&failed_id).is_none());
            }
            assert_eq!(std::fs::read(&broken).unwrap(), original);
        }
    }

    // 9f160b78: actual boot materialization reports a checkpoint failure for
    // that slot, never hosts it blank, and still registers a healthy sibling.
    #[tokio::test(flavor = "current_thread")]
    async fn checkpoint_failure_refuses_hosting_without_tainting_sibling_slots() {
        init_test_registry();
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let mut failed = fake_instance("checkpoint-refused");
        failed.home = home.path().join("refused-persona");
        let failed_id = failed.peer_id.as_uuid();
        let mut healthy = fake_instance("checkpoint-healthy");
        healthy.home = home.path().join("healthy-persona");
        let healthy_id = healthy.peer_id.as_uuid();
        let broken = home
            .path()
            .join(".continuum/personas")
            .join(failed_id.to_string())
            .join("volatile.json");
        std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
        std::fs::write(&broken, b"not a checkpoint").unwrap();
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: failed,
                profile: Ok(fake_profile("checkpoint-refused", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: healthy,
                profile: Ok(fake_profile("checkpoint-healthy", "model-b")),
            },
        ];
        let factory = ScriptedPersonaAdapterFactory::heuristic();
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;
        assert_eq!(hosted.len(), 2);
        match &hosted[0] {
            Err(SupervisorError::WorkspaceRegistration {
                slot_index,
                role,
                persona_id,
                source,
            }) => {
                assert_eq!(
                    (*slot_index, *role, *persona_id),
                    (0, RoleId::Helper, failed_id)
                );
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidData);
            }
            Err(other) => panic!("test: expected checkpoint failure, got {other:?}"),
            Ok(_) => panic!("test: corrupt checkpoint must not produce a hosted persona"),
        }
        assert!(hosted[1].is_ok());
        let registry = crate::cognition::persona_workspace::global();
        assert!(registry.get(&failed_id).is_none());
        assert!(registry.get(&healthy_id).is_some());
        assert_eq!(std::fs::read(&broken).unwrap(), b"not a checkpoint");
    }

    /// Warmup failure surfaces as `SupervisorError::AdapterWarmup` —
    /// the persona does NOT reach hosted state. Per [[no-fallbacks-ever]]
    /// an adapter that refuses to warm gets a typed slot failure;
    /// sibling slots continue.
    #[tokio::test]
    async fn warmup_failure_surfaces_as_typed_slot_error() {
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory = ScriptedPersonaAdapterFactory::heuristic_with_warmup_failure(
            "simulated warmup failure",
        );
        let hosted =
            materialize_adapters(plans, &factory, StubAircCitizen::fresh_lookup(), |_| None).await;

        assert_eq!(hosted.len(), 1);
        match &hosted[0] {
            Err(SupervisorError::AdapterWarmup {
                slot_index,
                role,
                message,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert!(
                    message.contains("simulated warmup failure"),
                    "error must propagate underlying cause: {message}"
                );
            }
            Err(other) => panic!("expected AdapterWarmup, got {other:?}"),
            Ok(_) => panic!("expected AdapterWarmup, got Ok"),
        }
    }

    /// Warmup-failed adapters never reach the hosted set, so a
    /// sibling slot whose adapter warms fine still materializes.
    /// Locks the per-slot isolation that
    /// `Profile` / `AdapterFactory` / `RuntimeMissing` already enforce.
    #[tokio::test]
    async fn warmup_failure_does_not_taint_sibling_slots() {
        init_test_registry();
        let (factory_ok, ok_counts) = ScriptedPersonaAdapterFactory::heuristic_with_counters();
        let ok_plan = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];
        let hosted_ok = materialize_adapters(
            ok_plan,
            &factory_ok,
            StubAircCitizen::fresh_lookup(),
            |_| None,
        )
        .await;
        assert!(hosted_ok[0].is_ok(), "ok-warmup adapter materializes");
        assert_eq!(ok_counts.warmups(), 1);

        let factory_fail = ScriptedPersonaAdapterFactory::heuristic_with_warmup_failure(
            "simulated warmup failure",
        );
        let fail_plan = vec![MaterializedPersonaPlan {
            role: RoleId::Coder,
            instance: fake_instance("Pax"),
            profile: Ok(fake_profile("Pax", "model-b")),
        }];
        let hosted_fail = materialize_adapters(
            fail_plan,
            &factory_fail,
            StubAircCitizen::fresh_lookup(),
            |_| None,
        )
        .await;
        assert!(
            matches!(hosted_fail[0], Err(SupervisorError::AdapterWarmup { .. })),
            "warmup-failing adapter fails its own slot"
        );
    }
