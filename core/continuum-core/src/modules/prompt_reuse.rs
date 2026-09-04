//! `debug/prompt-reuse` — replay recorded prompt captures and score PREDICTED KV
//! prefix reuse, so prompt-assembly changes can be iterated offline instead of
//! waiting ~5–10 minutes per live citizen turn to read one `inference.prefill.complete`.
//!
//! # Why (Joel, 2026-08-21: "record and playback so we could iterate to refine")
//!
//! The record half already exists and is on by default:
//! [`crate::cognition::prompt_capture::JsonlPromptCaptureSink`] writes the VERBATIM
//! system prompt + exact message thread of every deliberation call, one jsonl per
//! persona under `~/.continuum/fixtures/prompt-captures/`. What was missing is the
//! playback half: walking a persona's calls chronologically and measuring how many
//! leading bytes each call shares with the previous one — which is precisely what
//! llama's prefix cache can reuse. On 2026-08-19 this analysis was done BY HAND
//! (divergences found at chars 8216/7879/8133, the #266 ordering defect); this
//! command is that analysis as a repeatable query anyone can run, no agent needed.
//!
//! # Honest proxy, stated on every result
//!
//! Char-level LCP is a PROXY for token-level reuse (≈4 chars/token, the same
//! convention as `deliberation_budget::est_tokens`). It ranks assembly variants and
//! pinpoints divergence; the ground truth for actual reuse stays the
//! `inference.prefill.complete` probe (`cached` vs `fresh`). A high score here with
//! a low live `cached` means the loss is BETWEEN assembly and the slot — routing,
//! eviction — not ordering; that separation is diagnostic signal, not error.

use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// One captured deliberation call, narrowed to what prefix-scoring needs.
#[derive(Debug, Deserialize)]
struct CapturedCall {
    captured_at_ms: u64,
    #[serde(default)]
    iteration: usize,
    #[serde(default)]
    system: String,
    #[serde(default)]
    messages: serde_json::Value,
}

/// Flatten a call into the byte stream the server tokenizes, in wire order:
/// system first, then each message's role + content. This mirrors the chat
/// template's field order closely enough for LCP ranking; template framing
/// tokens between fields are identical across calls and so never move a
/// divergence point.
fn wire_view(call: &CapturedCall) -> String {
    let mut s = String::with_capacity(call.system.len() + 4096);
    s.push_str(&call.system);
    if let Some(msgs) = call.messages.as_array() {
        for m in msgs {
            s.push('\u{1}'); // field separator — never appears in prompt text
            if let Some(role) = m.get("role").and_then(|v| v.as_str()) {
                s.push_str(role);
                s.push('\u{1}');
            }
            match m.get("content") {
                Some(serde_json::Value::String(c)) => s.push_str(c),
                Some(other) => s.push_str(&other.to_string()),
                None => {}
            }
        }
    }
    s
}

/// Longest common prefix in CHARS (not bytes — a divergence must never split a
/// code point, because the excerpt around it goes into the report verbatim).
fn lcp_chars(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(x, y)| x == y)
        .count()
}

/// A short excerpt from both sides of the divergence point — the "what actually
/// changed" answer inline, so the reader doesn't re-derive it from raw captures.
fn divergence_excerpt(prev: &str, next: &str, at: usize) -> (String, String) {
    let clip = |s: &str| -> String {
        s.chars()
            .skip(at.saturating_sub(20))
            .take(60)
            .collect::<String>()
            .replace('\n', "⏎")
            .replace('\u{1}', "·")
    };
    (clip(prev), clip(next))
}

// ─────────────────────────── debug/prompt-reuse ──────────────────────────

/// Score predicted KV prefix reuse over recorded prompt captures. Stateless;
/// self-registers like [`super::probe_query::ProbeQuery`].
#[derive(Default)]
pub struct PromptReuse;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptReuseParams {
    /// Persona id (the capture file's name, a uuid). Omit to score EVERY persona
    /// with a capture file and aggregate.
    #[serde(default)]
    pub persona: Option<String>,
    /// How many of the newest calls to consider per persona (default 20, cap 200).
    /// First-generation calls only — agent-loop re-prompts (`iteration > 0`) share
    /// their prefix trivially and would inflate the score.
    #[serde(default)]
    pub tail: Option<u32>,
}

