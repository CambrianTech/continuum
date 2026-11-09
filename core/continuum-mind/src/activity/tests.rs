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
        for purpose in crate::experience::source::RecipeExperienceSource::shipped_purposes().expect("the embedded recipes parse") {
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
            .expect("the shipped benchmark recipe resolves"),
            "benchmark/hard-rs"
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

        // what this catches: the shipped benchmark recipe declares `academy/benchmarks` as its
        // base: a run is LEARNING and lives under the academy's benchmarks by what it is, beside
        // its training and coursework, never a flat top-level room.
        #[test]
        fn the_benchmark_recipe_roots_in_the_academy() {
            let recipe = crate::experience::recipe::ExperienceRecipe::from_json(include_str!(
                "../../../continuum-experience/src/experience/recipes/benchmark.json"
            ))
            .expect("shipped recipe parses"); // test: the embedded recipe is authored JSON
            assert_eq!(recipe.base.as_deref(), Some("academy/benchmarks"));
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

    // THE ROOM TREE (ACTIVITY-ROOM-PATTERNS.md): every activity is a room at a path, opened by
    // resume-or-create, its ancestors rooms too, archived never deleted.
    mod room_tree {
        use super::*;
        use crate::experience::children::{RoomChildRecord, CHILD_WALL_CATEGORY};

        async fn scope() -> (tempfile::TempDir, Airc) {
            let home = tempfile::tempdir().expect("temp airc home");
            let airc = Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon");
            (home, airc)
        }

        async fn room_at(airc: &Airc, path: &str) -> airc_lib::Room {
            let key = crate::experience::room_path::RoomPath::parse(path).expect("a room path").channel_key();
            airc.subscribe_room(&key).await.expect("the room opens")
        }

        async fn recipe_of(airc: &Airc, path: &str) -> Option<String> {
            let room = room_at(airc, path).await;
            let posts = airc.wall_posts_in(&room, Some(RECIPE_WALL_CATEGORY)).await.expect("its wall reads");
            crate::experience::binding::project_binding(&posts).expect("its binding reads").map(|b| b.recipe)
        }

        async fn children_of(airc: &Airc, path: &str) -> Vec<String> {
            let room = room_at(airc, path).await;
            let posts = airc.wall_posts_in(&room, Some(CHILD_WALL_CATEGORY)).await.expect("its wall reads");
            posts.iter().map(|p| serde_json::from_str::<RoomChildRecord>(&p.body).expect("a child record").name).collect()
        }

        // what this catches: an activity's room losing its place in the tree. A name goes under
        // its recipe's base (a benchmark run under academy/benchmarks); a name holding a `/` is
        // already its path; a name with neither stands at the top.
        #[tokio::test]
        async fn a_name_goes_under_its_base_and_a_path_is_its_own_place() {
            let (_home, airc) = scope().await;
            assert_eq!(activity_path(&airc, "run-1", None, Some("academy/benchmarks")).await.expect("placed").as_str(), "academy/benchmarks/run-1");
            assert_eq!(activity_path(&airc, "/projects/demo/", None, Some("academy")).await.expect("placed").as_str(), "projects/demo");
            assert_eq!(activity_path(&airc, "settings", None, None).await.expect("placed").as_str(), "settings");
        }

        // what this catches: the ancestors of an activity not being rooms (a run whose
        // `academy/benchmarks` nobody can open or walk), opening that moves the caller's focus,
        // and re-opening that re-creates instead of resuming: a second binding, a second child
        // record, an archived run left archived.
        #[tokio::test]
        async fn opening_a_path_makes_its_ancestors_rooms_and_opening_it_again_resumes_it() {
            let (_home, airc) = scope().await;
            let focus = airc.join("where-she-stands").await.expect("her focus").channel;

            let made = spawn_activity_room(&airc, "projects/demo/notes", "chat", None, &Default::default(), None, None).await.expect("opened");
            assert_eq!((made.path.as_str(), made.opened), ("projects/demo/notes", ActivityOpened::Created));
            assert_eq!(airc.current_room().await.expect("focus reads").channel, focus, "opening never moves her focus");
            assert_eq!(recipe_of(&airc, "projects").await.as_deref(), Some(CATEGORY_RECIPE), "an ancestor is a category room");
            assert_eq!(recipe_of(&airc, "projects/demo").await.as_deref(), Some(CATEGORY_RECIPE));
            assert_eq!(children_of(&airc, "projects").await, vec!["projects/demo".to_string()], "each child is on its parent's wall");
            assert_eq!(children_of(&airc, "projects/demo").await, vec!["projects/demo/notes".to_string()]);

            let room = room_at(&airc, "projects/demo/notes").await;
            publish_standing_in(&airc, &room, &RoomStanding { archived: true, ..RoomStanding::default() }).await.expect("archived");
            let again = spawn_activity_room(&airc, "projects/demo/notes", "chat", None, &Default::default(), None, None).await.expect("reopened");
            assert_eq!((again.room_id, again.opened, again.binding_post_id), (made.room_id, ActivityOpened::Resumed, made.binding_post_id), "the same room, resumed");
            assert!(!standing_in(&airc, &room).await.expect("standing").archived, "resuming brings it back to live");
            assert_eq!(children_of(&airc, "projects/demo").await.len(), 1, "resuming announces nothing twice");

            let refused = spawn_activity_room(&airc, "projects/demo/notes", "project", None, &Default::default(), None, None).await;
            assert!(refused.is_err(), "a path already bound to one recipe is never re-bound to another");
        }

        // A room opened before the room tree: channel = the last segment, bound with no path.
        async fn pre_tree_room(airc: &Airc, leaf: &str, recipe: &str) -> airc_lib::Room {
            let room = airc.subscribe_room(leaf).await.expect("the pre-tree room opens");
            let binding = crate::experience::binding::RoomRecipeBinding { recipe: recipe.to_string(), parent: None, params: Default::default(), path: None };
            let body = serde_json::to_string(&binding).expect("a binding encodes");
            airc.publish_wall_post_in(&room, RECIPE_WALL_CATEGORY.to_string(), body, None).await.expect("bound");
            room
        }

        // what this catches: a run opened before the room tree being orphaned. Its channel was its
        // last segment; the tree keys a nested room by its whole path, so re-opening it by name
        // would make a second, empty room beside the first and leave its cards and board behind
        // (Cormac on #5878). It is adopted once (its path written, announced to its parent) and
        // resumed every time after; no tree-keyed room is ever made for it.
        #[tokio::test]
        async fn a_room_opened_before_the_tree_is_resumed_never_duplicated() {
            let (_home, airc) = scope().await;
            let old = pre_tree_room(&airc, "run-1", "chat").await;

            let first = spawn_activity_room(&airc, "projects/demo/run-1", "chat", None, &Default::default(), None, None).await.expect("opened");
            assert_eq!((first.room_id, first.opened), (old.channel, ActivityOpened::Resumed), "the pre-tree room is the activity, resumed");
            let posts = airc.wall_posts_in(&old, Some(RECIPE_WALL_CATEGORY)).await.expect("its wall reads");
            let binding = crate::experience::binding::project_binding(&posts).expect("its binding reads").expect("bound");
            assert_eq!(binding.path.as_ref().map(|p| p.as_str()), Some("projects/demo/run-1"), "adopting writes its path");
            assert_eq!(children_of(&airc, "projects/demo").await, vec!["projects/demo/run-1".to_string()], "and puts it on its parent's wall");

            let again = spawn_activity_room(&airc, "projects/demo/run-1", "chat", None, &Default::default(), None, None).await.expect("reopened");
            assert_eq!((again.room_id, again.binding_post_id), (old.channel, first.binding_post_id), "the same room and binding again");
            assert_eq!(children_of(&airc, "projects/demo").await.len(), 1, "adopted once, announced once");
            let key = crate::experience::room_path::RoomPath::parse("projects/demo/run-1").expect("a room path").channel_key();
            assert!(airc.room_by_name_or_channel(&key, "check").await.is_err(), "no second, tree-keyed room was made");
        }

        // what this catches: a second node making its own room for a path another node already
        // stands at (Cormac on #5878). This scope never held `run-3`; its parent's wall carries the
        // announcement another node made when it adopted that pre-tree room. Opening the path walks
        // to that room by the record, never subscribes a second, tree-keyed one.
        #[tokio::test]
        async fn a_node_that_never_held_the_room_walks_to_it_by_its_parents_record() {
            let (_home, airc) = scope().await;
            let path = crate::experience::room_path::RoomPath::parse("projects/demo/run-3").expect("a room path");
            let parent = open_ancestors(&airc, &path).await.expect("ancestors open").expect("a nested path has a parent");
            let elsewhere = airc_lib::Room::from_name(airc.home(), "run-3").expect("a channel derives").channel;
            let record = crate::experience::children::RoomChildRecord {
                room_id: elsewhere.as_uuid(), name: path.as_str().to_string(), recipe: "chat".into(), suite: None, spawned_at_ms: 1,
            };
            let body = serde_json::to_string(&record).expect("a record encodes");
            airc.publish_wall_post_in(&parent, CHILD_WALL_CATEGORY.to_string(), body, None).await.expect("announced");
            assert!(airc.room_by_name_or_channel("run-3", "check").await.is_err(), "this scope never held it");

            let opened = spawn_activity_room(&airc, path.as_str(), "chat", None, &Default::default(), None, None).await.expect("opened");
            assert_eq!(opened.room_id, elsewhere, "it lands in the room the parent announces");
            assert!(airc.room_by_name_or_channel(&path.channel_key(), "check").await.is_err(), "no second, tree-keyed room was made");
        }

        // what this catches: adoption taking over an unrelated room that shares the last segment.
        // A pre-tree room bound to another recipe stays as it is, and the path gets its own room.
        #[tokio::test]
        async fn a_pre_tree_room_of_another_recipe_is_never_adopted() {
            let (_home, airc) = scope().await;
            let other = pre_tree_room(&airc, "run-2", "project").await;
            let made = spawn_activity_room(&airc, "projects/demo/run-2", "chat", None, &Default::default(), None, None).await.expect("opened");
            assert_eq!(made.opened, ActivityOpened::Created);
            assert_ne!(made.room_id, other.channel, "the other recipe's room is left alone");
        }
    }
}
