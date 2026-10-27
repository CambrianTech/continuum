//! Visible-text sanitizing for persona replies: `<think>` extraction and leaked tool/section markup stripping.

use super::*;

/// each as a `cognition:think-block` event for the (future) hippocampus
/// to consume. Returns the cleaned visible text + the count of blocks
/// emitted (for telemetry).
///
/// A.6: this is the strip-AND-emit pair. Stripping kills the persona
/// feedback loop where personas re-analyzed each other's working
/// memory; emitting preserves the trace for memory consolidation.
///
/// Today: the event surface is observable for debugging; nothing
/// listens. Tomorrow: hippocampus subscribes and turns each think
/// block into a long-term memory entity.
pub(super) fn strip_thinks_emit_events(raw: &str, persona_id: Uuid, message_id: Uuid) -> (String, u32) {
    // Match <think>...</think> non-greedy across newlines. Standalone
    // helper kept simple; if think blocks ever start nesting (they
    // don't today), this needs to grow.
    let mut visible = String::with_capacity(raw.len());
    let mut count: u32 = 0;
    let mut cursor = 0usize;
    let bytes = raw.as_bytes();
    while cursor < bytes.len() {
        if let Some(open_off) = find_at(bytes, cursor, b"<think>") {
            // Append everything before the open tag to visible.
            visible.push_str(&raw[cursor..open_off]);
            let after_open = open_off + b"<think>".len();
            if let Some(close_off) = find_at(bytes, after_open, b"</think>") {
                let think_text = &raw[after_open..close_off];
                emit_think_block(persona_id, message_id, think_text);
                count = count.saturating_add(1);
                cursor = close_off + b"</think>".len();
            } else {
                // Unterminated <think> — keep raw as visible to avoid
                // losing data; log + continue. Rare: model truncated
                // mid-think due to max_tokens.
                visible.push_str(&raw[open_off..]);
                break;
            }
        } else {
            // No more think blocks — append the tail.
            visible.push_str(&raw[cursor..]);
            break;
        }
    }
    // Cleanup: collapse leading/trailing whitespace introduced by
    // adjacent strips. Preserve internal formatting otherwise.
    (visible.trim().to_string(), count)
}

static TOOL_USE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<tool_use\b[^>]*>.*?</tool_use>").expect("tool_use regex")
});
static TOOL_RESULT_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<tool_result\b[^>]*>.*?</tool_result>").expect("tool_result regex")
});
static THINKING_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<thinking\b[^>]*>.*?</thinking>").expect("thinking regex")
});
static TOOL_NAME_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<tool_name\b[^>]*>.*?</tool_name>").expect("tool_name regex")
});
static PARAMETERS_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<parameters\b[^>]*>.*?</parameters>").expect("parameters regex")
});
static ARGUMENTS_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<arguments\b[^>]*>.*?</arguments>").expect("arguments regex")
});
static TOOL_CALLS_MARKER_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    // The Devstral/Mistral native tool-call marker `[TOOL_CALLS]` plus an optional
    // immediately-following bracket-tag (the tool the model tried to invoke, e.g.
    // `[room-roster]`). Reserved vocabulary that must NEVER appear in SPOKEN text.
    // When a real tool followed, the parser lifted it upstream; what reaches the
    // visible-text stripper is UNPARSED residue — an unknown/hallucinated tag
    // (#159) — that would otherwise leak as speech (glass-boxed 2026-07-17: Casper
    // spoke `[TOOL_CALLS][room-roster] (no one else is present)` verbatim).
    regex::Regex::new(r"\[TOOL_CALLS\]\s*(?:\[[a-z][a-z0-9/_-]*\])?")
        .expect("tool_calls marker regex")
});
static BARE_TOOL_REF_LINE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"^\s*['"`][a-z][a-z0-9_-]*/[a-z0-9_/-]+['"`]\s*$"#)
        .expect("bare tool ref line regex")
});
static EXCESS_BLANK_LINES_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\n{3,}").expect("blank lines regex"));

