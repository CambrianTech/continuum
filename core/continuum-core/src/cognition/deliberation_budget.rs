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
/// merged `user` message. The ONE place message-line formatting lives.
pub(super) fn turn_message_line(turn: &BurstTurn) -> String {
    if turn.is_self || turn.author.is_empty() {
        turn.content.clone()
    } else {
        format!("{}: {}", turn.author, turn.content)
    }
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
}
