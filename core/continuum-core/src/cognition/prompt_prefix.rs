//! How much of a persona's prompt her previous request already held (card 5b09111e).
//!
//! The engine reuses only a common PREFIX of consecutive prompts from its KV cache, and
//! one block that changes ahead of a stable region forfeits everything behind it. On the
//! M5 (2026-09-27) KV reuse read 0% for over an hour; two consecutive captured requests of
//! one persona shared only the first 12,546 of ~30k characters, and the break was the
//! steps ledger, found by diffing captures by hand. This makes that diff a probe on every
//! generation: how many characters matched, of how many, and WHICH message first
//! differed, named by its first line (a `[steps taken this session]` banner names itself).
//! So a change to prompt order is measured on the wire, not inferred.
//!
//! Pure comparison plus one small per-persona record of the previous request's messages.

use crate::ai::types::TextGenerationRequest;
use std::sync::LazyLock;

/// A request as the prefix comparison reads it: the system prompt, then each message
/// with its role. The role is part of the rendered prompt, so a role change is a change.
fn rendered(req: &TextGenerationRequest) -> Vec<String> {
    let mut out = Vec::with_capacity(req.messages.len() + 2);
    out.push(format!("<system>{}", req.system_prompt.as_deref().unwrap_or("")));
    // The tool surface: Qwen/ChatML templates append it to the END of the system turn,
    // ahead of every message, and it varies per turn (delib.tool_surface.withheld). Left out,
    // a tool-set change broke the engine's prefix at the system turn while this comparison
    // still read the prompt as kept (Fable on #4487).
    out.push(format!(
        "<tools>{}",
        req.tools.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default()).unwrap_or_default() // unwrap_or_default: an unserializable surface compares as empty, the same on both sides
    ));
    out.extend(req.messages.iter().map(|m| format!("<{}>{}", m.role, m.content_text())));
    out
}

/// What one comparison found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrefixMatch {
    /// Characters of the new request that the previous one held, from the start.
    pub common_chars: usize,
    pub total_chars: usize,
    /// The first message that differs (0 = the system prompt), and its first line.
    pub first_divergent: Option<(usize, String)>,
}

/// PURE: how much of `now` the `previous` request held from the start.
pub(crate) fn compare(previous: &[String], now: &[String]) -> PrefixMatch {
    let total_chars = now.iter().map(|m| m.chars().count()).sum();
    let mut common_chars = 0usize;
    for (i, message) in now.iter().enumerate() {
        match previous.get(i) {
            Some(before) if before == message => common_chars += message.chars().count(),
            other => {
                let shared = other.map_or(0, |before| {
                    before.chars().zip(message.chars()).take_while(|(a, b)| a == b).count()
                });
                // Past the `<role>` marker, the message's first non-empty line: a
                // block's banner (`[steps taken this session]`), whatever leads it.
                let body = message.split_once('>').map_or(message.as_str(), |(_, rest)| rest);
                let banner: String = body
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(60)
                    .collect();
                return PrefixMatch {
                    common_chars: common_chars + shared,
                    total_chars,
                    first_divergent: Some((i, banner)),
                };
            }
        }
    }
    PrefixMatch { common_chars, total_chars, first_divergent: None }
}

static LAST: LazyLock<dashmap::DashMap<uuid::Uuid, Vec<String>>> = LazyLock::new(dashmap::DashMap::new);

/// Whether the persona's IN-FLIGHT request extends her previous one whole (every message of
/// the previous request is still there, unchanged, as a prefix), recorded by [`observe`] and
/// read back by [`attribute_reuse`] once the engine answers (card 9e4d61e8).
static EXTENDS: LazyLock<dashmap::DashMap<uuid::Uuid, bool>> = LazyLock::new(dashmap::DashMap::new);

/// The engine's own token count for the persona's previous prompt (`cache_n + prompt_n`):
/// the prefix the engine could reuse when the next prompt extends that one whole.
static PREV_PROMPT_TOKENS: LazyLock<dashmap::DashMap<uuid::Uuid, u32>> = LazyLock::new(dashmap::DashMap::new);

