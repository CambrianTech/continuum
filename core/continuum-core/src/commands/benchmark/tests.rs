#![cfg(test)]
    use super::*;
    use crate::recipe::benchmark_catalog::Grader;

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
            source_reach(by_name("swe-bench-lite")),
            SourceReach::Rows {
                dataset: "princeton-nlp/SWE-bench_Lite",
                config: "default",
                split: "test"
            }
        );
        assert!(
            matches!(source_reach(by_name("humaneval")), SourceReach::ForeignSource { .. }),
            "a GitHub raw .jsonl must NOT fall through to the HF rows API — that returns an \
             in-band error a caller reads as 'the suite is empty'"
        );
        assert!(matches!(source_reach(by_name("hard-rs")), SourceReach::InTree));

        // The two live-verified script datasets (2026-08-19): the rows API refuses these at
        // EVERY coordinate, so they must be told apart from a wrong-config miss or the
        // operator retries configs forever.
        for n in ["apps", "livecodebench"] {
            assert!(
                matches!(source_reach(by_name(n)), SourceReach::HuggingFaceScriptDataset { .. }),
                "`{n}` is a loading-script dataset; classifying it as fetchable sends the \
                 caller into a config-guessing loop that can never succeed"
            );
        }

        // what this catches specifically: bigcodebench publishes REVISIONS as splits and has
        // no `test` split at all. The default coordinates return "Unexpected error" — measured
        // live — so the version we score against has to be a recorded catalog fact.
        assert_eq!(
            source_reach(by_name("bigcodebench")),
            SourceReach::Rows {
                dataset: "bigcode/bigcodebench",
                config: "default",
                split: "v0.1.4"
            }
        );

        // And no catalogued row may be silently unclassifiable.
        for b in known_benchmarks() {
            let reach = source_reach(b);
            if let SourceReach::Rows { dataset, .. } = reach {
                assert!(
                    dataset.contains('/'),
                    "`{}` resolves to `{dataset}`, which the rows API cannot address",
                    b.name
                );
            }
        }
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
        use crate::recipe::bench_title::{bench_card_key, dispatch_card_key};

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

