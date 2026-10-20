//! Reasoning/answer separation (pure, synchronous).

/// Separate a reasoning model's chain-of-thought from its user-facing answer at the
/// ADAPTER boundary, so reasoning is captured (for the glass-box harness) but NEVER
/// reaches the room. Precedence:
///
/// 1. A server-provided `reasoning_content` (vLLM-style parsers) — authoritative;
///    `content` is already clean.
/// 2. Inline `<think>…</think>` in `content` (unsloth/llama.cpp today) — the block
///    is reasoning; everything OUTSIDE it is the answer.
/// 3. An UNCLOSED `<think>` — the model ran out of tokens mid-thought (the runaway
///    loop that leaked into the room): the whole tail is reasoning and there is NO
///    answer. Returns empty text so the caller refuses to post, never leaking raw
///    reasoning.
///
/// And across all three: a BARE `</think>` in `content` (no `<think>` before it) closes
/// reasoning that began before `content` did, so everything up to the LAST one is
/// reasoning. Two shapes produce it. The Qwen3.x templates open `<think>` inside the PROMPT,
/// so an unparsed completion starts mid-thought. And a mind that QUOTES the close tag while
/// thinking (Kimi, 2026-09-28, reasoning about this very split) emits the special token, the
/// server's parser ends `reasoning_content` at the quote, and the rest of her thinking, her
/// real `</think>` and her answer all arrive as `content`: two fragments of her reasoning
/// were published to the room as speech. The recovered reasoning is APPENDED to the
/// reasoning, never dropped: thinking is what the learning genome trains on (Joel,
/// 2026-09-28: "think is quintessential to learning genome").
///
/// Returns `(clean_text, reasoning)`. Pure + synchronous → unit-tested in isolation.
pub(crate) fn extract_reasoning(
    content: &str,
    reasoning_content: Option<&str>,
) -> (String, Option<String>) {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    // A bare close: reasoning that began before `content` ends at its LAST STANDALONE
    // `</think>` (alone on its line, as the model's own close is: `\n</think>\n\n`). A close
    // written inline is a quote, in her thinking or in an answer, and closes nothing (Cormac
    // on #4493: an answer that says "a bare `</think>`" must keep its head).
    let (content, carried) = match last_standalone(content, CLOSE) {
        Some(close) if !content[..close].contains(OPEN) => {
            (&content[close + CLOSE.len()..], Some(content[..close].trim()))
        }
        _ => (content, None),
    };
    // Joined where the parser cut it: at the close tag she quoted (or the prompt's open).
    let joined = |before: Option<&str>| -> Option<String> {
        let parts: Vec<&str> = [before, carried].into_iter().flatten().map(str::trim).filter(|p| !p.is_empty()).collect();
        (!parts.is_empty()).then(|| parts.join(&format!("\n{CLOSE}\n")))
    };

    // (1) Server already split it out — trust that; content is the clean answer.
    if let Some(rc) = reasoning_content {
        let rc = rc.trim();
        if !rc.is_empty() {
            return (content.trim().to_string(), joined(Some(rc)));
        }
    }

    let Some(open_idx) = content.find(OPEN) else {
        // (no think) plain content, less any reasoning a bare close carried.
        return (content.trim().to_string(), joined(None));
    };
    let before = content[..open_idx].trim();
    let after_open = &content[open_idx + OPEN.len()..];

    // The last STANDALONE close, else the first close: a close she quoted inline mid-thought
    // is not the end of her thinking, and a compact `<think>x</think>answer` still splits.
    match last_standalone(after_open, CLOSE).or_else(|| after_open.find(CLOSE)) {
        // (2) Well-formed <think>…</think>: answer is whatever sits OUTSIDE the block.
        Some(close_rel) => {
            let reasoning = after_open[..close_rel].trim();
            let after_close = after_open[close_rel + CLOSE.len()..].trim();
            let mut text = String::from(before);
            if !text.is_empty() && !after_close.is_empty() {
                text.push('\n');
            }
            text.push_str(after_close);
            (
                text.trim().to_string(),
                (!reasoning.is_empty()).then(|| reasoning.to_string()),
            )
        }
        // (3) Unclosed <think>: truncated thinking → no answer (text is whatever
        // preceded the block, normally empty). The reasoning is the runaway tail.
        None => {
            let reasoning = after_open.trim();
            (
                before.to_string(),
                (!reasoning.is_empty()).then(|| reasoning.to_string()),
            )
        }
    }
}

/// PURE: the byte index of the last `tag` that stands alone on its line (only whitespace
/// between it and the line's start and end), or `None`.
fn last_standalone(text: &str, tag: &str) -> Option<usize> {
    text.match_indices(tag)
        .map(|(i, _)| i)
        .filter(|&i| {
            let before = text[..i].rsplit('\n').next().unwrap_or(""); // unwrap_or: rsplit always yields a piece
            let after = text[i + tag.len()..].split('\n').next().unwrap_or(""); // unwrap_or: as above
            before.trim().is_empty() && after.trim().is_empty()
        })
        .last()
}