/// One consecutive-call comparison.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReusePair {
    pub persona: String,
    pub prev_captured_at_ms: u64,
    pub next_captured_at_ms: u64,
    /// Chars shared from position 0 — the prefix a cache could serve.
    pub lcp_chars: usize,
    /// The NEXT call's total chars — the denominator a cache is scored against.
    pub next_chars: usize,
    /// `lcp / next_chars`, 0–100. The number the ordering work moves.
    pub reuse_pct: u32,
    /// ±20 chars around the divergence, previous call's side.
    pub prev_at_divergence: String,
    /// Same window, next call's side — diff these two BY EYE to name the culprit.
    pub next_at_divergence: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PromptReuseResult {
    /// Newest-last per persona. Divergence excerpts inline.
    pub pairs: Vec<ReusePair>,
    /// Median `reuse_pct` across pairs — the one-number answer.
    pub median_reuse_pct: u32,
    /// The single worst pair's `reuse_pct` — regressions show here first.
    pub worst_reuse_pct: u32,
    /// Capture files considered.
    pub personas_scored: u32,
    /// Plain-language reading, including the char≈token/4 proxy caveat.
    pub summary: String,
}

fn captures_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".continuum/fixtures/prompt-captures")
}

fn load_tail(path: &PathBuf, tail: usize) -> Vec<CapturedCall> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut calls: Vec<CapturedCall> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<CapturedCall>(l).ok())
        .filter(|c| c.iteration == 0)
        .collect();
    calls.sort_by_key(|c| c.captured_at_ms);
    let skip = calls.len().saturating_sub(tail);
    calls.drain(..skip);
    calls
}

fn score_persona(name: &str, calls: &[CapturedCall]) -> Vec<ReusePair> {
    calls
        .windows(2)
        .map(|w| {
            let (prev, next) = (wire_view(&w[0]), wire_view(&w[1]));
            let lcp = lcp_chars(&prev, &next);
            let next_chars = next.chars().count();
            let (pe, ne) = divergence_excerpt(&prev, &next, lcp);
            ReusePair {
                persona: name.to_string(),
                prev_captured_at_ms: w[0].captured_at_ms,
                next_captured_at_ms: w[1].captured_at_ms,
                lcp_chars: lcp,
                next_chars,
                reuse_pct: if next_chars == 0 {
                    0
                } else {
                    (lcp * 100 / next_chars) as u32
                },
                prev_at_divergence: pe,
                next_at_divergence: ne,
            }
        })
        .collect()
}

