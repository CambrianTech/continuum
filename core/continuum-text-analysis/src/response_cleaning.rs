//! Response Cleaning — Strip unwanted prefixes and thinking tags from AI-generated responses
//!
//! Ported from ResponseCleaner.ts (95 lines → ~70 lines Rust).
//! LLMs sometimes copy formatting from conversation history, adding
//! unwanted prefixes like "[HH:MM] Name: " to their responses.
//! Models like DeepSeek/Qwen also emit `<thinking>...</thinking>` chain-of-thought
//! blocks that should be stripped before displaying to users.
//!
//! Processing order:
//! 0. Strip `<thinking>` / `<think>` blocks (can appear anywhere)
//! 1. `[HH:MM] Name: ` — timestamp + name
//! 2. `Name: ` — name only (starts with capital)
//! 3. `[HH:MM] ` — timestamp only
//! 4. `**Name:** ` or `*Name:* ` — markdown role markers

use crate::scan::{fold_prefix, is_digit, is_space, take_one, take_while};
use std::ops::Range;

/// Result of response cleaning with optional extracted thinking content.
pub struct CleanResult {
    /// Cleaned response text with thinking blocks and prefixes stripped
    pub text: String,
    /// Extracted thinking/reasoning content, if any was found
    pub thinking: Option<String>,
}

// The matchers below are hand-written over `crate::scan` (they were regexes compiled with
// `.expect()`); each names the pattern it is, and the tests pin it against that regex.

/// One `<think>…</think>` / `<thinking>…</thinking>` block: the whole block and its inside.
struct ThinkingBlock {
    block: Range<usize>,
    inside: Range<usize>,
}

/// `(?i)<think(?:ing)?>` or its closing form at the start of `s`: the tag's byte length.
fn thinking_tag(s: &str, open: &str, open_long: &str) -> Option<usize> {
    fold_prefix(s, open_long).or_else(|| fold_prefix(s, open))
}

/// Every `(?si)<think(?:ing)?>(.*?)</think(?:ing)?>` block, leftmost first and
/// non-overlapping, as `Regex::captures_iter` finds them: each opening tag pairs with the
/// nearest closing tag after it.
fn thinking_blocks(s: &str) -> Vec<ThinkingBlock> {
    let tag_starts = |from: usize| s[from..].match_indices('<').map(move |(at, _)| from + at);
    let mut blocks = Vec::new();
    let mut pos = 0;
    while let Some((start, open_len)) =
        tag_starts(pos).find_map(|at| thinking_tag(&s[at..], "<think>", "<thinking>").map(|len| (at, len)))
    {
        let inside_start = start + open_len;
        let Some((close, close_len)) = tag_starts(inside_start)
            .find_map(|at| thinking_tag(&s[at..], "</think>", "</thinking>").map(|len| (at, len)))
        else {
            // No closing tag after the earliest opening one, so none after any later one.
            break;
        };
        blocks.push(ThinkingBlock { block: start..close + close_len, inside: inside_start..close });
        pos = close + close_len;
    }
    blocks
}

/// `\[\d{1,2}:\d{2}\]` at the start of `s`: the rest after the closing bracket.
fn bracketed_time(s: &str) -> Option<&str> {
    let rest = take_one(s, |c| c == '[')?;
    let (hours, rest) = take_while(rest, is_digit);
    let rest = take_one(rest, |c| c == ':').filter(|_| (1..=2).contains(&hours))?;
    let (minutes, rest) = take_while(rest, is_digit);
    take_one(rest, |c| c == ']').filter(|_| minutes == 2)
}

/// Byte length of the prefix of `s` that `rest` is the remainder of.
fn consumed(s: &str, rest: &str) -> usize {
    s.len() - rest.len()
}

/// `^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*` — "[11:59] Name: ": the prefix's byte length.
fn timestamp_name_prefix(s: &str) -> Option<usize> {
    let after_time = bracketed_time(s)?;
    // `\s+[^:]+:` — whitespace first, then at least two chars in all before the first colon.
    take_one(after_time, is_space)?;
    let colon = after_time.find(':')?;
    if after_time[..colon].chars().count() < 2 {
        return None;
    }
    let (_, rest) = take_while(&after_time[colon + 1..], is_space);
    Some(consumed(s, rest))
}

