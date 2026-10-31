#![cfg(test)]
    use super::*;

    /// A spawner on a host whose operator holds nobody out and rests nobody: the seat
    /// arithmetic under test, never the hold the machine running the suite carries.
    fn unheld(hw: HwCapabilityTier, tier: HwTierCategory) -> PersonaSpawnerModule {
        PersonaSpawnerModule::new(hw, tier).with_operator_roster(OperatorRoster::default)
    }

    // what this catches (#430): the roster is DATA. with_citizens drives the
    // plan; an EMPTY authored roster (headless serving node) legitimately
    // plans nobody, and the slice-14 guard hosts only the FIRST of a
    // multi-role roster (loud error, never a silent flip on boot 2).
    #[test]
    fn recipe_citizens_drive_the_plan() {
        let base = || unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat);
        assert!(base().with_citizens(vec![]).plan().is_empty());

        let coder = base().with_citizens(vec![RoleId::Coder]).plan();
        assert_eq!(coder.len(), 1);
        assert_eq!(coder[0].role, RoleId::Coder);

        let multi = base()
            .with_citizens(vec![RoleId::Helper, RoleId::Coder])
            .plan();
        assert_eq!(
            multi.len(),
            1,
            "multi-role rosters host the FIRST role only until #133 slice 14"
        );
        assert_eq!(multi[0].role, RoleId::Helper);
    }

    // what this catches (#432): the boot task re-sizing the plan to the
    // identity provider's real yield. set_population must grow plan() the same
    // way with_population does (with the same ≥1 clamp) — if it silently
    // no-opped, every resumed citizen beyond the mint floor would stay
    // unhosted on disk again.
    #[test]
    fn set_population_resizes_the_plan() {
        let mut spawner =
            unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat);
        assert_eq!(spawner.plan().len(), 1);
        spawner.set_population(3);
        assert_eq!(spawner.plan().len(), 3);
        spawner.set_population(0);
        assert_eq!(spawner.plan().len(), 0, "zero is deliberate: the provider will yield none (a team is seated elsewhere)");
    }

    // what this catches (2026-09-06): a roster larger than the warm lanes. With a real
    // plan of 4 lanes the population of 12 seats 4; with no plan yet the configured
    // population stands; a plan can never seat zero.
    // what this catches (2026-09-06): the hold applied AFTER the seat cut. Six on
    // disk, three seats, a hold naming Delta/Foxtrot/Alpha → the seats go to Alpha,
    // Delta, Foxtrot (yield order), never to Bravo/Charlie.
    #[tokio::test]
    async fn a_hold_picks_who_before_the_seats_count_how_many() {
        struct Yield(std::collections::VecDeque<&'static str>);
        #[async_trait::async_trait]
        impl crate::persona::identity_provider::PersonaIdentityProvider for Yield {
            fn name(&self) -> &'static str {
                "yield"
            }
            async fn next_persona(
                &mut self,
            ) -> Result<Option<PersonaIdentityIntent>, crate::persona::identity_provider::PersonaIdentityError> {
                Ok(self.0.pop_front().map(|n| PersonaIdentityIntent {
                    persona_id: uuid::Uuid::new_v4(),
                    agent_name: n.to_string(),
                    source: crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
                }))
            }
        }
        let mut provider = Yield(["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"].into_iter().collect());
        let plan = vec![
            plan_for_roles(&[RoleId::Helper], HwCapabilityTier::CpuOnly, HwTierCategory::Compat)[0].clone();
            3
        ];
        let hold = crate::persona::roster_hold::RosterHold {
            only: ["Delta", "Foxtrot", "Alpha"].iter().map(|s| s.to_string()).collect(),
            until_ms: u64::MAX,
            reason: "test".to_string(),
            exclusive: true,
        };
        let operator = OperatorRoster { hold: Some(hold), resting: Vec::new(), dormant: Default::default() };
        let intents = draw_intents(&mut provider, &plan, &operator, 0).await.expect("draw");
        let names: Vec<&str> = intents.iter().map(|i| i.agent_name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Delta", "Foxtrot"]);
    }

    // what this catches (2026-09-06, re-pinned 2026-09-13, REVERSED 2026-09-20, card
    // 7c38ff6f): the missing plan is the seats not yet filled — and the seats are the
    // population BOUNDED by the warm slots (lanes × the per-lane edge). Twelve minds on
    // five lanes seat min(12, 5 × edge); three hosted draws the rest; a full roster draws
    // none; more hosted than seats draws none (the shrink is the hourly rest's, not a draw).
    #[test]
    fn the_missing_plan_is_the_seats_not_yet_filled() {
        let per = crate::cognition::health_ledger::MINDS_PER_LANE_STARVED_ABOVE as usize;
        let mut spawner = unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat);
        spawner.set_population(12);
        spawner.serving_base_model = Some("some/model".to_string());
        spawner.serving_lanes = 5;
        let seats = 12.min(5 * per);
        assert_eq!(spawner.seats(), seats, "five lanes bound twelve minds to 5 × {per}");
        let per_seat = plan_for_roles(&spawner.citizens, spawner.hw_capability, spawner.tier_category).len();
        assert_eq!(missing_plan(&spawner, 3).len(), (seats - 3) * per_seat, "three hosted: the rest of the bounded seats are drawn");
        assert_eq!(missing_plan(&spawner, seats).len(), 0);
        assert_eq!(missing_plan(&spawner, 14).len(), 0);
        assert_eq!(missing_plan(&spawner, 0).len(), seats * per_seat);
    }

    // what this catches (card 10bba591): once the grid allocator has published, THIS
    // node's seat count is the allocation's (seated + open here), not the warm-slot
    // arithmetic — a wider grid seats more, a held or full grid seats fewer — while the
    // identity-side bound (the population) still holds; and with nothing published the
    // warm-slot prior stands exactly as before.
    #[test]
    fn a_published_allocation_owns_this_nodes_seat_count_and_the_population_still_bounds_it() {
        use crate::cognition::grid_allocation::GridRoster;
        let per = crate::cognition::health_ledger::MINDS_PER_LANE_STARVED_ABOVE as usize;
        let mut spawner = unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat);
        spawner.set_population(12);
        spawner.serving_base_model = Some("some/model".to_string());
        spawner.serving_lanes = 2;
        assert_eq!(spawner.seats(), 12.min(2 * per), "nothing published: the warm-slot prior");
        spawner.set_grid_roster(Some(GridRoster { seated: 4, open: 4, resuming: 0 }));
        assert_eq!(spawner.seats(), 8, "the allocation counts eight seats here");
        spawner.set_grid_roster(Some(GridRoster { seated: 1, open: 0, resuming: 0 }));
        assert_eq!(spawner.seats(), 1, "a held or full grid seats one");
        spawner.set_grid_roster(Some(GridRoster { seated: 2, open: 0, resuming: 2 }));
        assert_eq!(spawner.seats(), 4, "held minds can resume before they have live runtime ids");
        spawner.set_grid_roster(Some(GridRoster { seated: 10, open: 10, resuming: 0 }));
        assert_eq!(spawner.seats(), 12, "never past what the provider can yield");
        spawner.set_grid_roster(None);
        assert_eq!(spawner.seats(), 12.min(2 * per), "unpublished again: the prior");
    }

    #[test]
    fn the_roster_is_the_population_bounded_by_the_warm_slots() {
        let per = crate::cognition::health_ledger::MINDS_PER_LANE_STARVED_ABOVE as usize;
        let mut spawner = unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat);
        spawner.set_population(12);
        assert_eq!(spawner.seats(), 12, "no plan and no remembered lanes: nothing is known, the population stands");
        // A cold boot with a remembered geometry is bounded by it before any live plan.
        let remembered = unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat)
            .with_population(12)
            .with_remembered_lanes(Some(2));
        assert_eq!(remembered.seats(), 12.min(2 * per), "the boot draw is bounded by the last settled lanes");
        spawner.serving_base_model = Some("ggml-org/Qwen3.8-27B-GGUF".to_string());
        spawner.serving_lanes = 4;
        assert_eq!(spawner.seats(), 12.min(4 * per), "a live plan's lanes bound the draw: the rest stay dormant");
        assert_eq!(spawner.plan().len(), spawner.seats() * plan_for_roles(&spawner.citizens, spawner.hw_capability, spawner.tier_category).len());
        spawner.set_population(0);
        assert_eq!(spawner.seats(), 0, "zero is deliberate: the provider will yield none while a team is seated elsewhere on the grid");
    }

    // what this catches (IntelMac on #3798): a runtime plan that no longer fits the GPU
    // leaving the seat count on the EARLIER GPU plan's lanes. The marker and the lanes
    // move together, so a non-fitting live plan seats the configured population.
    #[test]
    fn a_plan_that_stops_fitting_the_gpu_releases_the_stale_lane_cap() {
        let per = crate::cognition::health_ledger::MINDS_PER_LANE_STARVED_ABOVE as usize;
        let mut spawner = unheld(HwCapabilityTier::CpuOnly, HwTierCategory::Compat);
        spawner.set_population(6);
        spawner.serving_base_model = Some("some/model".to_string());
        spawner.serving_lanes = 2;
        assert_eq!(spawner.seats(), 6.min(2 * per), "two live lanes bound the draw");
        spawner.set_serving(None);
        assert_eq!(spawner.seats(), 6, "no fitting plan and nothing remembered: no lane is known, the population stands");
        assert!(spawner.serving_base_model.is_none());
    }

    /// Compat tier produces the LCD roster: Helper + Coder both on
    /// the 1.5B floor. The canonical Intel-Mac startup state #133
    /// targets.
    ///
    /// Slice 13 update: temporarily single-Helper while ResumeOrMint-
    /// Provider's alphabetical sort + position-pairing hazard is
    /// resolved in slice 14. Coder will be re-added once
    /// role-in-seed.json lands.
    #[test]
    fn compat_tier_plans_single_helper_on_the_floor() {
        let plan = plan_for_tier(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].role, RoleId::Helper);
        // THE FLOOR IS 1.5B (2026-09-16): the pre-plan fallback names the smallest
        // model any tier may serve, and there is no 0.5B row to name.
        assert_eq!(
            plan[0].model_id,
            "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF"
        );
    }

    /// Every tier currently plans exactly one Helper — until slice 14
    /// lands role-in-seed.json + RoleAwareProvider, multi-role plans
    /// would mis-pair roles on boot 2 (alphabetic-disk-order vs
    /// plan-order). Single role per tier is the safe floor for now.
    #[test]
    fn every_tier_plans_single_helper() {
        for (hw, cat) in [
            (HwCapabilityTier::CpuOnly, HwTierCategory::Compat),
            (HwCapabilityTier::M1Uma8Gb, HwTierCategory::MSeries),
            (HwCapabilityTier::M5UmaProMax, HwTierCategory::MSeriesPro),
            (HwCapabilityTier::Sm120, HwTierCategory::Cuda),
            (HwCapabilityTier::Cloud, HwTierCategory::Cloud),
        ] {
            let plan = plan_for_tier(hw, cat);
            assert_eq!(
                plan.len(),
                1,
                "tier {cat:?} planned {} roles, want 1",
                plan.len()
            );
            assert_eq!(
                plan[0].role,
                RoleId::Helper,
                "tier {cat:?} first slot must be Helper"
            );
        }
    }

    /// ServiceModule.plan() is the same as the free function with the
    /// module's configured tier — proves the substrate-managed and
    /// pure-function paths agree.
    #[test]
    fn module_plan_matches_free_function() {
        let module = unheld(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );
        assert_eq!(
            module.plan(),
            plan_for_tier(
                HwCapabilityTier::MacIntelMetalDiscrete,
                HwTierCategory::Compat,
            )
        );
    }

    /// Provider exhaustion is a clean structured error. Tests slice
    /// 8's wiring without needing airc — the provider returns None
    /// before any `bootstrap_one` would fire, so the function
    /// short-circuits with a named error.
    #[tokio::test]
    async fn bootstrap_planned_exhausted_provider_errors_with_slot_info() {
        use crate::persona::identity_provider::{
            PersonaIdentityError, PersonaIdentityIntent, PersonaIdentityProvider,
        };
        use async_trait::async_trait;
        use std::path::PathBuf;

        // Provider that returns None immediately — simulates "we
        // configured a roster of 2 but only have 0 saved identities
        // and refuse to mint" (or any other exhaustion).
        struct EmptyProvider;
        #[async_trait]
        impl PersonaIdentityProvider for EmptyProvider {
            fn name(&self) -> &'static str {
                "empty"
            }
            async fn next_persona(
                &mut self,
            ) -> Result<Option<PersonaIdentityIntent>, PersonaIdentityError> {
                Ok(None)
            }
        }

        let module = unheld(
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwTierCategory::Compat,
        );

        // The bootstrapper is never reached because provider exhausts
        // first — its construction can be cheap-and-unreachable.
        // continuum_root/daemon_socket never get touched.
        let instance_manager = PersonaInstanceManagerModule::new(
            crate::persona::PersonaAircRuntimeRegistry::default(),
            PathBuf::from("/dev/null/unused"),
            PathBuf::from("/dev/null/unused"),
        );

        // Registry contents don't matter — derive_spawn_plan is never
        // reached when the provider exhausts at slot 0.
        let registry = std::sync::Arc::new(
            crate::model_registry::Registry::from_catalog(vec![], vec![]).expect("empty registry"),
        );

        let mut provider = EmptyProvider;
        let err = bootstrap_planned(
            &module,
            &instance_manager,
            &mut provider,
            "mac_intel_metal_discrete",
            &registry,
            0,
        )
        .await
        .expect_err("must error when provider exhausts");
        match err {
            BootstrapPlannedError::IdentityProviderExhausted {
                slot_index,
                role,
                provided,
                required,
            } => {
                assert_eq!(slot_index, 0);
                assert_eq!(role, RoleId::Helper);
                assert_eq!(provided, 0);
                // Slice 13 P2: single-Helper plan until slice 14 lands
                // role-in-seed.json. `required` reflects the current
                // plan size; updates to 2 when Coder returns.
                assert_eq!(required, 1);
            }
            other => panic!("expected IdentityProviderExhausted, got {other:?}"),
        }
    }

    /// Roundtrip the DesiredRole through serde — verifies the
    /// camelCase wire shape and that ts-rs export will produce a clean
    /// TS type.
    #[test]
    fn desired_role_serde_camel_case() {
        let role = DesiredRole {
            role: RoleId::Helper,
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            lanes: 1,
            served_context_window: 8192,
        };
        let json = serde_json::to_string(&role).expect("serialize");
        // RoleId already serializes as snake_case ("helper"); model_id
        // becomes modelId per the camelCase rename_all on this struct.
        assert!(json.contains("\"role\":\"helper\""));
        assert!(json.contains("\"modelId\":\"continuum-ai/qwen2.5-0.5b-instruct-GGUF\""));
        assert!(json.contains("\"servedContextWindow\":8192"));
        let back: DesiredRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, role);
    }
    // what this catches: a hold smaller than the population turning the boot into a
    // drain loop (2026-09-13: 12 seats, 5 allowed, zero residents for four minutes).
    // A hold defines the roster, so it bounds the seats; no hold, population stands.
    #[test]
    fn a_roster_hold_bounds_the_seats_to_the_names_it_allows() {
        let hold = crate::persona::roster_hold::RosterHold {
            only: ["Alpha", "Bravo", "Charlie"].iter().map(|s| s.to_string()).collect(),
            until_ms: u64::MAX,
            reason: "test".to_string(),
            exclusive: true,
        };
        assert_eq!(seats_under(12, Some(&hold)), 3, "the hold's three names are the roster");
        assert_eq!(seats_under(2, Some(&hold)), 2, "a hold never grows the population");
        assert_eq!(seats_under(12, None), 12, "no hold: the population stands");
        assert_eq!(seats_under(0, None), 0, "zero is deliberate — the grid holds the roster, this node offers lanes");
        // The module reads the hold and the resting seats from its injected source:
        // three allowed, one resting → two seats, on any machine.
        fn three_held_one_resting() -> OperatorRoster {
            OperatorRoster {
                hold: Some(crate::persona::roster_hold::RosterHold {
                    only: ["Alpha", "Bravo", "Charlie"].iter().map(|s| s.to_string()).collect(),
                    until_ms: u64::MAX,
                    reason: "test".to_string(),
                    exclusive: true,
                }),
                resting: vec![crate::persona::resting_seat::RestingSeat {
                    agent_name: "Charlie".to_string(),
                    persona_id: uuid::Uuid::nil(),
                    reason: "test".to_string(),
                    since_ms: 0,
                    build: String::new(),
                }],
                dormant: Default::default(),
            }
        }
        let held = PersonaSpawnerModule::new(HwCapabilityTier::CpuOnly, HwTierCategory::Compat)
            .with_population(12)
            .with_operator_roster(three_held_one_resting);
        assert_eq!(held.seats(), 2, "the injected hold bounds the seats, the resting seat comes off");
    }

    // what this catches (2026-09-13): a plan larger than the provider's identities turning
    // the boot into a drain loop — zero residents until an operator moved a file. The
    // identities the provider yields ARE the roster; the surplus seats are a probe.
    #[tokio::test]
    async fn a_provider_shortfall_seats_what_it_has_and_never_fails_the_boot() {
        struct Yield(std::collections::VecDeque<&'static str>);
        #[async_trait::async_trait]
        impl crate::persona::identity_provider::PersonaIdentityProvider for Yield {
            fn name(&self) -> &'static str {
                "yield"
            }
            async fn next_persona(
                &mut self,
            ) -> Result<Option<PersonaIdentityIntent>, crate::persona::identity_provider::PersonaIdentityError> {
                Ok(self.0.pop_front().map(|n| PersonaIdentityIntent {
                    persona_id: uuid::Uuid::new_v4(),
                    agent_name: n.to_string(),
                    source: crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
                }))
            }
        }
        let mut provider = Yield(["Alpha", "Bravo"].into_iter().collect());
        let plan: Vec<DesiredRole> = (0..5).map(|_| DesiredRole { role: RoleId::Helper, model_id: "m".to_string(), lanes: 1, served_context_window: 4096 }).collect();
        let intents = draw_intents(&mut provider, &plan, &OperatorRoster::default(), 0).await.expect("a shortfall is not an error");
        let names: Vec<&str> = intents.iter().map(|i| i.agent_name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Bravo"], "two identities seat two of five planned seats");
        let mut empty = Yield(std::collections::VecDeque::new());
        assert!(draw_intents(&mut empty, &plan, &OperatorRoster::default(), 0).await.is_err(), "NO identity at all is still the honest failure");
        // IntelMac 2026-09-21 13:00:49Z: the SAME empty provider on a TOP-UP pass — six
        // residents live, the plan wants two more — is a partial roster of zero, never a
        // boot failure the host would answer by draining the six.
        let mut empty = Yield(std::collections::VecDeque::new());
        let top_up: Vec<DesiredRole> = plan[..2].to_vec();
        let drawn = draw_intents(&mut empty, &top_up, &OperatorRoster::default(), 6).await.expect("a top-up that draws nothing is a partial roster");
        assert!(drawn.is_empty(), "nothing to seat, nothing failed");
    }

    // what this catches (the M5, 2026-10-27 00:16Z): the four-name hold expired and the
    // reconciler resumed every home from disk, ten minds on two lanes. A dormant home is
    // never drawn on boot (the seat goes to the next awake one) and never counted as a
    // seat, with or without the hold: twelve dormant of sixteen seat four either way.
    #[tokio::test]
    async fn a_dormant_home_is_not_resumed_on_boot_with_or_without_the_hold() {
        struct Yield(std::collections::VecDeque<&'static str>);
        #[async_trait::async_trait]
        impl crate::persona::identity_provider::PersonaIdentityProvider for Yield {
            fn name(&self) -> &'static str {
                "yield"
            }
            async fn next_persona(
                &mut self,
            ) -> Result<Option<PersonaIdentityIntent>, crate::persona::identity_provider::PersonaIdentityError> {
                Ok(self.0.pop_front().map(|n| PersonaIdentityIntent {
                    persona_id: uuid::Uuid::new_v4(),
                    agent_name: n.to_string(),
                    source: crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
                }))
            }
        }
        fn dormant(name: &str) -> crate::persona::dormancy::PersonaWakefulness {
            crate::persona::dormancy::PersonaWakefulness {
                id: crate::identity::PeerId::from_uuid(uuid::Uuid::new_v4()),
                agent_name: name.to_string(),
                state: crate::persona::dormancy::Wakefulness::Dormant { since_ms: 1, reason: "population".into(), by: "test".into() },
                changed_at_ms: 1,
            }
        }
        let operator = OperatorRoster { hold: None, resting: Vec::new(), dormant: crate::persona::dormancy::DormantSet::Known(vec![dormant("Bravo")]) };
        let mut provider = Yield(["Alpha", "Bravo", "Charlie"].into_iter().collect());
        let plan: Vec<DesiredRole> = (0..3).map(|_| DesiredRole { role: RoleId::Helper, model_id: "m".to_string(), lanes: 1, served_context_window: 4096 }).collect();
        let drawn = draw_intents(&mut provider, &plan, &operator, 0).await.expect("a partial roster"); // test: a failure here is the assertion failing loudly
        let names: Vec<&str> = drawn.iter().map(|i| i.agent_name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Charlie"], "the dormant home is skipped, the awake ones seat");

        fn twelve_dormant(hold: bool) -> OperatorRoster {
            let names = ["Aiko", "Aris", "Atlas", "Benchy"];
            OperatorRoster {
                hold: hold.then(|| crate::persona::roster_hold::RosterHold {
                    only: names.iter().map(|s| s.to_string()).collect(),
                    until_ms: u64::MAX,
                    reason: "population".into(),
                    exclusive: true,
                }),
                resting: Vec::new(),
                dormant: crate::persona::dormancy::DormantSet::Known((0..12).map(|i| dormant(&format!("Dormant{i}"))).collect()),
            }
        }
        let seats = |roster: fn() -> OperatorRoster| {
            PersonaSpawnerModule::new(HwCapabilityTier::CpuOnly, HwTierCategory::Compat)
                .with_population(16)
                .with_operator_roster(roster)
                .seats()
        };
        assert_eq!(seats(|| twelve_dormant(true)), 4, "under the hold: the four it names");
        assert_eq!(seats(|| twelve_dormant(false)), 4, "after the hold lapses: still four, the dormant stay down");
    }

    // what this catches: a wakefulness store that failed to open read as "nobody dormant",
    // which resumes every home from disk (tonight's incident, replayed). Unavailable fails
    // closed: no resumed home is drawn; a freshly minted birth still is.
    #[tokio::test]
    async fn an_unavailable_ledger_resumes_no_home_from_disk() {
        use crate::persona::identity_provider::PersonaIdentitySource;
        struct Yield(std::collections::VecDeque<(&'static str, PersonaIdentitySource)>);
        #[async_trait::async_trait]
        impl crate::persona::identity_provider::PersonaIdentityProvider for Yield {
            fn name(&self) -> &'static str {
                "yield"
            }
            async fn next_persona(
                &mut self,
            ) -> Result<Option<PersonaIdentityIntent>, crate::persona::identity_provider::PersonaIdentityError> {
                Ok(self.0.pop_front().map(|(n, source)| PersonaIdentityIntent { persona_id: uuid::Uuid::new_v4(), agent_name: n.to_string(), source }))
            }
        }
        let operator = OperatorRoster { hold: None, resting: Vec::new(), dormant: crate::persona::dormancy::DormantSet::Unavailable };
        let mut provider = Yield(
            [("Aiko", PersonaIdentitySource::ResumedFromDisk), ("Idris", PersonaIdentitySource::ResumedFromDisk), ("Newborn", PersonaIdentitySource::FreshlyMinted)]
                .into_iter()
                .collect(),
        );
        let plan: Vec<DesiredRole> = (0..3).map(|_| DesiredRole { role: RoleId::Helper, model_id: "m".to_string(), lanes: 1, served_context_window: 4096 }).collect();
        let drawn = draw_intents(&mut provider, &plan, &operator, 0).await.expect("a partial roster"); // test: a failure here is the assertion failing loudly
        let names: Vec<&str> = drawn.iter().map(|i| i.agent_name.as_str()).collect();
        assert_eq!(names, vec!["Newborn"], "no home from disk while the ledger is unavailable; a birth still seats");
    }

    // what this catches (2026-09-14): a working round's team hold shrinking the roster —
    // seven of twelve minds held out on a sixteen-seat node because the derived hold read
    // as exclusive. A derived hold orders (team first); only an operator hold excludes.
    #[test]
    fn a_derived_team_hold_seats_the_team_first_and_never_fewer_minds() {
        let derived = crate::persona::roster_hold::from_team_names(vec!["Delta".into(), "Alpha".into()], 0).expect("hold");
        assert!(!derived.exclusive);
        assert_eq!(seats_under(12, Some(&derived)), 12, "a team hold never bounds the seats");
        let operator = crate::persona::roster_hold::RosterHold { only: vec!["Alpha".into()], until_ms: u64::MAX, reason: "op".into(), exclusive: true };
        assert_eq!(seats_under(12, Some(&operator)), 1, "an operator hold does");
    }

    // what this catches (card 7c38ff6f; the M5 boot of 23d33019d, 01:12Z 9/20): sixteen minds
    // drawn onto a node whose lanes could seat four, their residency starving the plan to a
    // 2,048-token lane. The draw is bounded by the warm slots: lanes × the per-lane edge,
    // never below the resident floor, and an unknown lane count bounds nothing.
    #[test]
    fn the_draw_is_bounded_by_the_warm_slots_and_an_unknown_lane_count_bounds_nothing() {
        use crate::cognition::health_ledger::{MINDLESS_RESIDENT_FLOOR, MINDS_PER_LANE_STARVED_ABOVE};
        let per = MINDS_PER_LANE_STARVED_ABOVE as usize;
        assert_eq!(bounded_by_warm_slots(16, Some(2)), 2 * per, "two lanes seat two lanes' worth");
        assert_eq!(bounded_by_warm_slots(16, Some(1)), per.max(MINDLESS_RESIDENT_FLOOR as usize), "one lane: the edge, never below the floor");
        assert_eq!(bounded_by_warm_slots(16, Some(8)), 16, "lanes past the population change nothing");
        assert_eq!(bounded_by_warm_slots(16, None), 16, "no lane known bounds nothing — an absence is not a number");
        assert_eq!(bounded_by_warm_slots(16, Some(0)), 16, "a zero lane count is unknown, not a bound of zero");
        assert_eq!(bounded_by_warm_slots(0, Some(2)), 0, "an empty population stays empty (the one-roster rule)");
    }

