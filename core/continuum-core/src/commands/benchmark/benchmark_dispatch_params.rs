use super::*;

/// Compose the card BODY: the full prompt plus a definition of done a citizen
/// can act on with her own hands. Grading inputs that must stay held out
/// (`expect`, the harness `test`) are deliberately NOT written to the card —
/// the DoD names the artifact, not the answer key. `solution_file` and
/// `dod_shell` are legitimately visible: real work has a visible definition
/// of done.
pub(crate) fn dispatch_card_body(bench: &str, t: &crate::cognition::eval::EvalTask) -> String {
    let mut body = format!(
        "benchmark: {bench}\ntask: {}\n\n{}\n",
        t.id,
        t.prompt.trim()
    );
    if let Some(f) = &t.solution_file {
        body.push_str(&format!(
            "\nWrite your solution to `{f}` in your workspace (code/write)."
        ));
    }
    if let Some(dod) = &t.dod_shell {
        body.push_str(&format!(
            "\nDefinition of done: `{dod}` exits 0 in your workspace — run it \
             yourself (code/shell) and iterate until green."
        ));
    }
    body.push_str(
        "\n\nWhen your artifact is in place, mark the card done (work/state). \
         Your work is graded from what you actually wrote and ran.",
    );
    body
}

/// Compose the card BODY for a REAL SWE-bench project instance — a live open-source issue
/// on a real repo. The issue text plus a definition of done a citizen acts on with her own
/// git hands. The gold `patch` and the held-out `test_patch` are deliberately NOT written:
/// the card names the repo + commit + which tests must pass, never the answer key (same
/// held-out discipline as [`dispatch_card_body`]). This is the real-project tier — "full
/// projects, not stupid homework" (Joel, 2026-08-10).
pub(crate) fn dispatch_swe_card_body(
    bench: &str,
    i: &crate::cognition::swe_bench::SweInstance,
) -> String {
    let f2p = i.f2p();
    let tests = if f2p.is_empty() {
        "the repo's held-out failing tests".to_string()
    } else {
        f2p.join(", ")
    };
    // THE DEFINITION OF DONE LEADS. The card is a wall post, and a wall render keeps a
    // body PREFIX (wall_source: max_chars − reserved) — with the tests named after a long
    // problem statement, every holder's first turns went to "determine the FAIL_TO_PASS
    // tests" by reading the dataset (Kira, Atlas, 2026-09-13 11:2xZ) for a fact the card
    // already carried, below the cut.
    format!(
        "benchmark: {bench}\ninstance: {}\nrepo: {} @ {}\n\
         DEFINITION OF DONE: these tests pass — {}. THOSE TESTS ARE NOT IN YOUR CHECKOUT: the \
         grader adds them from a held-out test patch at verdict time, so do not search the tree \
         for them or try to run them by name — write your own repro from the issue text, and use \
         the tests that already exist in that file/module as the regression set. Do not edit \
         existing tests; your DIFF is graded against the repo's held-out suite.\n\n{}\n\n\
         This is a REAL open-source issue. When you CLAIM this card, the repo is staged in \
         your workspace at `swe/{}/` (checked out at the buggy commit), and while you hold the \
         card your hands (files, shell) are rooted AT that repo's root: paths are repo-relative \
         and `ls` lists the repo itself. Nothing starts by itself — you fix the bug IN PLACE with \
         your tools and prove it with your own repro. END EVERY WORK TURN with work/note (the \
         card's ledger: what you established, the competing hypotheses with the test that settles \
         each, the one unknown, the next test) — your next turn opens from it instead of \
         re-orienting.",
        i.instance_id,
        i.repo,
        i.base_commit,
        tests,
        i.problem_statement.trim(),
        i.instance_id,
    )
}

    pub(crate) enum CardWork {
        /// Gym: write a solution file; DoD is a compile/test shell (named in the body).
        Gym,
        /// SWE: the pulled instance. The CLAIM stages it into the claimer's
        /// workspace/swe/<instance> (`card_staging`).
        Swe {
            instance: Box<crate::cognition::swe_bench::SweInstance>,
        },
    }

    pub(crate) struct PreparedCard {
        pub(crate) title: String,
        pub(crate) body: String,
        pub(crate) work: CardWork,
    }

/// What one dispatch SELECTS from a suite. Pure data, so the same selection can be
/// replayed: `(suite, seed, sample)` is the replication contract the score is
/// published with, and `instances` is the explicit alternative to it.
#[derive(Debug, Clone, Default)]
pub(crate) struct CardSelection {
    pub(crate) instances: Option<Vec<String>>,
    pub(crate) sample: Option<u32>,
    pub(crate) seed: Option<u64>,
    /// Keep only instances whose harness speaks this language (`rust`, `go`, `php`,
    /// `ruby`, `javascript`, `java`, `c`; `python` for the SWE-bench family) — the
    /// Multilingual dataset mixes nine, and a round is one language's board.
    /// `None`/empty = every language.
    pub(crate) language: Option<String>,
}

