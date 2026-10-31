#![cfg(test)]
    use super::*;

    // what this catches: the start-a-project flow's wire surface. `activity/recipes`
    // is the catalogue read `activity/spawn`'s own description points at (before it,
    // discovery-by-refusal was the only path), and `activity/invite` is how a fresh
    // room gets its people. Both AiSafe — a citizen or the operator starting an
    // activity is the ordinary creative act. Renaming either orphans the UI flow.
    #[test]
    fn start_a_project_verbs_are_aisafe_under_their_wire_names() {
        assert_eq!(ActivityRecipes::NAME, "activity/recipes");
        assert_eq!(ActivityState::NAME, "activity/state");
        assert_eq!(ActivityRecipes::ACCESS, AccessLevel::AiSafe);
        assert_eq!(ActivityInvite::NAME, "activity/invite");
        assert_eq!(ActivityInvite::ACCESS, AccessLevel::AiSafe);
    }

    // what this catches: the exact live bug that made EVERY benchmark run room
    // render as plain chat — dispatch bound the recipe string "benchmark" while
    // the shipped recipe declares purpose "benchmark/hard-rs", and nothing
    // validated the string against the registry. The gate must refuse the bad
    // string LOUDLY (naming the actionable set) and pass every shipped purpose.
    // regression for #431 / commit at benchmark.rs:1091
    #[test]
    fn validate_recipe_refuses_unknown_and_passes_every_shipped_purpose() {
        let empty_overlay = tempfile::tempdir().expect("tempdir");
        for purpose in crate::experience::source::RecipeExperienceSource::shipped_purposes() {
            assert!(
                validate_recipe(&purpose, empty_overlay.path()).is_ok(),
                "shipped purpose {purpose:?} must validate"
            );
        }
        let err = validate_recipe("benchmark", empty_overlay.path())
            .expect_err("the old dispatch literal must refuse");
        let msg = format!("{err}");
        assert!(
            msg.contains("benchmark/hard-rs"),
            "refusal must name the actionable set, got: {msg}"
        );
    }

    // what this catches (#432): validation and resolution staying ONE set once
    // the disk overlay is live. An authored on-disk recipe must be spawnable
    // the moment the file exists — if validate_recipe only consulted the
    // embedded set, every authored recipe would be refused at activity/spawn
    // while the projection happily resolved it (or vice versa). Also pins the
    // fail-loud arm: a malformed overlay file refuses NAMING the file, never a
    // silent skip.
    #[test]
    fn validate_recipe_accepts_overlay_authored_purpose_and_refuses_malformed_overlay() {
        let overlay = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            overlay.path().join("wordstats.json"),
            r#"{ "purpose": "bench/wordstats", "regions": [], "affordances": [] }"#,
        )
        .expect("write authored recipe");
        match validate_recipe("bench/wordstats", overlay.path()) {
            Ok(()) => {}
            Err(e) => panic!("authored overlay purpose must validate, got: {e}"),
        }

        std::fs::write(overlay.path().join("broken.json"), "{ not json")
            .expect("write malformed recipe");
        let err = validate_recipe("bench/wordstats", overlay.path())
            .expect_err("a malformed overlay file must refuse loudly");
        assert!(
            format!("{err}").contains("broken.json"),
            "refusal must name the malformed file, got: {err}"
        );
    }

    // what this catches: core call sites bind rooms by shipped CONSTANT; the
    // purpose string must come from the recipe JSON, resolved through
    // shipped_purpose — if the mapping breaks, dispatch would silently bind a
    // wrong (or no) purpose again.
    #[test]
    fn shipped_benchmark_constant_resolves_to_its_declared_purpose() {
        assert_eq!(
            crate::experience::source::RecipeExperienceSource::shipped_purpose(
                crate::experience::source::shipped::BENCHMARK_HARD_RS
            )
            .as_deref(),
            Some("benchmark/hard-rs")
        );
    }

    // what this catches: the recipe binding riding a category every reader agrees
    // on. If this constant drifts from what the purpose resolver filters for, a
    // spawned room binds its recipe into a category nobody reads and silently
    // projects as a plain chat — the failure is invisible at the write site.
    #[test]
    fn the_recipe_binding_rides_the_shared_wall_category() {
        assert_eq!(RECIPE_WALL_CATEGORY, "recipe");
    }

    // what this catches: spawning a benchmark named after an existing project
    // replaced its recipe and let round cleanup remove the project's citizens.
    #[test]
    fn spawn_preserves_an_existing_activity_recipe() {
        let mut post = airc_core::doctrine::WallPostPublished {
            room_id: RoomId::from_uuid(uuid::Uuid::from_u128(1)),
            post_id: uuid::Uuid::from_u128(2),
            category: RECIPE_WALL_CATEGORY.into(),
            body: r#"{"recipe":"project"}"#.into(),
            supersedes: None,
            published_by: airc_core::PeerId::from_u128(3),
            published_at_ms: 0,
        };
        assert!(validate_spawn_binding("team-project", "project", &[]).is_ok());
        assert!(
            validate_spawn_binding("team-project", "project", std::slice::from_ref(&post)).is_ok()
        );
        let error = validate_spawn_binding(
            "team-project",
            "benchmark/hard-rs",
            std::slice::from_ref(&post),
        )
        .expect_err("a new activity cannot overwrite the project's type");
        assert!(error.to_string().contains("Choose a new activity name"));
        post.body = "{not json".into();
        assert!(validate_spawn_binding("team-project", "project", &[post]).is_err());
    }

    // what this catches: spawn staying AiSafe. A citizen with an idea creating a
    // room for it is the ordinary act this whole model is built around; if this
    // ever regresses to Privileged, citizens go back to reusing whatever room they
    // are standing in, which is exactly the behaviour #274 exists to end.
    // what this catches: a room nobody has ever marked reading as LIVE and
    // UNPROTECTED. The default is load-bearing — if `archived` ever defaulted true,
    // every ordinary room would silently stop offering its work; if `protected`
    // defaulted true, nothing could ever be deleted and the verb would look broken.
    #[test]
    fn an_unmarked_room_is_live_and_unprotected() {
        let s = RoomStanding::default();
        assert!(!s.archived);
        assert!(!s.protected);
        assert!(s.note.is_none());
    }

    // what this catches: standing round-tripping through the wall body. These verbs
    // MERGE — archiving must not clear protection and vice versa — so the encoded
    // form has to carry both fields, not just the one the caller set.
    #[test]
    fn standing_round_trips_both_flags_so_the_verbs_can_merge() {
        let s = RoomStanding {
            archived: true,
            protected: true,
            note: Some("K3 bring-up finished; evidence".into()),
        };
        let encoded = serde_json::to_string(&s).expect("encode");
        let back: RoomStanding = serde_json::from_str(&encoded).expect("decode");
        assert!(back.archived && back.protected);
        assert_eq!(back.note.as_deref(), Some("K3 bring-up finished; evidence"));

        // and a body written by an older client that only knew one field still reads
        // — a missing flag is `false`, never a parse failure that would strand the room.
        let partial: RoomStanding =
            serde_json::from_str(r#"{"archived":true}"#).expect("forward-compatible");
        assert!(partial.archived && !partial.protected);
    }

    #[test]
    fn spawning_a_room_is_an_ordinary_citizen_act() {
        assert!(matches!(ActivitySpawn::ACCESS, AccessLevel::AiSafe));
        assert!(ActivitySpawn::NATIVE);
    }

    mod params {
        use super::*;
        use crate::experience::recipe::ExperienceRecipe;
        use std::collections::BTreeMap;

        // what this catches: the shipped benchmark recipe declares `academy` as its base —
        // a round is LEARNING and roots there by what it is, not by where the curator
        // stood. Losing this line would drop every round to a flat top-level room again.
        #[test]
        fn the_benchmark_recipe_roots_in_the_academy() {
            let recipe = crate::experience::recipe::ExperienceRecipe::from_json(include_str!(
                "../../../continuum-experience/src/experience/recipes/benchmark.json"
            ))
            .expect("shipped recipe parses"); // test: the embedded recipe is authored JSON
            assert_eq!(recipe.base.as_deref(), Some("academy"));
        }

        fn recipe_with_params() -> ExperienceRecipe {
            ExperienceRecipe::from_json(
                r#"{
                    "purpose": "bench/param-fixture",
                    "regions": [],
                    "affordances": [],
                    "params": {
                        "suite": { "default": "swe-lite", "doc": "which benchmark suite" },
                        "instances": { "default": 1, "doc": "how many instances to run" },
                        "disclose": { "default": true, "doc": "announce full capacity" }
                    }
                }"#,
            )
            .expect("fixture recipe parses")
        }

        // what this catches (#433): the zero-arg contract. "A zero-arg spawn
        // always works" is the whole point of default-carrying decls — if an
        // empty override map didn't resolve to exactly the declared defaults,
        // every existing caller (benchmark dispatch passes an empty map) would
        // break or silently under-describe its room.
        #[test]
        fn empty_overrides_resolve_to_the_declared_defaults() {
            let resolved = resolve_params(&recipe_with_params(), &BTreeMap::new())
                .expect("zero-arg spawn always works");
            assert_eq!(resolved["suite"], serde_json::json!("swe-lite"));
            assert_eq!(resolved["instances"], serde_json::json!(1));
            assert_eq!(resolved["disclose"], serde_json::json!(true));
        }

        // what this catches (#433): an override MERGING over the defaults, not
        // replacing them — a caller setting one knob must still get every other
        // knob's default on the binding, or the room stops being self-describing.
        #[test]
        fn an_override_merges_over_the_remaining_defaults() {
            let given = BTreeMap::from([("instances".to_string(), serde_json::json!(5))]);
            let resolved = resolve_params(&recipe_with_params(), &given).expect("valid override");
            assert_eq!(resolved["instances"], serde_json::json!(5));
            assert_eq!(resolved["suite"], serde_json::json!("swe-lite"));
        }

        // what this catches (#433): a typo'd param name silently vanishing.
        // If unknown names were dropped instead of refused, a caller targeting
        // "instance" (singular) would run the default count and never learn why
        // — the refusal must name the declared set WITH docs, the author's
        // actionable surface.
        #[test]
        fn an_unknown_name_is_refused_naming_the_declared_set() {
            let given = BTreeMap::from([("instance".to_string(), serde_json::json!(5))]);
            let err = resolve_params(&recipe_with_params(), &given)
                .expect_err("unknown param must refuse");
            let msg = format!("{err}");
            assert!(msg.contains("instance"), "names the offender: {msg}");
            assert!(msg.contains("instances"), "names the declared set: {msg}");
            assert!(
                msg.contains("how many instances to run"),
                "carries the docs so the caller can self-correct: {msg}"
            );
        }

        // what this catches (#433): the default-IS-the-type contract. A string
        // where the declared default is a number must refuse — otherwise the
        // binding publishes a value no reader can trust the shape of, and the
        // type error surfaces far from the caller who made it.
        #[test]
        fn a_value_of_the_wrong_json_type_is_refused() {
            let given = BTreeMap::from([("instances".to_string(), serde_json::json!("five"))]);
            let err = resolve_params(&recipe_with_params(), &given)
                .expect_err("type mismatch must refuse");
            let msg = format!("{err}");
            assert!(msg.contains("number"), "names the expected type: {msg}");
            assert!(msg.contains("string"), "names the supplied type: {msg}");
            assert!(msg.contains(r#""params": {"instances": 1}"#), "shows the form: {msg}");
        }

        // what this catches (Kimi, 2026-09-28): a schema passed where a value belongs
        // ({"enum": [...], "type": "string"} for a string) refused as a bare type mismatch.
        // The refusal says it is a schema and shows the value form.
        #[test]
        fn a_schema_passed_as_a_value_is_named_as_one() {
            let given = BTreeMap::from([(
                "suite".to_string(),
                serde_json::json!({"enum": ["swe-lite"], "type": "string"}),
            )]);
            let msg = format!(
                "{}",
                resolve_params(&recipe_with_params(), &given).expect_err("a schema is not a value")
            );
            assert!(msg.contains("pass the value itself"), "{msg}");
            assert!(msg.contains(r#""params": {"suite": "..."}"#), "{msg}");
        }

        // what this catches (#433): a parameterless recipe (every shipped one
        // today) refusing ANY override — and saying so plainly rather than
        // implying a declared set that doesn't exist.
        #[test]
        fn a_parameterless_recipe_refuses_any_override_plainly() {
            let bare = ExperienceRecipe::from_json(
                r#"{ "purpose": "bench/bare", "regions": [], "affordances": [] }"#,
            )
            .expect("bare recipe parses");
            assert!(resolve_params(&bare, &BTreeMap::new())
                .expect("no params, no overrides — fine")
                .is_empty());
            let err = resolve_params(
                &bare,
                &BTreeMap::from([("anything".to_string(), serde_json::json!(1))]),
            )
            .expect_err("override on a parameterless recipe must refuse");
            assert!(
                format!("{err}").contains("NO parameters"),
                "says the recipe declares nothing: {err}"
            );
        }

        // what this catches (#433): resolve_recipe honoring the registry's
        // later-wins-by-purpose law. An overlay file re-declaring a shipped
        // purpose must be the one whose param decls govern the spawn — if the
        // embedded copy won, authored param additions would silently not exist.
        #[test]
        fn resolve_recipe_returns_the_overlay_copy_when_purposes_collide() {
            let overlay = tempfile::tempdir().expect("tempdir");
            std::fs::write(
                overlay.path().join("chat.json"),
                r#"{
                    "purpose": "chat",
                    "regions": [],
                    "affordances": [],
                    "params": { "topic": { "default": "open", "doc": "what this chat is for" } }
                }"#,
            )
            .expect("write overlay chat");
            let resolved =
                resolve_recipe("chat", overlay.path()).expect("shipped purpose still resolves");
            assert!(
                resolved.params.contains_key("topic"),
                "the OVERLAY copy (with its param decls) must win, got params: {:?}",
                resolved.params.keys().collect::<Vec<_>>()
            );
        }

        // what this catches (#433): the SHIPPED benchmark recipe's declared
        // param types drifting from what benchmark dispatch actually sends
        // (suite: string, instances: array, team: array). Dispatch builds
        // this exact map shape at benchmark.rs — if a decl's default changed
        // JSON type, every run-room spawn would refuse at the type gate and
        // no unit test on either side alone would say why.
        #[test]
        fn shipped_benchmark_recipe_accepts_dispatchs_targeting_map() {
            let empty_overlay = tempfile::tempdir().expect("tempdir");
            let recipe = resolve_recipe("benchmark/hard-rs", empty_overlay.path())
                .expect("shipped benchmark recipe resolves");
            for declared in ["suite", "instances", "team", "budget"] {
                assert!(
                    recipe.params.contains_key(declared),
                    "shipped benchmark recipe must declare {declared:?} (the catalog's \
                     targeting set), got: {:?}",
                    recipe.params.keys().collect::<Vec<_>>()
                );
            }
            let dispatch_shaped = std::collections::BTreeMap::from([
                ("suite".to_string(), serde_json::json!("swe-bench-lite")),
                (
                    "instances".to_string(),
                    serde_json::json!(["sympy__sympy-24152"]),
                ),
                ("team".to_string(), serde_json::json!(["Asha", "Anwen"])),
            ]);
            let resolved = resolve_params(&recipe, &dispatch_shaped)
                .expect("dispatch's targeting map must type-check against the decls");
            assert_eq!(resolved["suite"], serde_json::json!("swe-bench-lite"));
            assert_eq!(
                resolved["budget"],
                serde_json::json!("4h"),
                "unset budget rides the declared default onto the binding"
            );
        }
    }