/// `^[A-Z][A-Za-z\s]+:\s*` — "Name: ": the prefix's byte length.
fn name_only_prefix(s: &str) -> Option<usize> {
    let rest = take_one(s, |c| c.is_ascii_uppercase())?;
    let (len, rest) = take_while(rest, |c| c.is_ascii_alphabetic() || is_space(c));
    let rest = take_one(rest, |c| c == ':').filter(|_| len >= 1)?;
    let (_, rest) = take_while(rest, is_space);
    Some(consumed(s, rest))
}

/// `^\[\d{1,2}:\d{2}\]\s*` — "[11:59] ": the prefix's byte length.
fn timestamp_only_prefix(s: &str) -> Option<usize> {
    let (_, rest) = take_while(bracketed_time(s)?, is_space);
    Some(consumed(s, rest))
}

/// `^\*{1,2}[A-Za-z\s]+:\*{1,2}\s*` — "**Name:** ": the prefix's byte length.
fn markdown_role_prefix(s: &str) -> Option<usize> {
    let (stars, rest) = take_while(s, |c| c == '*');
    if !(1..=2).contains(&stars) {
        return None;
    }
    let (len, rest) = take_while(rest, |c| c.is_ascii_alphabetic() || is_space(c));
    let rest = take_one(rest, |c| c == ':').filter(|_| len >= 1)?;
    let after_one = take_one(rest, |c| c == '*')?;
    // `\*{1,2}` is greedy: a second star is taken when present, never a third.
    let rest = match take_one(after_one, |c| c == '*') {
        Some(after_two) => after_two,
        None => after_one,
    };
    let (_, rest) = take_while(rest, is_space);
    Some(consumed(s, rest))
}

/// Our OWN internal perception-frame section labels — the reserved block headers the
/// substrate feeds INTO the prompt (deliberation scaffold + perception blocks). A persona
/// must NEVER emit these: when they appear in output the model is regurgitating its own
/// context scaffolding as speech. Glass-boxed live twice:
///   2026-07-14: a persona posted `[working-memory] … [analysis] … [what I propose] …` as a turn.
///   2026-07-21: Devstral answered coding tasks with `[TOOL_CALLS][workspace] (no files…) <code>`
///     and whole `[room-roster]` / `[workspace-map]` block echoes — burying real code under
///     chaos, or (worse) spending the whole token budget echoing scaffolding and emitting no
///     code at all. That's #158 reserved-vocabulary mimicry: the model mirrors the framing we
///     feed it back as its answer. This is not a turn; it's leaked internals.
/// The perception headers here MUST match the forms actually emitted into the prompt
/// (`[room-roster]`, `[workspace-map]`, `[workspace]`, `[recall]`, `[context]`, `[actions]`,
/// `[room-purpose]`, `[no-acts]`, `[no-deliverable]`) — a header we render but omit here sails through uncaught,
/// which is exactly the gap this closes.
const SCAFFOLD_LABELS: [&str; 18] = [
    "[working-memory]",
    "[working memory]",
    "[your recent messages]",
    "[recent messages]",
    "[analysis]",
    "[what i propose]",
    "[what changed]",
    "[perception]",
    "[your recent acts]",
    "[room-roster]",
    "[workspace-map]",
    "[workspace]",
    "[recall]",
    "[context]",
    "[actions]",
    "[room-purpose]",
    "[no-acts]",
    "[no-deliverable]",
];

/// Strip a leaked leading native tool-call control token. `[TOOL_CALLS]` is the
/// Mistral/Devstral marker that PRECEDES a tool-call payload; when no tool call parses after
/// it (the model just leaked the reserved marker), a leading occurrence is noise — remove it
/// and recover the real content beneath. Idempotent: absent marker → unchanged.
fn strip_leading_tool_call_marker(s: &str) -> &str {
    s.trim_start()
        .strip_prefix("[TOOL_CALLS]")
        .map(str::trim_start)
        .unwrap_or(s)
}

