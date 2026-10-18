//! Card identity for dispatched benchmark tasks: the card key, title and body shapes, and their parsers, so the dispatcher, the run projection and review cards agree on one encoding.


/// The IDENTITY of a benchmark card: which task of which benchmark it is.
///
/// This is the ONE definition of "same task" on the board, and it is a strict
/// prefix of the rendered title — so the key a dispatch computes and the key
/// parsed back off a live card can never drift (pinned by
/// `a_rendered_title_yields_back_its_own_key`). `dispatch_card_title` builds on
/// it rather than re-forming the marker, per the compression rule: one logical
/// decision, one place.
pub(crate) fn dispatch_card_key(bench: &str, task_id: &str) -> String {
    format!("[bench {bench}] {task_id}:")
}

/// Recover the identity key from a rendered card title, or `None` when the
/// title is not a benchmark card at all (a hand-written card on the same board
/// must never collide with a task key). Reads the prefix through the FIRST `:`
/// after the `]` marker — a gist containing colons cannot widen the key.
pub(crate) fn bench_card_key(title: &str) -> Option<&str> {
    if !title.starts_with("[bench ") {
        return None;
    }
    let marker_end = title.find("] ")? + 2;
    let colon = title[marker_end..].find(':')? + marker_end;
    Some(&title[..=colon])
}

/// Which of a task's duplicate cards to CLOSE, and whether the duplication is
/// contended. Pure over the claim states so the rule is testable without a
/// board — the caller maps the returned indices back to cards.
///
/// The rule, in priority order:
///  • A card someone is genuinely ON (`Hold::Held`) is NEVER closed. Duplicates
///    are board litter; an in-flight claim is a citizen's work, and destroying
///    it to tidy up would cost more than the duplication does.
///  • If no card is held, keep the FIRST and close the rest — they are
///    interchangeable, so the choice only has to be deterministic.
///  • If MORE THAN ONE card is held, every held card is kept and the caller is
///    told (`contended`). Two citizens really are on the same task; that is a
///    coordination fact for them to settle, not something a prune should
///    silently resolve by cancelling someone.
pub(super) fn duplicates_to_close(holds: &[crate::persona::card_holder::Hold]) -> (Vec<usize>, bool) {
    use crate::persona::card_holder::Hold;
    if holds.len() <= 1 {
        return (Vec::new(), false);
    }
    let held: Vec<usize> = holds
        .iter()
        .enumerate()
        .filter(|(_, h)| matches!(h, Hold::Held))
        .map(|(i, _)| i)
        .collect();
    let to_close = if held.is_empty() {
        (1..holds.len()).collect()
    } else {
        (0..holds.len()).filter(|i| !held.contains(i)).collect()
    };
    (to_close, held.len() > 1)
}

/// Compose the card TITLE for one benchmark task. `[bench <name>]` is the
/// machine-findable marker the (future) grading sentinel keys on; the rest is
/// for the citizen scanning the board.
pub(crate) fn dispatch_card_title(bench: &str, task_id: &str, prompt: &str) -> String {
    let gist: String = prompt.chars().take(60).collect();
    let ellipsis = if prompt.chars().count() > 60 {
        "…"
    } else {
        ""
    };
    format!("{} {gist}{ellipsis}", dispatch_card_key(bench, task_id))
}

/// Parse `[bench <name>] <task>: <gist>` — the exact shape [`dispatch_card_title`]
/// writes — into `(bench_name, task_id)`. `None` for any non-bench title, so a normal
/// work card is neither graded nor staged. ONE parser beside the ONE writer: the grade
/// edge (`benchmark_grade`) and the on-claim staging (`card_staging`) both read this.
pub(crate) fn parse_card_title(title: &str) -> Option<(String, String)> {
    let rest = title.strip_prefix("[bench ")?;
    let (bench, after) = rest.split_once("] ")?;
    let task = after.split(':').next()?.trim();
    if bench.trim().is_empty() || task.is_empty() {
        return None;
    }
    Some((bench.trim().to_string(), task.to_string()))
}

/// A review card's title: `[review <parent8>] <instance>: …` — ONE writer
/// ([`crate::modules::work::open_review_card`]), one parser, beside the bench one.
pub(crate) fn review_card_title(parent: uuid::Uuid, instance: &str, owner: &str) -> String {
    format!(
        "[review {}] {instance}: review {owner}'s fix",
        &parent.simple().to_string()[..8]
    )
}

/// Parse a review card's title into the INSTANCE it reviews. `None` for any other
/// title, so a normal work card is never mistaken for a review.
pub(crate) fn parse_review_title(title: &str) -> Option<String> {
    let rest = title.strip_prefix("[review ")?;
    let (_, after) = rest.split_once("] ")?;
    let instance = after.split(':').next()?.trim();
    (!instance.is_empty()).then(|| instance.to_string())
}

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
