#![cfg(test)]
//! The mind's source guards: tree-wide scans that walk every crate's source through
//! [`crate::test_env::source_tree`] (core and each cut crate). They sat in the mind modules
//! they guard; the mind crate cannot name core's test environment, so they live here. Each
//! asserts exactly what it asserted before the move.

/// From `cognition::experience` (the lived-turn producer's one caller).
mod experience {
    // what this catches: the lived-turn producer wired per-CALL-SITE again.
    //
    // The regression this pins is not hypothetical — it is what shipped. The
    // producer was first called from ONE of the three `drive_to_settle` sites in
    // `service_loop`, so the self-tick and held-work paths settled turns that no
    // record ever described, and zero `LivedTurn` records existed on disk while
    // citizens were demonstrably deliberating. The fix moved the call INTO the
    // settle driver, the one place a `SettleOutcome` is born.
    //
    // So the invariant is a COUNT, not a location-check: exactly one production
    // caller, and it is the driver. A second caller is either a double-record or
    // a path that opted itself out of learning; both are the same defect class
    // ([[the-same-bug-at-two-sites-is-a-missing-constraint]]).
    #[test]
    fn the_lived_turn_producer_has_exactly_one_production_caller_the_settle_driver() {
        // Production code only (`source_tree`): prose naming the function — this
        // very comment, and the doc on `record_lived_turn` — never reads as a call,
        // and neither does a test module, inline or moved to a sibling file.
        let files = crate::test_env::source_tree::crate_sources();
        assert!(
            !files.is_empty(),
            "the source walk found nothing — the guard \
            would be vacuously green, which is worse than red"
        );

        let mut callers: Vec<String> = Vec::new();
        for file in &files {
            // The slot's own file DEFINES the call; the recorder is installed into it, never
            // called directly from cognition's core.
            if file.rel == "cognition/extensions.rs" {
                continue;
            }
            if file.production().contains("extensions::lived_turn_settled(") {
                callers.push(file.rel.clone());
            }
        }

        assert_eq!(
            callers.len(),
            1,
            "expected exactly ONE production caller of lived_turn_settled (the \
             settle driver); found {}: {callers:?}. If you are adding a call at a \
             turn call site, don't — `drive_to_settle` already records every \
             outcome it produces, so a second call double-writes her stream. If \
             you are adding a NEW way to settle a turn that bypasses the driver, \
             that is the thing to reconsider.",
            callers.len()
        );
        // The settle module, whichever of its files holds the driver: splitting
        // `settle.rs` by owner moves the call without changing who makes it. `rel` is
        // forward-slashed on every OS, so this compare holds on Windows.
        assert!(
            callers[0] == "cognition/act_observe/settle.rs"
                || callers[0].starts_with("cognition/act_observe/settle/"),
            "the one caller must be the settle driver — the only place a \
             SettleOutcome is born; found {}",
            callers[0]
        );
    }
}

/// From `cognition::swe_bench` (the env root's one spelling).
mod swe_bench {
    // what this catches: a SECOND swe env root. On 2026-08-17 two roots existed —
    // `swe_cache_dir()/envs` (live, 46 envs / 8 repos) and `~/.continuum/cache/swe-envs`
    // (retired, 14 envs / 3 repos, the legacy python default). Nothing failed: each root was
    // internally consistent, so whichever you `ls`ed answered confidently. Reading the retired
    // one produced "77% of staged instances have no environment", which was reported as the
    // benchmark's root cause and had a design approved on it. Truth was 95% coverage.
    //
    // A cache with two roots cannot report its own coverage. So: the env root is derived from
    // swe_cache_dir() and appears as a path literal NOWHERE else in the crate. If you need the
    // envs dir, call the function. See swe_cache_dir's doc for the full incident.
    #[test]
    fn the_swe_env_root_has_exactly_one_spelling() {
        let files = crate::test_env::source_tree::crate_sources();

        // The retired root, and any hand-spelled sibling of the live one. ASSEMBLED at
        // runtime, never written whole: a literal needle would match its own declaration and
        // the guard would fail on itself (it did, first run). Comments are stripped too, so
        // this test's prose and swe_cache_dir's doc can name the paths freely.
        let retired = format!("cache/{}-envs", "swe");
        let live_spelled_out = format!("benchmarks/{}/envs", "swe");
        let banned = [retired.as_str(), live_spelled_out.as_str()];
        let mut hits = Vec::new();
        for file in &files {
            for (n, raw) in file.text.lines().enumerate() {
                let code = crate::test_env::source_tree::code_only(raw);
                for b in banned {
                    if code.contains(b) {
                        hits.push(format!("{}:{} → {}", file.rel, n + 1, code.trim()));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "a SECOND spelling of the swe env root appeared — this is exactly how the \
             2026-08-17 coverage misdiagnosis happened (two roots, both real, neither naming \
             the other, opposite answers to the same question). Derive it from \
             `swe_cache_dir()` instead of writing the path:\n  {}",
            hits.join("\n  ")
        );
    }
}
