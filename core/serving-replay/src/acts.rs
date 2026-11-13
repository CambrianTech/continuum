//! An act sequence: the order in which minds submit acts, each act the full request the
//! mind sent. Two sources build one (`captures`, `synthetic`); the replay reads only this.

use serde::Serialize;

use crate::target::{ActRequest, MindId};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Act {
    pub mind: MindId,
    /// Where the act came from: a captured request id, or `synthetic:<mind>:<n>`.
    pub source: String,
    pub request: ActRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActSequence {
    /// What was replayed, e.g. `captures: 4 minds from ~/.continuum/fixtures/prompt-captures`.
    pub label: String,
    pub acts: Vec<Act>,
}

impl ActSequence {
    pub fn minds(&self) -> Vec<MindId> {
        let mut minds: Vec<MindId> = self.acts.iter().map(|a| a.mind.clone()).collect();
        minds.sort();
        minds.dedup();
        minds
    }
}

/// The reuse an act makes AVAILABLE: the share of its characters that sit in a prefix
/// unchanged since the same mind's previous act (system and tools, the leading messages
/// both acts carry unchanged, and the common start of the first message that differs).
/// An engine's achieved reuse is judged against this ceiling; the gap is ours to close.
/// Characters, not tokens: the ratio is template- and tokenizer-free, and a prefix of
/// equal text tokenizes the same, so the two ratios differ only at the boundary token.
pub fn available_reuse(previous: Option<&ActRequest>, current: &ActRequest) -> f64 {
    let total = request_chars(current);
    if total == 0 {
        return 0.0;
    }
    let Some(previous) = previous else {
        return 0.0;
    };
    if previous.system != current.system || previous.tools != current.tools {
        let a = previous.system.as_deref().unwrap_or_default(); // unwrap_or_default: no system prompt is the empty head
        let b = current.system.as_deref().unwrap_or_default(); // unwrap_or_default: no system prompt is the empty head
        return common_prefix_chars(a, b) as f64 / total as f64;
    }
    let mut shared = head_chars(current);
    for (p, c) in previous.messages.iter().zip(&current.messages) {
        if p == c {
            shared += c.role.len() + c.content.len();
            continue;
        }
        if p.role == c.role {
            shared += c.role.len() + common_prefix_chars(&p.content, &c.content);
        }
        break;
    }
    shared as f64 / total as f64
}

fn head_chars(r: &ActRequest) -> usize {
    r.system.as_ref().map_or(0, String::len) + r.tools.iter().map(|t| t.to_string().len()).sum::<usize>()
}

fn request_chars(r: &ActRequest) -> usize {
    head_chars(r) + r.messages.iter().map(|m| m.role.len() + m.content.len()).sum::<usize>()
}

fn common_prefix_chars(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or_else(|| a.len().min(b.len()), |((i, _), _)| i)
}

/// Keep the newest `n` characters of `text` (its end: what happened last), cut on a char
/// boundary. Used only to scale a replay down; equal texts stay equal, so the prefix
/// structure the reuse measurement depends on survives the scaling.
pub fn newest_chars(text: &str, n: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Message;

    fn req(msgs: &[(&str, &str)]) -> ActRequest {
        ActRequest {
            system: Some("sys".into()),
            messages: msgs.iter().map(|(r, c)| Message { role: (*r).into(), content: (*c).into() }).collect(),
            tools: vec![],
            n_predict: 1,
        }
    }

    // what this catches: a wrong ceiling. An append-only act must read as almost fully
    // reusable, a changed system prompt as almost nothing, and a mid-history edit only
    // up to the edit — otherwise the report blames the engine for the mind's prompt.
    #[test]
    fn available_reuse_counts_only_the_unchanged_prefix() {
        let a = req(&[("user", "hello world"), ("assistant", "ok")]);
        let appended = req(&[("user", "hello world"), ("assistant", "ok"), ("user", "x")]);
        let edited = req(&[("user", "hello there"), ("assistant", "ok")]);
        assert_eq!(available_reuse(None, &a), 0.0);
        let total = request_chars(&appended) as f64;
        assert_eq!(available_reuse(Some(&a), &appended), (total - 5.0) / total);
        let edited_total = request_chars(&edited) as f64;
        assert_eq!(available_reuse(Some(&a), &edited), (3.0 + 4.0 + 6.0) / edited_total);
        let mut other_system = appended.clone();
        other_system.system = Some("zzz".into());
        assert_eq!(available_reuse(Some(&a), &other_system), 0.0);
        assert_eq!(newest_chars("abcdé", 2), "dé");
        assert_eq!(newest_chars("ab", 5), "ab");
    }
}