// System-prompt-section header line: matches `=== SENTINELS ===`,
// `=== ACTIVITY CONTEXT ===`, `=== TOOL DEFINITIONS ===`, `=== END ===`.
// When a model echoes its own scaffolding back as the visible reply
// (post-#1077 BUG-F observed on canary 08bbc7a34: Teacher AI #489be5
// dumped full system prompt + tool definitions as chat content), the
// existing XML-tag regexes do NOT match because these are shell-rule-
// style section headers, not tags. The strip logic uses this regex
// line-by-line: we walk lines, when we hit a section header we drop the
// header AND every following line until we hit the NEXT section header
// or end-of-string. The regex crate doesn't support arbitrary
// lookahead, so we do the boundary detection in Rust instead of in the
// pattern.
static SECTION_HEADER_LINE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^=== [A-Z][A-Z0-9 _-]* ===\s*$").expect("section header line regex")
});

/// Strip system-prompt section blocks. A block opens at a
/// `=== HEADER ===` line and closes at either the next
/// `=== HEADER ===` line OR a blank line. This means real reply prose
/// separated from scaffold by a paragraph break survives, while
/// contiguous prompt-internal content (sentinels, activity, tool
/// definitions, etc.) gets dropped together.
///
/// Guarded by the header regex's strict all-caps + space-padded shape
/// requirement, so markdown separators like `--- ` or lowercase
/// dividers do not trigger. Used by strip_leaked_tool_markup to scrub
/// leaked scaffolding from visible chat replies.
fn strip_section_header_blocks(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        if SECTION_HEADER_LINE_RE.is_match(line) {
            in_block = true;
            continue;
        }
        if line.trim().is_empty() {
            // Blank line closes any open block. We still pass the blank
            // through so paragraph spacing in real prose is preserved.
            in_block = false;
            out.push(line);
            continue;
        }
        if !in_block {
            out.push(line);
        }
    }
    out.join("\n")
}

/// Strip dead tool-invocation markup from text before the host posts it.
///
/// Tool execution belongs in Rust cognition, not in the TS chat shim.
/// Until every generated tool call is consumed by the Rust executor,
/// local models can leak `<tool_use>` / `<parameters>` fragments as
/// visible prose. Posting those fragments poisons room history and
/// drives echo loops. Keep the cleanup Rust-side so every host surface
/// (TS, CLI, future native apps) receives the same post-processed text.
pub(super) fn strip_leaked_tool_markup(text: &str) -> String {
    let mut cleaned = text.to_string();
    for re in [
        &*TOOL_USE_RE,
        &*TOOL_RESULT_RE,
        &*THINKING_RE,
        &*TOOL_NAME_RE,
        &*PARAMETERS_RE,
        &*ARGUMENTS_RE,
        &*TOOL_CALLS_MARKER_RE,
    ] {
        cleaned = re.replace_all(&cleaned, "").into_owned();
    }
    cleaned = strip_section_header_blocks(&cleaned);
    cleaned = cleaned
        .lines()
        .filter(|line| !BARE_TOOL_REF_LINE_RE.is_match(line))
        .collect::<Vec<_>>()
        .join("\n");
    EXCESS_BLANK_LINES_RE
        .replace_all(&cleaned, "\n\n")
        .trim()
        .to_string()
}

fn find_at(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Emit a `cognition:think-block` event so the (future) hippocampus
/// can subscribe and consolidate.
///
/// **STUB** — wired during chat-path integration. Will go through the
/// existing event-broadcast mechanism (TBD: confirm path during
/// integration; either ServiceModule's event channel or the SSE/IPC
/// broadcast surface). Today: writes to the cognition log so the
/// blocks are observable for debugging.
fn emit_think_block(persona_id: Uuid, message_id: Uuid, think_text: &str) {
    // TODO(A.6 integration): replace with real event emission via the
    // existing broadcast channel. Tracing log is the temporary
    // observable surface — hippocampus subscribers will be wired in
    // the dedicated migration PR.
    tracing::debug!(
        target: "cognition::think_block",
        persona_id = %persona_id,
        message_id = %message_id,
        think_text_len = think_text.len(),
        "captured think block (event emission TBD)"
    );
}
