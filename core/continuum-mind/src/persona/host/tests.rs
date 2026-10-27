#![cfg(test)]
    use super::*;

    // what this catches: a refused seat re-attempted at pass rate (the 2026-09-12
    // storm: 34 attempts/min, 24 leaked daemon streams each), a backoff that never
    // expires, or one that does not double and cap.
    #[test]
    fn a_failed_seat_backs_off_doubling_and_capped_and_is_tried_again_after() {
        assert_eq!(backoff_delay(1).as_secs(), 2);
        assert_eq!(backoff_delay(2).as_secs(), 4);
        assert_eq!(backoff_delay(5).as_secs(), 32);
        assert_eq!(backoff_delay(40).as_secs(), 600, "capped at ten minutes");
        let plan = plan_named("Atlas");
        let pid = plan.instance.peer_id.as_uuid();
        let mut map = std::collections::HashMap::new();
        map.insert(pid, SlotBackoff { attempts: 3, until_ms: 10_000 });
        assert!(filter_by_backoff_with(9_999, &map, vec![plan]).is_empty(), "inside the backoff: skipped");
        let plan = plan_named("Atlas");
        map.insert(plan.instance.peer_id.as_uuid(), SlotBackoff { attempts: 3, until_ms: 10_000 });
        assert_eq!(filter_by_backoff_with(10_000, &map, vec![plan]).len(), 1, "at expiry: tried again");
        assert_eq!(filter_by_backoff_with(0, &map, vec![plan_named("Demetri")]).len(), 1, "another persona is untouched");
    }

    fn plan_named(name: &str) -> crate::persona::spawner_module::MaterializedPersonaPlan {
        crate::persona::spawner_module::MaterializedPersonaPlan {
            role: crate::experience::role::RoleId::Helper,
            instance: crate::persona::instance_manager::PersonaInstanceInfo {
                agent_name: name.to_string(),
                peer_id: crate::identity::PeerId::from_uuid(uuid::Uuid::new_v4()),
                home: std::path::PathBuf::from("/tmp/unused"),
                default_room: uuid::Uuid::new_v4(),
                source: crate::persona::identity_provider::PersonaIdentitySource::FreshlyMinted,
            },
            profile: Err(
                crate::persona::inference_profile::InferenceProfileError::UnknownModel {
                    model_id: "fixture".into(),
                    role_id: "helper".into(),
                },
            ),
        }
    }

    // what this catches: regression for the 2026-08-23 boot bypass — the
    // roster-hold gate lived only in spawn_all, so host_unattended (fired on
    // every serving edge) hosted held-out citizens with zero held_out probes.
    // Both reconciler entrances now share filter_by_hold; this pins the pure
    // half's contract: allowed names pass, everyone else is dropped, and no
    // hold means no filtering.
    #[test]
    fn hold_filter_drops_unallowed_plans_and_passes_without_a_hold() {
        let hold = crate::persona::roster_hold::RosterHold {
            only: vec!["Atlas".into()],
            until_ms: u64::MAX,
            reason: "test measurement window".into(),
            exclusive: true, // an operator-issued hold: only these names sit
        };
        let plans = vec![plan_named("Atlas"), plan_named("Kira")];
        let kept = PersonaSpawnSupervisor::filter_by_hold_with(Some(hold), plans);
        assert_eq!(kept.len(), 1, "held-out citizen must be dropped");
        assert_eq!(kept[0].instance.agent_name, "Atlas");

        let plans = vec![plan_named("Atlas"), plan_named("Kira")];
        let kept = PersonaSpawnSupervisor::filter_by_hold_with(None, plans);
        assert_eq!(kept.len(), 2, "no hold means every plan passes");
    }

    // what this catches: the hosting reconciler (#429) runs host_unattended on
    // EVERY serving-plan edge for the life of the process. With nothing
    // unattended it must be a QUIET no-op — empty summary, zero failure rows
    // (a spurious row here would warn-log every ~5s forever), and the adapter
    // factory must never be consulted (a factory call on an empty scan would
    // probe /v1 each tick for nothing). Hosting an actual newborn end-to-end
    // needs a live airc daemon (same limit the registry's own tests document
    // in clone_shares_roster); the scan + early-return contract is the
    // unit-pinnable half.
    #[tokio::test]
    async fn host_unattended_with_nothing_unattended_is_a_quiet_noop() {
        struct NeverConsultedFactory;
        #[async_trait::async_trait]
        impl PersonaAdapterFactory for NeverConsultedFactory {
            async fn build_adapter(
                &self,
                _profile: &crate::persona::inference_profile::PersonaInferenceProfile,
            ) -> Result<Arc<dyn crate::ai::adapter::AIProviderAdapter>, String> {
                panic!(
                    "factory must not be consulted when the registry has no unattended citizens"
                );
            }
        }

        let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::new();
        let tmp = std::env::temp_dir().join("host-unattended-noop-test");
        let instance_manager = Arc::new(
            crate::persona::instance_manager::PersonaInstanceManagerModule::new(
                registry,
                tmp.join("daemon.sock"),
                tmp,
            ),
        );
        // Idempotent — safe under full-suite parallelism (the singleton's own
        // doc: "subsequent init_global calls are no-ops").
        let model_registry =
            crate::model_registry::init_global().expect("model registry init for test");
        let supervisor = PersonaSpawnSupervisor::new(
            PersonaSpawnerModule::new(
                crate::cognition::model_resolver::types::HwCapabilityTier::CpuOnly,
                crate::persona::hw_tier_descriptor::HwTierCategory::Compat,
            ),
            instance_manager,
            Arc::new(NeverConsultedFactory),
            "test-tier",
            model_registry,
            tokio::runtime::Handle::current(),
        );

        // A real executor over an empty registry: the host requires one (card 920619c5), and
        // nothing here dispatches through it.
        let executor = Arc::new(crate::runtime::CommandExecutor::new(Arc::new(crate::runtime::ModuleRegistry::new())));
        let summary = supervisor.host_unattended(executor).await;
        assert_eq!(summary.hosted, 0, "nothing to host → nothing hosted");
        assert!(
            summary.failures.is_empty(),
            "an empty scan must not synthesize failure rows: {:?}",
            summary.failures
        );
    }

    #[test]
    fn boot_summary_attempted_sums_hosted_and_failures() {
        let mut s = BootSummary::default();
        s.hosted = 3;
        s.failures.push(BootSlotFailure {
            slot_index: 1,
            role: Some(RoleId::Helper),
            persona_id: None,
            reason: "test".into(),
        });
        s.failures.push(BootSlotFailure {
            slot_index: 2,
            role: Some(RoleId::Coder),
            persona_id: None,
            reason: "test".into(),
        });
        assert_eq!(s.attempted(), 5);
        assert_eq!(s.failed(), 2);
    }

    #[test]
    fn boot_summary_serde_camel_case() {
        let s = BootSummary {
            hosted: 1,
            failures: vec![BootSlotFailure {
                slot_index: 0,
                role: Some(RoleId::Helper),
                persona_id: None,
                reason: "demo".into(),
            }],
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(json.contains("\"hosted\":1"));
        assert!(json.contains("\"slotIndex\":0"));
    }
    // what this catches (2026-09-14): the reconciler's hold filter dropping the rest of
    // the population under a DERIVED team hold. Team first, nobody dropped.
    #[test]
    fn a_derived_team_hold_orders_the_plans_and_drops_nobody() {
        let hold = crate::persona::roster_hold::from_team_names(vec!["Kira".into()], 0).expect("hold");
        let plans = vec![plan_named("Atlas"), plan_named("Kira"), plan_named("Benchy")];
        let kept = PersonaSpawnSupervisor::filter_by_hold_with(Some(hold), plans);
        let names: Vec<&str> = kept.iter().map(|p| p.instance.agent_name.as_str()).collect();
        assert_eq!(names, vec!["Kira", "Atlas", "Benchy"], "team first, everyone seated");
    }

    // what this catches (IntelMac 2026-09-21 13:00:49Z): an exhausted identity provider
    // on a RECONCILER pass drained the six live residents as "partially registered" and the
    // node sat at zero until a reboot. Only ids registered since the pass began are
    // orphans; a resident live at entry is kept. Both directions: nothing new → nothing
    // drained; one new registration → exactly that one.
    #[test]
    fn a_failed_pass_drains_only_what_it_registered_never_the_live_residents() {
        let live: Vec<Uuid> = (0..6).map(|_| Uuid::new_v4()).collect();
        assert!(super::partially_registered(&live, &live).is_empty(), "six live, nothing new: nothing to drain");
        let newcomer = Uuid::new_v4();
        let mut now = live.clone();
        now.push(newcomer);
        assert_eq!(super::partially_registered(&live, &now), vec![newcomer], "only the id this pass registered");
        assert!(super::partially_registered(&[], &now).len() == 7, "at boot the registry is all this pass's");
    }