/// Strip contiguous LEADING lines that ARE a reserved scaffold header (optionally carrying an
/// inline body on the same line, e.g. `[workspace] (no files open)`). Recovers the real answer
/// a model buried under a regurgitated scaffold prefix — the `has_close_elements` code in leak
/// #2 (2026-07-21) sat directly under a `[workspace] (no files…)` line. Conservative: only
/// leading, only whole header-lines; stops at the first line that is real content, so a code
/// fence or prose sentence is never touched. Pure-echo (2+ headers) is handled upstream by the
/// suppression gate; this recovers the single-header-prefix case.
fn strip_leading_scaffold_lines(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        let line_end = s.find('\n').unwrap_or(s.len());
        let line_lower = s[..line_end].to_lowercase();
        if SCAFFOLD_LABELS
            .iter()
            .any(|h| line_lower.trim_start().starts_with(h))
        {
            s = if line_end < s.len() {
                &s[line_end + 1..]
            } else {
                ""
            };
        } else {
            return s;
        }
    }
}

/// Is this response a leaked deliberation scaffold rather than a turn? Trigger only on
/// TWO+ distinct internal labels — one alone ("my [analysis] shows…") can be legitimate
/// prose, but two+ of our own frame labels together is unambiguous. Conservative by
/// design: a false negative just posts a slightly odd turn; a false positive would eat a
/// real message, so we demand the stronger signal.
pub(crate) fn is_leaked_deliberation_scaffold(text: &str) -> bool {
    let lower = text.to_lowercase();
    SCAFFOLD_LABELS
        .iter()
        .filter(|l| lower.contains(*l))
        .count()
        >= 2
}