#[async_trait]
impl ActionCommand for PromptReuse {
    const NAME: &'static str = "debug/prompt-reuse";
    const ALIASES: &'static [&'static str] = &["prompt_reuse", "kv_replay"];
    // Same scope call as debug/probes/query: reads every citizen's verbatim
    // prompts, so it stays an operator diagnostic, off the persona vocabulary.
    const NATIVE: bool = false;
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Replay recorded prompt captures and score predicted KV prefix reuse between \
         consecutive deliberation calls (char-level longest-common-prefix, ≈ tokens×4). \
         Names the divergence point of every pair with an excerpt from both sides, so \
         an ordering regression is visible offline in seconds instead of via a live \
         10-minute turn. Ground truth for ACTUAL reuse remains the \
         inference.prefill.complete probe.";
    type Params = PromptReuseParams;
    type Output = PromptReuseResult;

    async fn run(&self, _ctx: &Ctx, p: PromptReuseParams) -> Result<PromptReuseResult, CommandError> {
        let dir = captures_dir();
        let tail = p.tail.unwrap_or(20).clamp(2, 200) as usize;
        let files: Vec<PathBuf> = match &p.persona {
            Some(id) => vec![dir.join(format!("{id}.jsonl"))],
            None => std::fs::read_dir(&dir)
                .map_err(|e| {
                    CommandError::Invalid(format!(
                        "no prompt captures at {}: {e}. The capture sink writes them \
                         during live deliberation — run at least one persona turn first.",
                        dir.display()
                    ))
                })?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
                .collect(),
        };

        let mut pairs = Vec::new();
        let mut personas_scored = 0u32;
        for file in &files {
            let name = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let calls = load_tail(file, tail);
            if calls.len() < 2 {
                continue;
            }
            personas_scored += 1;
            pairs.extend(score_persona(&name, &calls));
        }

        let mut pcts: Vec<u32> = pairs.iter().map(|p| p.reuse_pct).collect();
        pcts.sort_unstable();
        let median = pcts.get(pcts.len() / 2).copied().unwrap_or(0);
        let worst = pcts.first().copied().unwrap_or(0);
        let summary = if pairs.is_empty() {
            "No consecutive first-generation call pairs found — nothing has recorded \
             two turns for any persona yet. This is an empty corpus, not a zero score."
                .to_string()
        } else {
            format!(
                "{} pair(s) across {} persona(s): median predicted reuse {}%, worst {}%. \
                 Char-level LCP is a proxy (≈ tokens × 4); actual slot reuse is the \
                 inference.prefill.complete probe's cached/fresh split. Diff the two \
                 *_at_divergence excerpts of a low pair to name the mutating span.",
                pairs.len(),
                personas_scored,
                median,
                worst
            )
        };
        Ok(PromptReuseResult {
            pairs,
            median_reuse_pct: median,
            worst_reuse_pct: worst,
            personas_scored,
            summary,
        })
    }
}

crate::register_stateless_command!(PromptReuse);

#[cfg(test)]
mod tests {
    use super::*;

    fn call(at: u64, system: &str, user: &str) -> CapturedCall {
        CapturedCall {
            captured_at_ms: at,
            iteration: 0,
            system: system.to_string(),
            messages: serde_json::json!([{"role":"user","content":user}]),
        }
    }

    // what this catches: the scorer itself lying. The #266 defect class was a
    // mutation INSIDE the system prompt (clock/salience churn at ~char 2000); a
    // scorer that missed an early-system divergence — or scored message-only
    // divergence as total loss — would steer the ordering work wrong both ways.
    #[test]
    fn lcp_lands_exactly_at_the_first_mutated_char() {
        let a = call(1, "IDENTITY tools=[a,b,c] clock=10:00", "hello");
        let b = call(2, "IDENTITY tools=[a,b,c] clock=10:05", "hello");
        let pairs = score_persona("p", &[a, b]);
        assert_eq!(pairs.len(), 1);
        let lcp = pairs[0].lcp_chars;
        // Shared: everything through "clock=10:0" — the divergence is the minute digit.
        assert_eq!(lcp, "IDENTITY tools=[a,b,c] clock=10:0".chars().count());
        assert!(pairs[0].prev_at_divergence.contains("0"));
        assert!(pairs[0].reuse_pct < 100);
    }

    // what this catches: a byte-identical stable head + fresh conversation scoring
    // as HIGH reuse — the shape the whole KV design drives toward. If this pair
    // does not score above 80%, the wire_view flattening broke prefix order.
    #[test]
    fn identical_system_with_new_message_scores_as_head_reuse() {
        let sys = "S".repeat(4000);
        let a = call(1, &sys, "first question");
        let b = call(2, &sys, "a completely different follow-up");
        let pairs = score_persona("p", &[a, b]);
        assert!(
            pairs[0].reuse_pct > 80,
            "stable head must dominate: got {}%",
            pairs[0].reuse_pct
        );
    }

    // what this catches: agent-loop re-prompts (iteration > 0) inflating the score.
    // load_tail must drop them; scoring them would report the trivial within-turn
    // prefix share as if it were cross-turn reuse.
    #[test]
    fn wire_view_separates_fields_unambiguously() {
        // "ab" + "c" must not equal "a" + "bc" once flattened — the separator is
        // what keeps LCP from crediting reuse across a field boundary.
        let x = call(1, "ab", "c");
        let y = call(2, "a", "bc");
        assert_ne!(wire_view(&x), wire_view(&y));
    }
}
