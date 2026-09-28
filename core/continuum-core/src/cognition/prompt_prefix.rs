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

/// The whole request as the reuse split fingerprints it: the model it is routed to, the
/// system prompt, the tool surface, and every message serialized in full (tool-call metadata
/// included), so a routing, surface or metadata change reads as a change. The engine's
/// template and any truncation still sit between this and the tokens it prefills, which is
/// why the split is a CANDIDATE gap, never a causal one (Codex on #4487).
fn fingerprint(req: &TextGenerationRequest) -> Vec<String> {
    let mut out = Vec::with_capacity(req.messages.len() + 3);
    out.push(format!("<model>{}", req.model.as_deref().unwrap_or(""))); // unwrap_or: no model named = the lane's default, fingerprinted as empty on both sides
    out.push(format!("<system>{}", req.system_prompt.as_deref().unwrap_or(""))); // unwrap_or: no system prompt renders as an empty one
    out.push(format!(
        "<tools>{}",
        req.tools.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default()).unwrap_or_default() // unwrap_or_default: an unserializable surface compares as empty on both sides
    ));
    out.extend(req.messages.iter().map(|m| serde_json::to_string(m).unwrap_or_default())); // unwrap_or_default: as above
    out
}

/// The request in flight per persona, staged before it is sent: its id and fingerprint.
static PENDING: LazyLock<dashmap::DashMap<uuid::Uuid, (String, Vec<String>)>> = LazyLock::new(dashmap::DashMap::new);

/// The persona's last request that the engine ANSWERED WITH TIMINGS: its fingerprint and the
/// engine's own token count for it (`cache_n + prompt_n`). Committed only on such an answer,
/// in one record, so a failed or untimed attempt can never pair one request's content with
/// another's token count (Codex on #4487).
static COMMITTED: LazyLock<dashmap::DashMap<uuid::Uuid, (Vec<String>, u32)>> = LazyLock::new(dashmap::DashMap::new);

/// Where one request's prompt reuse went (card 9e4d61e8), in the engine's tokens only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReuseSplit {
    /// The request extends the last answered one whole, so up to `reusable` tokens (the
    /// engine's count for that request) were reusable; `gap` of them it did not serve. A
    /// CANDIDATE slot loss: the template or truncation can still differ from the fingerprint.
    Candidate { reusable: u32, cached: u32, gap: u32 },
    /// The request changed inside the last answered one; no gap is guessed
    /// (`delib.prompt.common_prefix` names the block that moved).
    PromptChanged { cached: u32 },
}

/// PURE: whether `now` holds every element of `previous`, unchanged, as its prefix.
pub(crate) fn extends_whole(previous: &[String], now: &[String]) -> bool {
    now.len() >= previous.len() && previous.iter().zip(now).all(|(a, b)| a == b)
}

/// PURE: the split, from whether the request extends the last answered one and that one's
/// engine token count.
pub(crate) fn split_reuse(extends_previous: bool, previous_prompt_tokens: Option<u32>, cached: u32) -> Option<ReuseSplit> {
    match (extends_previous, previous_prompt_tokens) {
        (true, Some(reusable)) => Some(ReuseSplit::Candidate { reusable, cached, gap: reusable.saturating_sub(cached) }),
        (false, Some(_)) => Some(ReuseSplit::PromptChanged { cached }),
        (_, None) => None,
    }
}

/// Stage the request about to be sent. An earlier request of this persona that never settled
/// (it failed, or returned without timings) leaves the engine's slot state unknown, so the
/// committed record is dropped and this request's split says nothing.
pub(crate) fn stage_reuse(persona: uuid::Uuid, request_id: &str, req: &TextGenerationRequest) {
    if PENDING.insert(persona, (request_id.to_string(), fingerprint(req))).is_some() {
        COMMITTED.remove(&persona);
    }
}

