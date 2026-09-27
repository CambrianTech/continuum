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
    let mut out = Vec::with_capacity(req.messages.len() + 1);
    out.push(format!("<system>{}", req.system_prompt.as_deref().unwrap_or("")));
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
}