/// Where one request's prompt reuse went (card 9e4d61e8), in the ENGINE'S tokens only: no
/// characters are compared with tokens (Codex on #4487: a share of characters minus a share
/// of tokens invents a gap wherever prose and code tokenize differently).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReuseSplit {
    /// The prompt extended the previous one whole, so the engine could have reused all
    /// `reusable` tokens of it; `slot_lost` of them it did not (the slot was evicted, its
    /// page not restored, or the request landed on another slot).
    Exact { reusable: u32, cached: u32, slot_lost: u32 },
    /// The prompt changed inside the previous one (a block moved, the tool surface changed),
    /// so what was reusable cannot be told in tokens without re-tokenizing; the prompt's own
    /// change is `delib.prompt.common_prefix` (which names the block), and no slot figure is
    /// guessed.
    PromptChanged { cached: u32 },
}

/// PURE: the split, from whether this prompt extends the previous one whole and the engine's
/// token counts for the previous prompt and this one.
pub(crate) fn split_reuse(extends_previous: bool, previous_prompt_tokens: Option<u32>, cached: u32) -> Option<ReuseSplit> {
    match (extends_previous, previous_prompt_tokens) {
        (true, Some(reusable)) => Some(ReuseSplit::Exact { reusable, cached, slot_lost: reusable.saturating_sub(cached) }),
        (false, Some(_)) => Some(ReuseSplit::PromptChanged { cached }),
        (_, None) => None, // the mind's first request: nothing to compare with
    }
}

/// Once the engine answered: probe where this persona's prompt reuse went, and remember this
/// prompt's token count for her next request.
pub(crate) fn attribute_reuse(persona: uuid::Uuid, cached_tokens: u32, prefill_tokens: u32) {
    let extends = EXTENDS.remove(&persona).map(|(_, e)| e);
    let previous = PREV_PROMPT_TOKENS.get(&persona).map(|t| *t);
    PREV_PROMPT_TOKENS.insert(persona, cached_tokens.saturating_add(prefill_tokens));
    let Some(extends) = extends else { return };
    match split_reuse(extends, previous, cached_tokens) {
        Some(ReuseSplit::Exact { reusable, cached, slot_lost }) => crate::probe!(
            class = "delib.prompt.reuse_split",
            persona = %persona,
            kind = "exact",
            reusable = u64::from(reusable),
            cached = u64::from(cached),
            slot_lost = u64::from(slot_lost),
            prefill = u64::from(prefill_tokens),
            "the prompt extended the previous one whole: every reusable token the engine did not serve is the slot's loss"
        ),
        Some(ReuseSplit::PromptChanged { cached }) => crate::probe!(
            class = "delib.prompt.reuse_split",
            persona = %persona,
            kind = "prompt_changed",
            cached = u64::from(cached),
            prefill = u64::from(prefill_tokens),
            "the prompt changed inside the previous one: its own change is delib.prompt.common_prefix; no slot figure is guessed"
        ),
        None => {}
    }
}