/// Settle the request once it completed: `timing` is the engine's `(cache_n, prompt_n)`, or
/// `None` for a failure or a provider without timings. Probes the split, then commits this
/// request as the one the next is compared with, only when the engine answered with timings.
pub(crate) fn settle_reuse(persona: uuid::Uuid, request_id: &str, timing: Option<(u32, u32)>) {
    // Only THIS request's staging is taken: a late completion of an older request must not
    // erase a newer one staged since (Codex on #4487).
    let Some((_, (_, now))) = PENDING.remove_if(&persona, |_, (staged_id, _)| staged_id == request_id) else {
        return;
    };
    let Some((cached, prefill)) = timing else {
        COMMITTED.remove(&persona);
        return;
    };
    let previous = COMMITTED.get(&persona).map(|c| (extends_whole(&c.0, &now), c.1));
    match previous.and_then(|(extends, tokens)| split_reuse(extends, Some(tokens), cached)) {
        Some(ReuseSplit::Candidate { reusable, cached, gap }) => crate::probe!(
            class = "delib.prompt.reuse_split",
            persona = %persona,
            request = request_id,
            kind = "candidate",
            reusable = u64::from(reusable),
            cached = u64::from(cached),
            gap = u64::from(gap),
            prefill = u64::from(prefill),
            "the request extended the last answered one whole: the reusable tokens the engine did not serve are a candidate slot loss (template and truncation not verified)"
        ),
        Some(ReuseSplit::PromptChanged { cached }) => crate::probe!(
            class = "delib.prompt.reuse_split",
            persona = %persona,
            request = request_id,
            kind = "prompt_changed",
            cached = u64::from(cached),
            prefill = u64::from(prefill),
            "the request changed inside the last answered one: its own change is delib.prompt.common_prefix; no gap is guessed"
        ),
        None => {}
    }
    COMMITTED.insert(persona, (now, cached.saturating_add(prefill)));
}

/// Compare this request with the persona's previous one and probe the result; the first
/// request a persona sends has nothing to compare against and says nothing.
pub(crate) fn observe(persona: uuid::Uuid, req: &TextGenerationRequest) {
    let now = rendered(req);
    let found = LAST.get(&persona).map(|previous| compare(&previous, &now));
    LAST.insert(persona, now);
    let Some(found) = found else { return };
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

    // what this catches (card 9e4d61e8, Codex and Fable on #4487): reading low KV reuse as one
    // number, comparing characters with tokens, and pairing one request's content with another's
    // token count. The split is in engine tokens; a request extending the last ANSWERED one whole
    // gets a candidate gap; a changed one gets none; and a failed or untimed attempt drops the
    // committed record, so the next request says nothing instead of pairing wrong.
    #[test]
    fn a_requests_lost_reuse_is_a_candidate_gap_only_against_the_last_answered_request() {
        assert_eq!(split_reuse(true, Some(10_000), 4_000), Some(ReuseSplit::Candidate { reusable: 10_000, cached: 4_000, gap: 6_000 }));
        assert_eq!(split_reuse(true, Some(10_000), 12_000), Some(ReuseSplit::Candidate { reusable: 10_000, cached: 12_000, gap: 0 }));
        assert_eq!(split_reuse(false, Some(10_000), 4_000), Some(ReuseSplit::PromptChanged { cached: 4_000 }));
        assert_eq!(split_reuse(true, None, 4_000), None);
        let (a, b, c) = (vec!["m".to_string(), "s".into()], vec!["m".to_string(), "s".into(), "x".into()], vec!["m".to_string(), "t".into()]);
        assert!(extends_whole(&a, &b) && extends_whole(&a, &a) && !extends_whole(&a, &c) && !extends_whole(&b, &a));

        // a timed A, an untimed B, then C: C must not be compared with A's token count
        let persona = uuid::Uuid::new_v4();
        let req = |text: &str| {
            let mut r = TextGenerationRequest::default();
            r.messages = vec![crate::ai::types::ChatMessage::text("user", text)];
            r
        };
        stage_reuse(persona, "a", &req("one"));
        settle_reuse(persona, "a", Some((0, 100)));
        assert!(COMMITTED.contains_key(&persona), "an answered request is committed");
        stage_reuse(persona, "b", &req("two"));
        settle_reuse(persona, "b", None);
        assert!(!COMMITTED.contains_key(&persona), "an untimed attempt drops the record");
        stage_reuse(persona, "c", &req("three"));
        stage_reuse(persona, "d", &req("four"));
        assert!(!COMMITTED.contains_key(&persona), "a request that never settled leaves nothing to compare");
        settle_reuse(persona, "c", Some((0, 10)));
        assert!(PENDING.contains_key(&persona), "a late completion of c must not erase d, staged since");
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
