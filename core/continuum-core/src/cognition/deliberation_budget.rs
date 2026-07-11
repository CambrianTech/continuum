//! Fitting the deliberation prompt into the served context window.
//!
//! Pure token-accounting + message-line rendering, lifted out of
//! [`super::llm_deliberation_faculty`] so the faculty owns *deciding what to send*
//! and this owns *measuring and trimming it to fit*. All functions are pure and
//! independently testable — the window guard is load-bearing (an over-budget prompt
//! is a hard `n_ctx` overshoot → a 400 from llama-server that mutes the persona for
//! the whole tick), so it earns its own home + tests.

use super::workspace::BurstTurn;

/// Chars-per-token divisor for the conservative window estimate. English is ~4
/// chars/token, but the deliberation prompt carries UUID-dense rosters, structured
/// engram observations, and code, which tokenize far denser — so we OVER-count
/// tokens (divide by 3, not 4) to stay safely under `n_ctx`. The completion reserve
/// absorbs the remaining slack.
pub(super) const GUARD_CHARS_PER_TOKEN: usize = 3;

/// Conservative token estimate for the window guard (see [`GUARD_CHARS_PER_TOKEN`]).
pub(super) fn est_tokens(s: &str) -> usize {
    s.len() / GUARD_CHARS_PER_TOKEN
}

/// Keep the TAIL of `s` that fits `budget_tokens`, cutting at a line boundary so a
/// trimmed message starts on a clean line (never mid-line). The latest lines — the
/// turn's most recent activity — always survive; the head is what gets dropped. Used
/// by `messages_within` to trim the single message that straddles the served-window
/// budget.
pub(super) fn tail_to_tokens(s: &str, budget_tokens: usize) -> String {
    let budget_chars = budget_tokens.saturating_mul(GUARD_CHARS_PER_TOKEN);
    if s.len() <= budget_chars {
        return s.to_string();
    }
    let mut start = s.len().saturating_sub(budget_chars);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    let slice = &s[start..];
    match slice.find('\n') {
        Some(nl) => slice[nl + 1..].to_string(),
        None => slice.to_string(),
    }
}

/// Render ONE burst turn as the body line for its chat message. The persona's own
/// turns and opaque (authorless) turns render verbatim — her own voice carries no
/// name prefix (the system prompt forbids self-prefixing), and an opaque burst is
/// reproduced byte-for-byte so the eval/test/replay paths are unchanged. A peer's
/// turn is prefixed `{author}: ` so several speakers stay distinguishable inside a
/// merged `user` message — and when the message's first line carries a vocative
/// naming another participant, the prefix carries the addressee too:
/// `Asha (to Anwen): …` / `Anwen (to you): …`. The ONE place message-line
/// formatting lives.
///
/// Why the addressee annotation exists (glass-boxed 2026-07-10): Asha asked
/// "Sure, Anwen. Could you please post your current implementation…" — the
/// addressing lived only in prose, and Atlas (whose turn fired next) answered AS
/// the implementer, presenting code as "my current implementation". His own
/// history then testified he held Anwen's role, and the confusion compounded into
/// third-person self-address. Prose never changes behavior; structure does — so
/// WHO a message is for becomes part of the rendered structure, by pure vocative
/// geometry against known participant names, never content NLP
/// ([[no-hardcoded-heuristics-to-steer-cognition]] — this renders a fact visible,
/// it steers nothing).
pub(super) fn turn_message_line(turn: &BurstTurn) -> String {
    if turn.is_self || turn.author.is_empty() {
        turn.content.clone()
    } else {
        format!("{}: {}", turn.author, turn.content)
    }
}