/// Compare this request with the persona's previous one and probe the result; the first
/// request a persona sends has nothing to compare against and says nothing.
pub(crate) fn observe(persona: uuid::Uuid, req: &TextGenerationRequest) {
    let now = rendered(req);
    let previous_len = LAST.get(&persona).map_or(0, |previous| previous.len());
    let found = LAST.get(&persona).map(|previous| compare(&previous, &now));
    LAST.insert(persona, now);
    let Some(found) = found else {
        EXTENDS.remove(&persona);
        return;
    };
    // Whole-extension: the first divergence is past every message the previous request had.
    EXTENDS.insert(persona, found.first_divergent.as_ref().map_or(true, |(i, _)| *i >= previous_len));
    let (index, banner) = found.first_divergent.clone().unwrap_or((usize::MAX, String::new()));
    crate::probe!(
        class = "delib.prompt.common_prefix",
        persona = %persona,
        common_chars = found.common_chars as u64,
        total_chars = found.total_chars as u64,
        share = if found.total_chars == 0 { 0.0 } else { found.common_chars as f64 / found.total_chars as f64 },
        first_divergent_index = if index == usize::MAX { -1 } else { index as i64 }, // probe field: -1 = identical
        first_divergent = %banner,
        "how much of this prompt the persona's previous request already held, and the first message that changed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a prefix break the wire cannot name (card 5b09111e: the M5 at 0%
    // KV reuse was diagnosed by diffing captures by hand). The comparison must count every
    // byte that matched up to the first changed message plus that message's shared start,
    // and name the changed message by its banner, so a sliding block identifies itself.
    #[test]
    fn the_first_changed_block_is_named_and_the_shared_start_is_counted() {
        let before = vec![
            "<system>you are Kimi".to_string(),
            "<user>[room-wall]\nrecipe".to_string(),
            "<user>[steps taken this session]\n[action #1827] code/shell\n[action #1828] work/get".to_string(),
            "<user>[work turn] fix the bug".to_string(),
        ];
        let mut now = before.clone();
        now[2] = "<user>[steps taken this session]\n[action #1828] work/get\n[action #1829] code/read".to_string();
        let found = compare(&before, &now);
        let (index, banner) = found.first_divergent.clone().expect("a block changed");
        assert_eq!((index, banner.as_str()), (2, "[steps taken this session]"));
        let head = before[0].chars().count() + before[1].chars().count();
        let shared = "<user>[steps taken this session]\n[action #182".chars().count();
        assert_eq!(found.common_chars, head + shared);
        assert!(found.common_chars < found.total_chars);
        let same = compare(&before, &before);
        assert_eq!((same.first_divergent, same.common_chars), (None, same.total_chars));
    }

    // what this catches (card 9e4d61e8): reading low KV reuse as one number, and inventing a
    // slot loss by comparing characters with tokens (Codex on #4487). A prompt that extends the
    // previous one whole could reuse every token the engine counted for it, so what the engine
    // did not serve is exactly the slot's loss, in its own tokens; a prompt that changed inside
    // gets no slot figure at all; a mind's first request says nothing.
    #[test]
    fn a_requests_lost_reuse_is_the_slots_only_when_the_prompt_extended_the_last_one() {
        assert_eq!(split_reuse(true, Some(10_000), 4_000), Some(ReuseSplit::Exact { reusable: 10_000, cached: 4_000, slot_lost: 6_000 }));
        assert_eq!(split_reuse(true, Some(10_000), 12_000), Some(ReuseSplit::Exact { reusable: 10_000, cached: 12_000, slot_lost: 0 }), "serving more than the last prompt is no loss");
        assert_eq!(split_reuse(false, Some(10_000), 4_000), Some(ReuseSplit::PromptChanged { cached: 4_000 }));
        assert_eq!(split_reuse(true, None, 4_000), None, "the first request has nothing to compare");
    }

    // what this catches (Fable on #4487): a tool-surface change read as a kept prompt. The
    // template renders the tools at the end of the system turn, so a changed surface is a
    // change at index 1, ahead of every message.
    #[test]
    fn a_changed_tool_surface_is_a_prompt_change_at_the_system_turn() {
        use crate::ai::types::{ChatMessage, NativeToolSpec};
        let mut a = TextGenerationRequest::default();
        a.system_prompt = Some("you are Kimi".into());
        a.messages = vec![ChatMessage::text("user", "fix the bug")];
        let mut b = a.clone();
        let tool = |name: &str| NativeToolSpec {
            name: name.into(),
            description: String::new(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".into(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        };
        a.tools = Some(vec![tool("code/read")]);
        b.tools = Some(vec![tool("code/write")]);
        let found = compare(&rendered(&a), &rendered(&b));
        assert_eq!(found.first_divergent.map(|(i, _)| i), Some(1), "the tool surface sits right after the system prompt");
    }
}