/// THE ONE WRITER of benchmark cards: task + oracle only, projected into the
/// title/body a citizen sees and the grader keys on (`[bench <suite>] <task>: …`).
/// No side effects — no room, no board, no env pre-warm. `benchmark/import` (the
/// `benchmark/round` recipe's import step) is its one caller, so every round posts the
/// same cards for the same selection by construction.
pub(crate) async fn prepare_cards(
    spec: &BenchmarkSpec,
    selection: &CardSelection,
) -> Result<Vec<PreparedCard>, CommandError> {
    let prepared: Vec<PreparedCard> = if let Some(dataset) = spec.swe_dataset() {
        // Real-project tier: pull instances on demand (cached on first use), one
        // full-project card each. Reuses the SAME loader agent/solve grades against —
        // no second source of truth. THIS is what killed the "no runnable eval_set"
        // refusal that had blocked every SWE dispatch (Joel: "fix the goddamn thing").
        let mut instances = crate::cognition::swe_bench::load_dataset(dataset)
            .await
            .map_err(|e| {
                CommandError::Internal(format!("swe dataset '{dataset}' load failed: {e}"))
            })?;
        if let Some(lang) = selection.language.as_deref().filter(|l| !l.is_empty()) {
            let before = instances.len();
            instances.retain(|i| i.language() == lang);
            if instances.is_empty() {
                return Err(CommandError::Invalid(format!(
                    "no instance in '{dataset}' is `{lang}` (of {before}) — a language is \
                     what an instance's harness speaks: rust, go, php, ruby, javascript, \
                     java, c; python for the SWE-bench family"
                )));
            }
        }
        // Deterministic sample: (dataset, seed, n) → the same list on every
        // machine. Fisher-Yates over the dataset order with the shared LCG —
        // the command IS the replication recipe (no operator-side scripts).
        if let Some(n) = selection.sample.filter(|n| *n > 0) {
            if selection.instances.as_ref().is_some_and(|w| !w.is_empty()) {
                return Err(CommandError::Invalid(
                    "pass either `sample` (seeded random) or `instances` (explicit list),                          not both — they are competing selection recipes"
                        .into(),
                ));
            }
            let n = (n as usize).min(instances.len());
            let mut rng = crate::cognition::gym_rng::Lcg::new(selection.seed.unwrap_or(0)); // documented default seed: 0 is part of the replication contract, not a guess
            for i in 0..n {
                let j = i + rng.next(instances.len() - i);
                instances.swap(i, j);
            }
            instances.truncate(n);
            let list: Vec<&str> =
                instances.iter().map(|i| i.instance_id.as_str()).collect();
            tracing::info!(
                probe_class = "benchmark.dispatch.sample",
                dataset,
                n,
                seed = selection.seed.unwrap_or(0), // same documented default as the draw above
                instances = ?list,
                "seeded sample selected — publish (dataset, seed, n) with the score"
            );
        }
        // Caller-targeted instances win over dataset order — select by substring (so a
        // short id resolves) and preserve the CALLER's ordering, fail loud on a miss so a
        // typo never silently dispatches the wrong (or whole) set.
        if let Some(wanted) = selection.instances.as_ref().filter(|w| !w.is_empty()) {
            let mut picked: Vec<crate::cognition::swe_bench::SweInstance> = Vec::new();
            for want in wanted {
                match instances
                    .iter()
                    .position(|i| i.instance_id.contains(want.as_str()))
                {
                    Some(idx) => picked.push(instances.remove(idx)),
                    None => {
                        return Err(CommandError::Invalid(format!(
                            "no instance in '{dataset}' matches '{want}' — check the id (e.g. sympy__sympy-24152)"
                        )));
                    }
                }
            }
            instances = picked;
        }
        // ENV PRE-WARM (background): a cold native build (scikit's cython,
        // matplotlib's freetype) mid-round burns a solve attempt on an ENV
        // failure and reads as a model miss. Build every instance's env
        // AHEAD of the driver, in REVERSE card order so the warmer and the
        // solver approach from opposite ends (ensure_env itself holds the
        // per-instance lock, so even a meeting in the middle is safe).
        // Fire-and-forget: a prewarm failure is probed — the SAME failure
        // the solve would hit, surfaced hours earlier and attributable.
        instances
            .into_iter()
            .map(|i| PreparedCard {
                title: dispatch_card_title(spec.name, &i.instance_id, &i.problem_statement),
                body: dispatch_swe_card_body(spec.name, &i),
                work: CardWork::Swe {
                    instance: Box::new(i),
                },
            })
            .collect()
    } else {
        let reference = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!(
                "benchmark '{}' has no runnable eval_set yet — it is catalogued but its \
                 task collection hasn't been pulled/committed (see benchmark/list `runnable`)",
                spec.name
            ))
        })?;
        prepare_gym_cards(spec.name, reference)?
    };
    Ok(prepared)
}