/// Clean an AI response by stripping thinking blocks and unwanted prefixes.
///
/// Returns a `CleanResult` with the cleaned text and any extracted thinking content.
///
/// Processing order:
/// 0. Strip `<thinking>` / `<think>` blocks (extracted for downstream use)
/// 1. `[HH:MM] Name: ` → strip timestamp + name
/// 2. `Name: ` → strip name-only prefix (starts with capital letter)
/// 3. `[HH:MM] ` → strip timestamp-only prefix
/// 4. `**Name:** ` or `*Name:* ` → strip markdown role markers
pub fn clean_response(response: &str) -> CleanResult {
    // Phase 0: Extract and strip thinking blocks
    let blocks = thinking_blocks(response);
    let mut thinking_parts: Vec<String> = Vec::new();
    for ThinkingBlock { inside, .. } in &blocks {
        let trimmed = response[inside.clone()].trim();
        if !trimmed.is_empty() {
            thinking_parts.push(trimmed.to_string());
        }
    }
    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join("\n\n"))
    };

    // The response with every block removed.
    let mut after_thinking = String::with_capacity(response.len());
    let mut kept_from = 0;
    for ThinkingBlock { block, .. } in &blocks {
        after_thinking.push_str(&response[kept_from..block.start]);
        kept_from = block.end;
    }
    after_thinking.push_str(&response[kept_from..]);
    let mut cleaned = after_thinking.trim();

    // Phase 0.4: strip a leaked native `[TOOL_CALLS]` control-token prefix. Devstral/Mistral
    // emit it before a tool payload; a bare leading occurrence (no parseable call after) is
    // the reserved marker leaking as speech — remove it before the scaffold checks so a
    // `[TOOL_CALLS][workspace]…` prefix is seen for the scaffold echo it is. (#158, 2026-07-21)
    cleaned = strip_leading_tool_call_marker(cleaned);

    // Phase 0.5: a LEAKED deliberation scaffold is not a turn — suppress it. Preserve the
    // whole block as thinking (hippocampus still learns from the reasoning) and return
    // empty text so the caller posts nothing; she re-turns cleanly next tick. Better a
    // silent beat than her internal perception frame in the room. (#158)
    if is_leaked_deliberation_scaffold(cleaned) {
        let mut leaked = thinking_parts;
        leaked.push(cleaned.to_string());
        return CleanResult {
            text: String::new(),
            thinking: Some(leaked.join("\n\n")),
        };
    }

    // Phase 0.6: a SINGLE leaked scaffold block as a PREFIX (below the 2+ suppression bar) —
    // strip the header line(s) and recover the real answer beneath. This is what saves a
    // correct solution the model wrote under a `[workspace] (no files…)` echo, instead of
    // grading the chaos. (#158, 2026-07-21)
    cleaned = strip_leading_scaffold_lines(cleaned);

    // Phase 1-4: Apply prefix patterns in priority order
    for prefix in [timestamp_name_prefix, name_only_prefix, timestamp_only_prefix, markdown_role_prefix] {
        if let Some(len) = prefix(cleaned) {
            cleaned = &cleaned[len..];
        }
    }

    CleanResult {
        text: cleaned.trim().to_string(),
        thinking,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check if a response has a prefix that would be cleaned.
    fn has_prefix(response: &str) -> bool {
        let trimmed = response.trim();
        timestamp_name_prefix(trimmed).is_some()
            || name_only_prefix(trimmed).is_some()
            || timestamp_only_prefix(trimmed).is_some()
            || markdown_role_prefix(trimmed).is_some()
    }

    #[test]
    fn test_strip_timestamp_and_name() {
        assert_eq!(
            clean_response("[11:59] GPT Assistant: Yes, Operator...").text,
            "Yes, Operator..."
        );
    }

    #[test]
    fn test_strip_name_only() {
        assert_eq!(
            clean_response("GPT Assistant: Yes, Operator...").text,
            "Yes, Operator..."
        );
    }

    #[test]
    fn test_strip_timestamp_only() {
        assert_eq!(clean_response("[11:59] message here").text, "message here");
    }

    #[test]
    fn test_strip_markdown_double_star() {
        assert_eq!(
            clean_response("**Assistant:** answer here").text,
            "answer here"
        );
    }

    #[test]
    fn test_strip_markdown_single_star() {
        assert_eq!(clean_response("*Helper:* the answer").text, "the answer");
    }

    #[test]
    fn test_no_prefix() {
        let result = clean_response("Just a normal response");
        assert_eq!(result.text, "Just a normal response");
        assert!(result.thinking.is_none());
    }

    #[test]
    fn test_preserves_content() {
        let input = "This response has no prefix but mentions [time] and Name: in the middle.";
        assert_eq!(clean_response(input).text, input);
    }

    #[test]
    fn test_nested_prefixes() {
        assert_eq!(
            clean_response("[11:59] GPT: Assistant: hello").text,
            "hello"
        );
    }

    #[test]
    fn test_has_prefix_true() {
        assert!(has_prefix("[11:59] GPT: hello"));
        assert!(has_prefix("Assistant: hello"));
        assert!(has_prefix("[11:59] hello"));
        assert!(has_prefix("**Helper:** hello"));
    }

    #[test]
    fn test_has_prefix_false() {
        assert!(!has_prefix("Just a normal message"));
        assert!(!has_prefix("lowercase: not a name"));
        assert!(!has_prefix("123: not a name"));
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(clean_response("").text, "");
        assert!(!has_prefix(""));
    }

    #[test]
    fn test_whitespace_trimming() {
        assert_eq!(clean_response("  [11:59] GPT: hello  ").text, "hello");
    }

    // ─── Thinking tag tests ────────────────────────────────────

    #[test]
    fn test_strip_thinking_tags() {
        let input = "<thinking>Let me analyze this...</thinking>Here is my response.";
        let result = clean_response(input);
        assert_eq!(result.text, "Here is my response.");
        assert_eq!(result.thinking.as_deref(), Some("Let me analyze this..."));
    }

    #[test]
    fn test_strip_think_tags_qwen_variant() {
        let input = "<think>I need to consider the options carefully.</think>\nThe answer is 42.";
        let result = clean_response(input);
        assert_eq!(result.text, "The answer is 42.");
        assert_eq!(
            result.thinking.as_deref(),
            Some("I need to consider the options carefully.")
        );
    }

    #[test]
    fn test_thinking_tags_case_insensitive() {
        let input = "<THINKING>Deep thought here</THINKING>Result text.";
        let result = clean_response(input);
        assert_eq!(result.text, "Result text.");
        assert_eq!(result.thinking.as_deref(), Some("Deep thought here"));
    }

    #[test]
    fn test_thinking_multiline() {
        let input = "<thinking>\nStep 1: analyze\nStep 2: decide\nStep 3: respond\n</thinking>\nHere's my answer.";
        let result = clean_response(input);
        assert_eq!(result.text, "Here's my answer.");
        assert!(result.thinking.is_some());
        assert!(result.thinking.as_deref().unwrap().contains("Step 1"));
        assert!(result.thinking.as_deref().unwrap().contains("Step 3"));
    }

    #[test]
    fn test_multiple_thinking_blocks() {
        let input =
            "<thinking>First thought</thinking>Part one. <think>Second thought</think>Part two.";
        let result = clean_response(input);
        assert_eq!(result.text, "Part one. Part two.");
        let thinking = result.thinking.unwrap();
        assert!(thinking.contains("First thought"));
        assert!(thinking.contains("Second thought"));
    }

    #[test]
    fn test_no_thinking_tags() {
        let input = "Just a normal response with no thinking tags.";
        let result = clean_response(input);
        assert_eq!(result.text, input);
        assert!(result.thinking.is_none());
    }

    #[test]
    fn test_thinking_plus_prefix_stripping() {
        // Thinking tags stripped first, then prefix stripping runs on remainder
        let input = "<thinking>reasoning here</thinking>**Assistant:** The answer is yes.";
        let result = clean_response(input);
        assert_eq!(result.text, "The answer is yes.");
        assert_eq!(result.thinking.as_deref(), Some("reasoning here"));
    }

    #[test]
    fn test_empty_thinking_tags() {
        let input = "<thinking></thinking>Actual response.";
        let result = clean_response(input);
        assert_eq!(result.text, "Actual response.");
        assert!(result.thinking.is_none()); // Empty thinking is not stored
    }

    #[test]
    fn test_thinking_with_only_whitespace() {
        let input = "<thinking>   \n  \n   </thinking>Response here.";
        let result = clean_response(input);
        assert_eq!(result.text, "Response here.");
        assert!(result.thinking.is_none()); // Whitespace-only thinking is not stored
    }

    /// what this catches: a persona's INTERNAL deliberation scaffold leaking into the
    /// room as a turn (glass-boxed live 2026-07-14 — Anwen posted her whole
    /// [working-memory]/[analysis]/[what I propose]/[example] frame). Two+ of our own
    /// frame labels ⇒ suppress: empty posted text, whole block preserved as thinking so
    /// hippocampus still learns. Regression for #158.
    #[test]
    fn leaked_deliberation_scaffold_is_suppressed_not_posted() {
        let leaked = "[working-memory]\nYour recent acts and the room's state: ...\n\
            [your recent messages]\n1. \"...\"\n2. \"...\"\n[analysis]\nYou've repeated \
            yourself.\n[what I propose]\nAcknowledge and pivot.\n[example]\n\"Let's look at \
            models.rs next.\"";
        let out = clean_response(leaked);
        assert_eq!(out.text, "", "leaked scaffold must NOT post");
        assert!(
            out.thinking
                .as_deref()
                .unwrap_or("")
                .contains("what I propose"),
            "the leaked frame is preserved as thinking for memory"
        );
    }

    /// what this catches: over-stripping a LEGITIMATE turn that merely mentions one label
    /// in prose. One label is not a leak; only 2+ of our frame labels together is.
    #[test]
    fn single_label_in_prose_is_kept() {
        let legit = "My [analysis] of the code shows a race condition in the lock path.";
        let out = clean_response(legit);
        assert_eq!(
            out.text, legit,
            "a single incidental label must not trigger suppression"
        );
    }

    #[test]
    fn normal_turn_untouched_by_scaffold_gate() {
        let msg = "Sure — I'll open models.rs and summarize how providers are wired.";
        assert_eq!(clean_response(msg).text, msg);
    }

    // ─── Scaffold-echo leak recovery (#158, glass-boxed live 2026-07-21) ────────

    /// what this catches: leak #2 — Devstral wrote CORRECT code but prefixed it with the
    /// native marker + a regurgitated `[workspace]` block. The single-header prefix must be
    /// stripped and the real code recovered, so the gym grades the solution, not the chaos.
    #[test]
    fn tool_calls_plus_workspace_prefix_recovers_code() {
        let leaked = "[TOOL_CALLS][workspace] (no files or repositories are open right now)\n\
            You're asking me to implement a Rust function. Here's my implementation:\n\n\
            ```rust\nfn has_close_elements(n: Vec<f64>) -> bool { false }\n```";
        let out = clean_response(leaked).text;
        assert!(
            out.starts_with("You're asking me"),
            "scaffold prefix must be stripped, got: {out:?}"
        );
        assert!(
            out.contains("```rust"),
            "the real code must survive: {out:?}"
        );
        assert!(!out.contains("[TOOL_CALLS]"), "marker must be gone");
        assert!(
            !out.contains("[workspace]"),
            "leaked block header must be gone"
        );
    }

    /// what this catches: leak #1 — a PURE scaffolding echo (native marker + `[room-roster]`
    /// body + `[workspace-map]` body, no real content). Two+ reserved headers ⇒ suppress the
    /// whole turn; nothing posts, the frame is preserved as thinking. Before this fix neither
    /// perception header was in SCAFFOLD_LABELS so the echo sailed through as the answer.
    #[test]
    fn pure_scaffold_echo_with_perception_headers_is_suppressed() {
        let leaked = "[TOOL_CALLS][room-roster]\nAsha (yourself)\nNando\n\n\
            [workspace-map]\nThis workspace is a real checkout rooted at: /repo\n\
            Top-level directories: src, docs";
        let out = clean_response(leaked);
        assert_eq!(out.text, "", "pure scaffold echo must not post");
        assert!(
            out.thinking
                .as_deref()
                .unwrap_or("")
                .contains("workspace-map"),
            "the leaked frame is preserved as thinking"
        );
    }

    /// what this catches: the marker with NO scaffold body — bare `[TOOL_CALLS]` leading a
    /// normal answer. Strip the marker, keep the prose.
    #[test]
    fn bare_tool_calls_marker_stripped_prose_kept() {
        let out = clean_response("[TOOL_CALLS]The answer is 42.").text;
        assert_eq!(out, "The answer is 42.");
    }

    /// what this catches: a legitimate code answer that merely OPENS with a fence must never
    /// be touched by the scaffold strippers (no marker, no header lines).
    #[test]
    fn clean_code_answer_untouched() {
        let msg = "```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```";
        assert_eq!(clean_response(msg).text, msg);
    }

    // what this catches: a prefix matcher drifting from its anchored regex (digit runs
    // past their bounds, Unicode digits, a third star, whitespace before the first colon,
    // a colon with nothing between it and the bracket).
    #[test]
    fn prefixes_match_the_replaced_regexes() {
        type PrefixMatcher = fn(&str) -> Option<usize>;
        let cases: [(&str, PrefixMatcher); 4] = [
            (r"^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*", timestamp_name_prefix),
            (r"^[A-Z][A-Za-z\s]+:\s*", name_only_prefix),
            (r"^\[\d{1,2}:\d{2}\]\s*", timestamp_only_prefix),
            (r"^\*{1,2}[A-Za-z\s]+:\*{1,2}\s*", markdown_role_prefix),
        ];
        let tokens = ["[", "]", "1", "12", "123", "٣", ":", " ", "\n", "*", "**", "Name", "a", "Z", "x y", "é", "[11:59]"];
        for (pattern, ours) in cases {
            let re = regex::Regex::new(pattern).unwrap();
            for text in crate::scan::tests::corpus(&tokens, 30_000, 7) {
                assert_eq!(ours(&text), re.find(&text).map(|m| m.end()), "{pattern} on {text:?}");
            }
        }
    }

    // what this catches: the thinking-block scan drifting from the regex's captures
    // (case-folded tags incl. a KELVIN SIGN, an unclosed opener, nested and adjacent blocks).
    #[test]
    fn thinking_blocks_match_the_replaced_regex() {
        let re = regex::Regex::new(r"(?si)<think(?:ing)?>(.*?)</think(?:ing)?>").unwrap();
        let tokens = ["<think>", "</think>", "<thinking>", "</THINKING>", "<thin\u{212A}>", "<", ">", "/", "think", "ing", "x", "\n", "é"];
        for text in crate::scan::tests::corpus(&tokens, 30_000, 7) {
            let want: Vec<_> = re
                .captures_iter(&text)
                .filter_map(|c| Some((c.get(0)?.range(), c.get(1)?.range())))
                .collect();
            let ours: Vec<_> = thinking_blocks(&text).into_iter().map(|b| (b.block, b.inside)).collect();
            assert_eq!(ours, want, "{text:?}");
        }
    }
}
