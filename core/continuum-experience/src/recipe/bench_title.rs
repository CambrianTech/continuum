//! The bench CARD TITLE grammar: `[bench <name>] <task>: <gist>`. ONE writer
//! ([`dispatch_card_title`]) beside ONE parser ([`parse_card_title`]), so the key a
//! dispatch computes and the task identity read back off a live card cannot drift.
//!
//! It lives in the mind ring (moved from `commands/benchmark`, card 67af4fca) because the
//! citizen's own code reads it: the credit stamp records which task a turn worked
//! (`cognition::held_out`), and her act and card-holding paths parse it. The surface
//! commands call down into it.

/// The IDENTITY of a benchmark card: which task of which benchmark it is.
///
/// This is the ONE definition of "same task" on the board, and it is a strict
/// prefix of the rendered title — so the key a dispatch computes and the key
/// parsed back off a live card can never drift (pinned by
/// `a_rendered_title_yields_back_its_own_key`). `dispatch_card_title` builds on
/// it rather than re-forming the marker, per the compression rule: one logical
/// decision, one place.
pub fn dispatch_card_key(bench: &str, task_id: &str) -> String {
    format!("[bench {bench}] {task_id}:")
}

/// Recover the identity key from a rendered card title, or `None` when the
/// title is not a benchmark card at all (a hand-written card on the same board
/// must never collide with a task key). Reads the prefix through the FIRST `:`
/// after the `]` marker — a gist containing colons cannot widen the key.
pub fn bench_card_key(title: &str) -> Option<&str> {
    if !title.starts_with("[bench ") {
        return None;
    }
    let marker_end = title.find("] ")? + 2;
    let colon = title[marker_end..].find(':')? + marker_end;
    Some(&title[..=colon])
}

/// Compose the card TITLE for one benchmark task. `[bench <name>]` is the
/// machine-findable marker the (future) grading sentinel keys on; the rest is
/// for the citizen scanning the board.
pub fn dispatch_card_title(bench: &str, task_id: &str, prompt: &str) -> String {
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
pub fn parse_card_title(title: &str) -> Option<(String, String)> {
    let rest = title.strip_prefix("[bench ")?;
    let (bench, after) = rest.split_once("] ")?;
    let task = after.split(':').next()?.trim();
    if bench.trim().is_empty() || task.is_empty() {
        return None;
    }
    Some((bench.trim().to_string(), task.to_string()))
}

/// A review card's title: `[review <parent8>] <instance>: …` — ONE writer
/// (`modules::work::open_review_card`), one parser, beside the bench one.
pub fn review_card_title(parent: uuid::Uuid, instance: &str, owner: &str) -> String {
    format!(
        "[review {}] {instance}: review {owner}'s fix",
        &parent.simple().to_string()[..8]
    )
}

/// Parse a review card's title into the INSTANCE it reviews. `None` for any other
/// title, so a normal work card is never mistaken for a review.
pub fn parse_review_title(title: &str) -> Option<String> {
    let rest = title.strip_prefix("[review ")?;
    let (_, after) = rest.split_once("] ")?;
    let instance = after.split(':').next()?.trim();
    (!instance.is_empty()).then(|| instance.to_string())
}