/// [`turn_message_line`] with addressee annotation for peer turns. `participants`
/// is every display name known in the window (peers + self); `self_name` is THIS
/// persona's name, rendered as "you" so a directed ask reads as directed.
pub(super) fn turn_message_line_addressed(
    turn: &BurstTurn,
    participants: &[String],
    self_name: &str,
) -> String {
    if turn.is_self || turn.author.is_empty() {
        return turn.content.clone();
    }
    match vocative_addressee(&turn.content, participants) {
        // A vocative naming someone OTHER than the speaker → annotate. (A speaker
        // can't address themself; a match on the author is a mention, not a vocative.)
        Some(addr) if !addr.eq_ignore_ascii_case(&turn.author) => {
            let target = if addr.eq_ignore_ascii_case(self_name) {
                "you"
            } else {
                addr
            };
            format!("{} (to {}): {}", turn.author, target, turn.content)
        }
        _ => format!("{}: {}", turn.author, turn.content),
    }
}

/// Case-insensitive match of `name` at byte `pos` of `line` (ASCII fold — persona
/// display names are ASCII by genesis convention).
fn matches_name_at(line: &str, pos: usize, name: &str) -> bool {
    line.get(pos..pos + name.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(name))
}

/// Find WHO a message's first line addresses, by vocative GEOMETRY only — never
/// content interpretation. Two shapes, matched against known participant names:
///
/// - **Leading vocative**: `Anwen, …` / `Atlas: …` / `Asha — …` / `@Anwen …`
/// - **Greeting vocative** in the first line: `Sure, Anwen. Could you…` /
///   `Thanks, Atlas!` — `, Name` closed by punctuation or end-of-line. The
///   punctuation requirement doubles as the word boundary (`, Anwenne.` does not
///   match `Anwen`).
///
/// A bare mention ("I agree with Anwen's plan") matches neither shape and stays
/// unannotated. Leading beats greeting; among greetings the earliest wins.
pub(super) fn vocative_addressee<'a>(content: &str, participants: &'a [String]) -> Option<&'a str> {
    let first_line = content.lines().find(|l| !l.trim().is_empty())?.trim();

    // Leading vocative: name at position 0 (or after '@') followed by address
    // punctuation. `@Name` also accepts whitespace after (mention syntax).
    for name in participants {
        if name.is_empty() {
            continue;
        }
        let (start, at_form) = if first_line.starts_with('@') {
            (1, true)
        } else {
            (0, false)
        };
        if matches_name_at(first_line, start, name) {
            let after = first_line[start + name.len()..].trim_start();
            let boundary = after
                .chars()
                .next()
                .is_none_or(|c| matches!(c, ',' | ':' | '!' | '—' | '-' | '.'));
            if boundary || (at_form && !after.is_empty()) {
                return Some(name);
            }
        }
    }

    // Greeting vocative: earliest `, Name` closed by punctuation or end-of-line.
    let mut best: Option<(usize, &'a str)> = None;
    for name in participants {
        if name.is_empty() {
            continue;
        }
        for (comma_pos, _) in first_line.match_indices(',') {
            let name_pos = comma_pos + 2; // ", " then the name
            if !first_line[comma_pos + 1..].starts_with(' ')
                || !matches_name_at(first_line, name_pos, name)
            {
                continue;
            }
            let after = &first_line[name_pos + name.len()..];
            let closed = after
                .chars()
                .next()
                .is_none_or(|c| matches!(c, '.' | '!' | '?' | ',' | ';'));
            if closed && best.is_none_or(|(p, _)| name_pos < p) {
                best = Some((name_pos, name));
            }
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the tail-trim must keep the LATEST lines (drop the head),
    // start on a clean line boundary (never mid-line), and never split a UTF-8 char.
    // A regression that trimmed the tail instead of the head would drop the turn's
    // most recent activity — the exact thing the persona must respond to.
    #[test]
    fn tail_to_tokens_keeps_latest_lines_on_a_clean_boundary() {
        // Under budget → returned whole.
        assert_eq!(tail_to_tokens("short", 100), "short");

        // Over budget → keep the tail, cut at a line boundary. "old line\nnew line"
        // is 17 chars; a 3-token budget (9 chars) lands the window INSIDE the first
        // line, so the straddled head ("old line") is dropped and the result resumes
        // at the clean line boundary — the latest line survives whole.
        let trimmed = tail_to_tokens("old line\nnew line", 3);
        assert_eq!(
            trimmed, "new line",
            "head dropped, latest line kept clean: {trimmed:?}"
        );
        assert!(
            !trimmed.contains('\n'),
            "cut on the line boundary: {trimmed:?}"
        );

        // Multibyte content must never panic on a mid-char cut (window start lands
        // mid-codepoint and is walked forward to the next char boundary).
        let multibyte = "αβγδ\nεζηθ\nικλμ";
        let _ = tail_to_tokens(multibyte, 3); // must not panic
    }

    // what this catches: identity capture via prose-only addressing (live incident
    // 2026-07-10). Asha asked "Sure, Anwen. Could you please post your current
    // implementation…" — Atlas's turn fired next, nothing structural marked the
    // message as Anwen's, and he answered AS the implementer ("Here's my current
    // implementation…"), corrupting his own history into holding her role. The
    // addressee must be part of the rendered structure.
    #[test]
    fn vocative_addressee_renders_who_a_message_is_for() {
        let names = vec!["Anwen".to_string(), "Asha".to_string(), "Atlas".to_string()];

        // The exact live specimen: greeting vocative in the first line, rendered
        // from ATLAS's seat → the addressee is a third party, named explicitly.
        let asha_to_anwen = BurstTurn::attributed(
            false,
            "Asha",
            "Sure, Anwen. Could you please post your current implementation of the wordstats tool in Rust?",
            None,
        );
        let line = turn_message_line_addressed(&asha_to_anwen, &names, "Atlas");
        assert!(
            line.starts_with("Asha (to Anwen): Sure, Anwen."),
            "greeting vocative annotated: {line:?}"
        );

        // Leading vocative addressed to the READING persona → "(to you)". This is
        // Anwen's live delegation line, rendered from Atlas's seat.
        let anwen_to_atlas = BurstTurn::attributed(
            false,
            "Anwen",
            "Atlas, thank you for offering to help with testing! Could you please create those test files?",
            None,
        );
        let line = turn_message_line_addressed(&anwen_to_atlas, &names, "Atlas");
        assert!(
            line.starts_with("Anwen (to you): Atlas,"),
            "self-addressed vocative renders as 'you': {line:?}"
        );

        // A bare MENTION is not a vocative — no annotation. ("Anwen's" is closed
        // by an apostrophe, not address punctuation.)
        let mention = BurstTurn::attributed(false, "Asha", "I agree with Anwen's plan for the parser.", None);
        assert_eq!(
            turn_message_line_addressed(&mention, &names, "Atlas"),
            "Asha: I agree with Anwen's plan for the parser."
        );

        // Self turns and opaque turns render verbatim — annotation is peer-only.
        let own = BurstTurn::attributed(true, "Atlas", "Anwen, here are the test results.", None);
        assert_eq!(
            turn_message_line_addressed(&own, &names, "Atlas"),
            "Anwen, here are the test results."
        );
        let opaque = BurstTurn::opaque("Anwen, do the thing.");
        assert_eq!(
            turn_message_line_addressed(&opaque, &names, "Atlas"),
            "Anwen, do the thing."
        );

        // A vocative matching the AUTHOR is a signature/self-reference, never an
        // addressee ("Thanks, Asha!" quoted inside Asha's own message).
        let self_named = BurstTurn::attributed(false, "Asha", "Asha, reporting in: review done.", None);
        assert_eq!(
            turn_message_line_addressed(&self_named, &names, "Atlas"),
            "Asha: Asha, reporting in: review done."
        );

        // @-mention form.
        let at_form = BurstTurn::attributed(false, "Asha", "@Atlas can you run the suite?", None);
        let line = turn_message_line_addressed(&at_form, &names, "Atlas");
        assert!(line.starts_with("Asha (to you): @Atlas"), "@-form: {line:?}");

        // Name-prefix false positive guard: ", Anwenne." must not match "Anwen"
        // (the closing-punctuation requirement doubles as the word boundary).
        let names2 = vec!["Anwen".to_string()];
        assert_eq!(
            vocative_addressee("Sure, Anwenne. Please post it.", &names2),
            None
        );
    }
}
