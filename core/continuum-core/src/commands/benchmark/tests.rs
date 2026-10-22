#![cfg(test)]
    // what this catches: c744's singleton deck was voiced as its recipient and
    // filtered as self-speech. The persisted announcement must retain its caller
    // and explicit room even when that caller subsequently focuses elsewhere.
    #[tokio::test]
    async fn citizen_deck_retains_command_author_and_target_room() {
        use crate::persona::airc_source::AircTranscriptReader;
        let home = tempfile::tempdir().expect("isolated author scope");
        let author = airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await.expect("local author without daemon");
        let room = author.join("bench-author-contract").await.expect("join activity");
        let lobby = author.join("general").await.expect("move ambient focus");
        let id = super::publish_citizen_deck(
            &author, room.channel.as_uuid(), "coder-write-eval", &["card-1".into()],
        ).await.expect("publish through caller authority");
        let events = AircTranscriptReader::page_recent_in(&author, Some(room.channel), 10)
            .await.expect("activity transcript");
        let event = events.iter().find(|e| e.event_id == id).expect("published deck in target room");
        assert_eq!(event.peer_id, author.peer_id());
        assert_eq!(event.room_id, room.channel);
        let body = serde_json::to_string(&event.body).expect("public announcement body");
        assert!(body.contains("card-1") && body.contains("claim_task <id>"));
        assert!(!body.contains("substrate claims"), "no automatic-claim promise");
        assert_eq!(author.current_room().await.expect("current room").channel, lobby.channel);
    }

    // what this catches: the recipe row's serde contract — a minimal hand-authored
    // `data/create` row (name + dispatches only) MUST parse, extra/unknown future
    // fields MUST be tolerated, and instances default empty. The recipe is data
    // authored by operators; a brittle parse turns a typo into a dead experiment
    // with no compiler to catch it (2026-08-29, the two-command flow).
    // what this catches: the template→instance substitution contract — `{model}`
    // resolves from params everywhere in the row, and an unresolved placeholder
    // is a LOUD error naming the key, never a literal "{model}" reaching a lane
    // pin (Joel 2026-08-29: the model is an ARGUMENT to the recipe, recipes are
    // templates — one "challenge" recipe serves every future model drop).
    #[test]
    fn recipe_templates_substitute_and_fail_loud_on_missing() {
        let row = serde_json::json!({
            "name": "challenge",
            "model_id": "{model}",
            "dispatches": [{"benchmark": "{dataset}", "instances": ["a__b-1"]}]
        });
        let mut params = std::collections::BTreeMap::new();
        params.insert("model".to_string(), "org/some-model".to_string());
        params.insert("dataset".to_string(), "swe-bench-lite".to_string());
        let out = BenchmarkDispatch::instantiate_recipe(row.clone(), &params).unwrap(); // unwrap: test asserts substitution succeeds
        assert_eq!(out["model_id"], "org/some-model");
        assert_eq!(out["dispatches"][0]["benchmark"], "swe-bench-lite");

        let err = BenchmarkDispatch::instantiate_recipe(row, &Default::default()).unwrap_err();
        assert!(format!("{err:?}").contains("model"), "names the missing key");
    }

    #[test]
    fn recipe_rows_parse_tolerantly() {
        let minimal: BenchmarkRecipe = serde_json::from_value(serde_json::json!({
            "name": "x",
            "dispatches": [{"benchmark": "swe-bench-lite"}]
        }))
        .expect("minimal row parses"); // expect: test asserts the serde contract
        assert!(minimal.model_id.is_none());
        assert!(minimal.dispatches[0].instances.is_empty());

        let rich: BenchmarkRecipe = serde_json::from_value(serde_json::json!({
            "name": "hard-eight",
            "description": "d",
            "model_id": "some/model",
            "dispatches": [
                {"benchmark": "swe-bench-lite", "instances": ["a__b-1"], "limit": 3},
                {"benchmark": "swe-bench-verified", "instances": ["c__d-2"]}
            ],
            "some_future_field": {"ignored": true}
        }))
        .expect("future fields tolerated"); // expect: test asserts tolerant parse
        assert_eq!(rich.dispatches.len(), 2);
        assert_eq!(rich.dispatches[0].limit, Some(3));
    }
    mod coverage_gate {
        use super::super::*;

        // what this catches: the gate turning into a BLOCKER for people who have
        // never run benchmark/validate. Joel's constraint, 2026-08-28: "we don't
        // slow them down, but we run checks before wasting the persona and
        // graders time." So every ambiguous case must DISPATCH: no map on disk, a
        // green class, or a map earned on a DIFFERENT machine class (a coverage
        // claim is only true for the platform that earned it). Only a class this
        // very box proved red may withhold a card — and then it must name the
        // wall, because a silent skip is the partial-dispatch lie the counters
        // exist to prevent.
        #[test]
        fn the_coverage_gate_fails_open_and_only_blocks_a_locally_proven_red_class() {
            let dataset = "swe-bench-unit-test-fixture";
            let here = BenchmarkPlatformFingerprint::capture().machine_class;
            // A TEMPDIR, never the operator's real ~/.continuum: writing a
            // fixture into a person's live data directory is both a lie about
            // isolation and a way to clobber their state — and it made this very
            // test pass alone and FAIL in the full suite.
            let root = std::env::temp_dir().join(format!("cov-gate-{}", uuid::Uuid::new_v4()));
            let path = coverage_map_path_in(&root, dataset, &here);

            // 1. No map at all — the fresh-clone case. Must dispatch.
            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2017),
                None,
                "a user who never ran validate must never be gated"
            );

            // 2. A map from THIS box: red class blocks (with its wall), green does not.
            let mut map = BenchmarkValidateResult {
                platform: BenchmarkPlatformFingerprint::capture(),
                classes: vec![
                    BenchmarkValidateClass {
                        repo: "astropy/astropy".into(),
                        era: "2017".into(),
                        representative: "astropy__astropy-6938".into(),
                        covers: 1,
                        green: false,
                        wall: Some("numpy 2 rejects copy=False".into()),
                    },
                    BenchmarkValidateClass {
                        repo: "django/django".into(),
                        era: "2022".into(),
                        representative: "django__django-15252".into(),
                        covers: 9,
                        green: true,
                        wall: None,
                    },
                ],
                instances_green: 9,
                dataset_size: 10,
                summary: String::new(),
            };
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d).unwrap();
            }
            std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();

            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2017).as_deref(),
                Some("numpy 2 rejects copy=False"),
                "a locally-proven red class must withhold the card AND name the wall"
            );
            assert_eq!(
                known_red_wall_in(&root, dataset, "django/django", 2022),
                None,
                "a green class must dispatch"
            );
            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2023),
                None,
                "an era with no row is not evidence of anything — dispatch"
            );

            // 3. A map earned on ANOTHER machine class must not gate this one.
            map.platform.machine_class = format!("not-{here}");
            std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();
            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2017),
                None,
                "another box's coverage claim is not evidence about this box"
            );

            let _ = std::fs::remove_dir_all(&root);
        }
    }

    use super::*;

    fn citizen(name: &str) -> (String, uuid::Uuid) {
        (
            name.to_string(),
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes()),
        )
    }

    // what this catches: #370. Every catalogued suite carried a real `source_url` and exactly
    // ONE of them could be pulled, because the only fetcher was fused to the SWE row shape.
    // This pins the derivation that makes the OTHER rows reachable — and pins that a non-HF
    // source (a GitHub raw .jsonl, of which the catalog has two) is REFUSED rather than
    // silently handed to the HF rows API, which would return an in-band error the caller
    // would read as "the suite is empty".
    #[test]
    // what this catches: a name collision in the catalog. Found live 2026-08-23:
    // the Terminal-Bench adapter's full row landed while the old stub row of the
    // same name survived — benchmark/list showed the name twice and fetch-by-name
    // silently took whichever matched first. A name IS the lookup key everywhere
    // (fetch, dispatch, verify), so duplicates are a routing hazard, not cosmetics.
    fn catalog_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for b in known_benchmarks() {
            assert!(
                seen.insert(b.name),
                "catalog name '{}' appears more than once — the name is the lookup \
                 key for fetch/dispatch/verify; merge the rows",
                b.name
            );
        }
    }

    #[test]
    fn every_hf_catalogued_suite_yields_coordinates_and_non_hf_is_refused() {
        let by_name = |n: &str| known_benchmarks().iter().find(|b| b.name == n).unwrap();

        assert_eq!(
            by_name("swe-bench-lite").reach(),
            SourceReach::Rows {
                dataset: "princeton-nlp/SWE-bench_Lite",
                config: "default",
                split: "test"
            }
        );
        assert!(
            matches!(by_name("humaneval").reach(), SourceReach::ForeignSource { .. }),
            "a GitHub raw .jsonl must NOT fall through to the HF rows API — that returns an \
             in-band error a caller reads as 'the suite is empty'"
        );
        assert!(matches!(by_name("hard-rs").reach(), SourceReach::InTree));

        // The two live-verified script datasets (2026-08-19): the rows API refuses these at
        // EVERY coordinate, so they must be told apart from a wrong-config miss or the
        // operator retries configs forever.
        for n in ["apps", "livecodebench"] {
            assert!(
                matches!(by_name(n).reach(), SourceReach::HuggingFaceScriptDataset { .. }),
                "`{n}` is a loading-script dataset; classifying it as fetchable sends the \
                 caller into a config-guessing loop that can never succeed"
            );
        }

        // what this catches specifically: bigcodebench publishes REVISIONS as splits and has
        // no `test` split at all. The default coordinates return "Unexpected error" — measured
        // live — so the version we score against has to be a recorded catalog fact.
        assert_eq!(
            by_name("bigcodebench").reach(),
            SourceReach::Rows {
                dataset: "bigcode/bigcodebench",
                config: "default",
                split: "v0.1.4"
            }
        );

        // And no catalogued row may be silently unclassifiable.
        for b in known_benchmarks() {
            let reach = b.reach();
            if let SourceReach::Rows { dataset, .. } = reach {
                assert!(
                    dataset.contains('/'),
                    "`{}` resolves to `{dataset}`, which the rows API cannot address",
                    b.name
                );
            }
        }
    }

    // what this catches: THE round-killer of 2026-08-18. Citizens registered but not yet
    // hosted made every readiness surface report a ready roster, and dispatch staged a full
    // round into a room where nobody had a perception stream — `dispatched: 2, kickoffs: 2,
    // kickoff_errors: []`, zero turns. Resolving against RESIDENCY must refuse instead, and
    // the refusal must say WHICH state this is, because the two have opposite fixes:
    // registered-but-not-resident is a WAIT, unregistered is `persona/spawn`.
    #[test]
    fn registered_but_not_resident_refuses_and_names_the_wait() {
        let registered = vec![citizen("Atlas"), citizen("Benchy")];
        let err = resolve_dispatch_roster(&[], &registered, &[]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            matches!(err, CommandError::Denied(_)),
            "staging into an empty room is denied, not a soft warning: {msg}"
        );
        assert!(
            msg.contains("Atlas") && msg.contains("Benchy"),
            "the refusal names WHO is registered so the operator can wait on them: {msg}"
        );
        assert!(
            msg.contains("NOT RESIDENT"),
            "and says the state plainly, not 'not online' (which reads as a typo): {msg}"
        );
    }

    // what this catches: the OTHER arm must stay distinguishable. Nobody registered at all
    // is a different problem with a different fix — `persona/spawn`, not a wait — and
    // collapsing the two would send an operator to wait forever for a citizen who was
    // never born.
    #[test]
    fn nobody_registered_at_all_points_at_spawn_not_at_waiting() {
        let err = resolve_dispatch_roster(&[], &[], &[]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("persona/spawn"), "names the actual fix: {msg}");
        assert!(
            !msg.contains("NOT RESIDENT"),
            "must NOT claim a residency wait when there is nobody to wait for: {msg}"
        );
    }

    // what this catches: a named assignee who is registered but not resident must not be
    // reported as a typo. "not online" against a name the operator can see in
    // `persona/roster` sends them hunting for a misspelling that does not exist — the same
    // lie as the round-level one, in a smaller box.
    #[test]
    fn a_named_assignee_who_is_not_resident_is_told_apart_from_a_typo() {
        let resident = vec![citizen("Atlas")];
        let registered = vec![citizen("Atlas"), citizen("Benchy")];

        let err =
            resolve_dispatch_roster(&resident, &registered, &["Benchy".into()]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("registered but not resident") && msg.contains("not a typo"),
            "a real citizen who is merely unhosted must be named as a WAIT: {msg}"
        );

        // ...while a genuine typo still reads as one, with no residency excuse attached.
        let err = resolve_dispatch_roster(&resident, &registered, &["Atals".into()]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            !msg.contains("not a typo"),
            "an unknown name gets no residency note — it IS a typo: {msg}"
        );
    }

    // what this catches: with everyone resident, dispatch behaves exactly as before —
    // empty request → the whole resident roster, in order. The residency gate must not
    // narrow the happy path (the default dispatch is what actuates a round at all).
    #[test]
    fn all_resident_dispatches_the_whole_roster_in_order() {
        let all = vec![citizen("Atlas"), citizen("Benchy")];
        let got = resolve_dispatch_roster(&all, &all, &[]).unwrap();
        assert_eq!(got, all, "empty request → everyone resident, order preserved");

        let got = resolve_dispatch_roster(&all, &all, &["Benchy".into()]).unwrap();
        assert_eq!(got, vec![citizen("Benchy")], "named assignee resolves");
    }

    // what this catches: a derived run-room name that airc REFUSES. `ChannelName::new` accepts
    // only `[a-z0-9_-]`, so a benchmark named with a `/`, a `.` or a capital (`swe-bench/lite`,
    // `humaneval-rs.v2`) would build a name that fails at `join` — dispatch would die at the
    // room, AFTER the caller believes a run started. Asserting through airc's own constructor
    // rather than a hand-copied charset, so the two can never drift.
    #[test]
    fn a_derived_run_room_name_is_always_a_name_airc_accepts() {
        for bench in [
            "humaneval-rs",
            "swe-bench/lite",       // the `/` the design-of-record path form wants
            "HumanEval.Rs v2",      // capitals, a dot, and a space
            "tool_bugfix_rs",
        ] {
            let name = default_run_room_name(bench, 1_786_000_000);
            airc_lib::ChannelName::new(&name)
                .unwrap_or_else(|e| panic!("derived room {name:?} from {bench:?} is unusable: {e}"));
        }
        // Stamped, so two runs of one benchmark are two rooms — the whole point.
        assert_ne!(
            default_run_room_name("humaneval-rs", 1),
            default_run_room_name("humaneval-rs", 2),
        );
    }

    // what this catches: the catalog is non-empty, humaneval-rs is present + runnable (has an
    // eval_set the grader understands), and every runnable benchmark names a real committed gym.
    // Guards the persona-callable competition surface from a stale/broken catalog row.
    #[test]
    fn catalog_has_a_runnable_committed_benchmark() {
        let ks = known_benchmarks();
        assert!(!ks.is_empty(), "the benchmark catalog must not be empty");
        let hr = ks
            .iter()
            .find(|b| b.name == "humaneval-rs")
            .expect("humaneval-rs catalogued");
        assert!(hr.eval_set.is_some(), "humaneval-rs must be runnable");
        assert!(matches!(hr.grader, Grader::Rust));
        // Every COMMITTED gym's eval_set must resolve through the gym resolver.
        // A FETCHED gym — the new class ds-1000 introduced: eval_set AND source_url
        // both present — legitimately resolves only after `benchmark/fetch` converts
        // it into the gym cache, so on a fresh checkout its eval_set correctly does
        // NOT resolve. The invariant for that class is different and equally pinned:
        // its eval_set must NOT shadow a committed gym (a name collision would make
        // the fetch silently overwrite a bundled suite's identity).
        for b in ks.iter().filter(|b| b.eval_set.is_some()) {
            let fetched = b.source_url.is_some();
            match crate::cognition::gym::resolve_gym(b.eval_set.unwrap()) {
                Ok((origin, _)) if fetched => assert!(
                    !origin.starts_with("embedded:"),
                    "fetched benchmark '{}' shadows committed gym '{origin}'",
                    b.name
                ),
                Ok(_) => {}
                Err(e) if fetched => assert!(
                    e.contains("could not be resolved"),
                    "fetched benchmark '{}' failed oddly: {e}",
                    b.name
                ),
                Err(e) => panic!("benchmark '{}' eval_set does not resolve: {e}", b.name),
            }
        }
    }

    // The dispatch idempotence key (#417). These pin the ONE property the gate
    // rests on: the key a dispatch computes for a task and the key parsed back
    // off that task's own live card are the same string. If they ever diverge,
    // dedupe silently stops working and the board refills with duplicates —
    // which is exactly the state these tests were written from (124 bench cards
    // for 51 distinct tasks, sympy__sympy-24152 holding 15 of them).
    mod dispatch_identity {
        use super::*;
        use crate::cognition::bench_title::dispatch_card_key;

        // what this catches: key/title drift. The key MUST be a prefix of the
        // rendered title, so a card on the board can be matched back to the task
        // that would produce it. A future edit to either function that breaks the
        // prefix relation fails here instead of silently duplicating cards.
        #[test]
        fn a_rendered_title_yields_back_its_own_key() {
            for (bench, task, prompt) in [
                ("swe-bench-lite", "sympy__sympy-24152", "Bug in expand of TensorProduct"),
                ("hard-rs", "rle_roundtrip", "Implement run-length encoding and decoding"),
                // A prompt long enough to be truncated with an ellipsis.
                ("frontier-rs", "dijkstra", &"x".repeat(200)),
                // A prompt containing colons must not widen the key past the FIRST one.
                ("hard-rs", "spiral_order", "note: returns Vec<i32>: in spiral order"),
            ] {
                let title = dispatch_card_title(bench, task, prompt);
                let key = dispatch_card_key(bench, task);
                assert!(
                    title.starts_with(&key),
                    "key must be a prefix of its own title\n  title: {title}\n  key:   {key}"
                );
                assert_eq!(
                    bench_card_key(&title),
                    Some(key.as_str()),
                    "parsing a rendered title must recover exactly the constructed key ({title})"
                );
            }
        }

        // what this catches: two DIFFERENT tasks (or the same task id under two
        // different benchmarks) must never collide onto one key — a collision
        // would suppress a legitimate card as a false duplicate.
        #[test]
        fn distinct_tasks_never_share_a_key() {
            let a = dispatch_card_key("swe-bench-lite", "sympy__sympy-24152");
            let b = dispatch_card_key("swe-bench-lite", "sympy__sympy-24066");
            let c = dispatch_card_key("swe-bench-verified", "sympy__sympy-24152");
            assert_ne!(a, b, "different task ids must differ");
            assert_ne!(a, c, "same task under a different benchmark must differ");
        }

        // what this catches: a hand-written card sharing the board must never be
        // read as a benchmark task key. `bench_card_key` returning Some for an
        // ordinary card would let unrelated work suppress a real dispatch.
        #[test]
        fn a_non_benchmark_card_has_no_task_key() {
            for title in [
                "Stage-on-claim: SWE checkout follows the CLAIMER",
                "[not-a-bench] whatever: text",
                "",
                // Marker present but no colon at all — not a dispatchable card.
                "[bench swe-bench-lite] malformed title with no colon",
            ] {
                assert_eq!(
                    bench_card_key(title),
                    None,
                    "non-benchmark title must yield no key: {title:?}"
                );
            }
        }
    }

    // The prune's selection rule. These guard a DESTRUCTIVE operation, so the
    // bar is: never close work someone is doing, and never resolve a genuine
    // two-citizen collision by cancelling one of them.
    mod duplicate_selection {
        use super::*;
        use crate::persona::card_holder::Hold;

        // what this catches: THE unacceptable failure — closing a card a citizen
        // is actively working. Every arrangement of a held card among duplicates
        // must leave that card open.
        #[test]
        fn a_held_card_is_never_closed() {
            for holds in [
                vec![Hold::Held, Hold::Unclaimed],
                vec![Hold::Unclaimed, Hold::Held],
                vec![Hold::Unclaimed, Hold::Held, Hold::Lapsed],
                vec![Hold::Lapsed, Hold::Lapsed, Hold::Held],
            ] {
                let held: Vec<usize> = holds
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| matches!(h, Hold::Held))
                    .map(|(i, _)| i)
                    .collect();
                let (to_close, _) = duplicates_to_close(&holds);
                for h in held {
                    assert!(
                        !to_close.contains(&h),
                        "index {h} is HELD and must never be closed (holds={holds:?}, \
                         to_close={to_close:?})"
                    );
                }
                // And the prune must actually do something about the rest.
                assert_eq!(
                    to_close.len(),
                    holds.len() - 1,
                    "exactly one card survives when a single card is held"
                );
            }
        }

        // what this catches: two citizens genuinely on the same task. Cancelling
        // either would destroy real in-flight work, so BOTH survive and the caller
        // is told it is contended.
        #[test]
        fn a_contended_task_keeps_every_holder_and_is_reported() {
            let holds = vec![Hold::Held, Hold::Held, Hold::Unclaimed, Hold::Lapsed];
            let (to_close, contended) = duplicates_to_close(&holds);
            assert!(contended, "two live claims on one task must report contention");
            assert_eq!(
                to_close,
                vec![2, 3],
                "only the unheld duplicates are closed; both holders survive"
            );
        }

        // what this catches: the ordinary case — nobody is on any of them, so the
        // choice only has to be deterministic. Keep the first.
        #[test]
        fn unheld_duplicates_collapse_to_the_first() {
            let holds = vec![Hold::Unclaimed, Hold::Unclaimed, Hold::Lapsed];
            let (to_close, contended) = duplicates_to_close(&holds);
            assert_eq!(to_close, vec![1, 2]);
            assert!(!contended);
        }

        // what this catches: a task with ONE card is not a duplicate and must be
        // left completely alone — a prune that closed singletons would empty the
        // board instead of deduplicating it.
        #[test]
        fn a_single_card_is_never_touched() {
            for holds in [vec![], vec![Hold::Unclaimed], vec![Hold::Held], vec![Hold::Lapsed]] {
                let (to_close, contended) = duplicates_to_close(&holds);
                assert!(to_close.is_empty(), "singleton/empty must yield no closes: {holds:?}");
                assert!(!contended);
            }
        }
    }
