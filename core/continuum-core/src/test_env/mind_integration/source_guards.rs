#![cfg(test)]
//! The mind's source guards: tree-wide scans that walk every crate's source through
//! [`crate::test_env::source_tree`] (core and each cut crate). They sat in the mind modules
//! they guard; the mind crate cannot name core's test environment, so they live here. Each
//! asserts exactly what it asserted before the move.

/// From `cognition::context_budget` (its sanctioned home for the calibration).
mod context_budget {
    /// what this catches: THE SIXTH TIME. This defect has been fixed and has come back at
    /// least five times — #45 (max_tokens clamps), #46 (per-tier context caps), #50 (single-
    /// source the effective window), #124 (`EVAL_LANE_CONTEXT = 16_384`, deleted), and then
    /// `EXAM_LANE_CTX = 16384` reintroduced the *same* constant in the *same file* three
    /// weeks later under a determinism justification. Doc comments and task entries did not
    /// stop it; each new author re-derives a "reasonable" number in good faith.
    ///
    /// So this is a RED TEST, not a doc. It scans the whole crate for the shape — a
    /// constant whose name says window/context/token/prompt/chars and whose value is a bare
    /// literal — and fails on a new one. Adding a bound is still allowed: put it on
    /// [`ContextBudget`] as a FRACTION, where it scales with the served window. If you are
    /// here because this test failed, that is the fix; do not add your name to the list above.
    ///
    /// A constant that genuinely is NOT a window-relative bound (a phrase length, a dedup key
    /// prefix, another model's stated REQUIREMENT) declares itself with a
    /// `context-budget-exempt: <why>` comment on the line above. Exemptions are cheap to write
    /// and permanently visible in review — which is the point. A silent exemption is not
    /// available, because silence is how this came back five times.
    ///
    /// Weakening this test is the same act as re-introducing the constant.
    #[test]
    fn no_new_hardcoded_context_or_prompt_size_constant_anywhere_in_the_crate() {
        // Crate-wide, not just cognition/: the 4096/2048 adapter floor (#46's own surface) sat
        // outside cognition and would not have been caught by a cognition-only scan.
        let named = |n: &str| {
            // Categorically not size bounds, so they don't need to spend an exemption line:
            // a duration (`*_MS`), an identifier (`*_ID`), or a vocabulary offset (`*_OFFSET`).
            if n.ends_with("_MS") || n.ends_with("_ID") || n.ends_with("_OFFSET") {
                return false;
            }
            ["WINDOW", "CONTEXT", "CTX", "TOKEN", "PROMPT", "CHARS"]
                .iter()
                .any(|k| n.contains(k))
        };
        let mut offenders = Vec::new();
        for file in crate::test_env::source_tree::crate_sources() {
            // Vendored upstream code (candle/llama ports) is not ours to restyle; its
            // constants are the upstream model's own defaults.
            if file.rel.split('/').any(|c| c == "vendored") {
                continue;
            }
            // This module IS the sanctioned home for the calibration; its doc table
            // quotes the very constants it replaced.
            if file.path.file_name().is_some_and(|f| f == "context_budget.rs") {
                continue;
            }
            let (path, src) = (&file.path, &file.text);
            let lines: Vec<&str> = src.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim_start();
                // Declared non-bound: the marker appears anywhere in the contiguous
                // comment block directly above (a reason worth writing often needs two
                // lines, and a doc comment may sit between it and the const).
                let exempt = lines[..i]
                    .iter()
                    .rev()
                    .take_while(|l| {
                        let t = l.trim_start();
                        t.starts_with("//") || t.is_empty()
                    })
                    .any(|l| l.contains("context-budget-exempt:"));
                if exempt {
                    continue;
                }
                // Any visibility, `pub(in crate::path)` included: a split by owner widens
                // a moved constant to that form, and a fixed prefix list lost it.
                let unscoped = match t.strip_prefix("pub") {
                    Some(after) if after.starts_with('(') => {
                        after.split_once(')').map_or(t, |(_, rest)| rest.trim_start())
                    }
                    Some(after) => after.trim_start(),
                    None => t,
                };
                let Some(rest) = unscoped.strip_prefix("const ") else {
                    continue;
                };
                let Some((name, tail)) = rest.split_once(':') else {
                    continue;
                };
                if !named(name.trim()) {
                    continue;
                }
                let Some((ty, value)) = tail.split_once('=') else {
                    continue;
                };
                // Only sizes and RATIOS. A &str / bool / Duration named "...CONTEXT..." is
                // not a bound.
                //
                // The float types are here because of a hole this test had for its whole
                // life, found 2026-08-20 the first time anyone wrote a window-relative
                // constant that wasn't a token COUNT: `WINDOW_COMPARABILITY_FACTOR: f64 =
                // 2.0` (#2339, mine). Name matched, value was a bare literal, and it
                // sailed through — because the type list stopped at the integers. A guard
                // against invented numbers that a `f64` annotation defeats teaches exactly
                // one lesson: type your magic number as a float. A ratio over the window is
                // as much a fresh guess as a count of its tokens, and is caught the same way.
                if !["u32", "usize", "u64", "i32", "i64", "f32", "f64"].contains(&ty.trim()) {
                    continue;
                }
                // A bare literal is the defect — decimal or float, since both are equally
                // guessed. An expression built from another named bound (`MIN_SERVE_CTX * 8`)
                // is a derivation, not a fresh guess.
                let v = value.trim().trim_end_matches(';').trim();
                let bare_literal = !v.is_empty()
                    && v.chars().all(|c| c.is_ascii_digit() || c == '_' || c == '.')
                    // `.` only as a decimal point: `1.5` yes, `A.b` / `1..2` no.
                    && v.matches('.').count() <= 1
                    && v.starts_with(|c: char| c.is_ascii_digit());
                if bare_literal {
                    offenders.push(format!(
                        "{}:{} — const {}: {} = {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        i + 1,
                        name.trim(),
                        ty.trim(),
                        v
                    ));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "hardcoded context/prompt size constant(s) reintroduced in cognition — express \
             these as a fraction on ContextBudget so they scale with the SERVED window \
             (a 4k-shaped constant makes a 1M-context model useless and deletes the reason \
             to run MoE at all):\n  {}",
            offenders.join("\n  ")
        );
    }
}

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
            // This file DEFINES it — not a production call site.
            if file.rel == "cognition/experience.rs" {
                continue;
            }
            if file.production().contains("record_lived_turn(") {
                callers.push(file.rel.clone());
            }
        }

        assert_eq!(
            callers.len(),
            1,
            "expected exactly ONE production caller of record_lived_turn (the \
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
