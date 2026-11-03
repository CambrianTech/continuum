//! One act, reduced on the node that holds its capture (THE-PROVING-GROUND 4a, P9): sizes, exact
//! token counts, timings, causal ids, content hashes and outcome kinds. No field holds words: every
//! string is a closed value (`closed`), so a reduction can leave its node.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::closed::{GitSha, ModelId, Sha256, Slug};
use crate::provenance::{Joined, Known};

/// The tokenizer the counts were taken with: the serving model's own, by id AND artifact, so the
/// same capture reduces to the same numbers on any node (Codex on #5618).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenizerIdentity {
    pub model: ModelId,
    /// The artifact's revision (a commit or registry revision).
    pub revision: Slug,
    /// SHA-256 of the tokenizer artifact's bytes.
    pub artifact: Sha256,
}

/// Why the act ran. The capture's own causes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// Her own time: nothing addressed her.
    Ambient,
    /// Something arrived for her: a message, a result, a mention.
    Stimulus,
    /// The substrate made the act (a probe, a warm-up).
    Synthetic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActIdentity {
    pub persona: Uuid,
    pub room: Uuid,
    pub request: Uuid,
    pub session: Uuid,
    pub cycle: u64,
    pub build: GitSha,
    pub captured_at_ms: u64,
    pub cause: Cause,
    /// The act that started this causal chain; `None` when this act is the root.
    pub cause_root: Option<Uuid>,
}

/// Anything sized and identified without its words: a system prompt, a reasoning run, a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sized {
    pub tokens: u32,
    pub hash: Sha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageReduction {
    pub role: Role,
    pub tokens: u32,
    pub hash: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolReduction {
    pub name: Slug,
    pub schema_hash: Sha256,
    pub tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestReduction {
    pub model: ModelId,
    pub context_window: u32,
    pub max_tokens: Known<u32>,
    /// The request's purpose as the capture names it (`cognition/act`, `cognition/deliberation`):
    /// an open registry, so a closed slug rather than an enum this crate would have to grow.
    pub purpose: Slug,
    pub system_prompt: Sized,
    pub messages: Vec<MessageReduction>,
    pub tools: Vec<ToolReduction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Stop,
    ToolUse,
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub input: u32,
    pub output: u32,
}

/// The engine's own timing for the act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    /// Prompt tokens the engine reused instead of prefilling.
    pub cached: u32,
    pub prefill_tokens: u32,
    pub prefill_ms: u64,
    pub decode_tokens: u32,
    pub decode_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallReduction {
    pub name: Slug,
    /// The canonical arguments' fingerprint (MIND-CONTEXT invariant 4): repeats are visible
    /// without the words.
    pub args: Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseReduction {
    pub finish: Known<Finish>,
    pub usage: Known<Usage>,
    pub timing: Known<Timing>,
    pub tool_calls: Vec<ToolCallReduction>,
    /// Kept apart from the text: a cut inside thinking is not an answer.
    pub reasoning: Known<Sized>,
    pub text: Known<Sized>,
}

/// One claim in the act's allocation receipt (`delib.window.flex`, `delib.view.sized`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimReduction {
    pub source: Slug,
    pub floor: u32,
    pub want: u32,
    pub granted: u32,
}

/// The mechanism a known-bad fails on, not only its symptom (4a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationReduction {
    pub claims: Vec<ClaimReduction>,
    pub unallocated: u32,
    /// The act started a new epoch of her view.
    pub turnover: bool,
}

/// What happened on the wire. `failure` is the producer's typed class (a closed slug), never its
/// message: today's captures carry the message, so the reducer maps it to a class before writing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum Delivery {
    Completed {
        elapsed_ms: u64,
        attempts: u32,
    },
    Cancelled {
        elapsed_ms: u64,
        failure: Slug,
    },
    Failed {
        elapsed_ms: u64,
        failure: Slug,
    },
}

/// What the act did, as the producer reports it (HOW-A-MIND section 7), never read from text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    EditApplied,
    MessageSent,
    PrOpened,
    MoveMade,
    ToolError,
    Unfulfilled,
}

/// Where the engine resumed the act's prompt from (`llama-server`'s `lcp after reuse`, its checkpoint
/// restores, and the core's page probes). The tokens it then re-read are `Timing::prefill_tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case", deny_unknown_fields)]
pub enum Resume {
    /// Her KV was resident in the slot, and the engine kept this many tokens of it.
    SlotPrefix { kept: u32 },
    /// Rolled back to a context checkpoint ending at this token (a hybrid model resumes only there).
    Checkpoint { at: u32 },
    /// Paged in from disk, and the engine kept this many tokens of the page.
    PageRestored { kept: u32 },
    /// The page was refused, and the act prefilled cold. The class names why (`adapter_identity`,
    /// `no_space`), never the engine's message.
    PageRefused { class: Slug },
    /// Nothing was reusable.
    Cold {},
}

/// One act, with no words in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActReduction {
    pub identity: ActIdentity,
    pub tokenizer: TokenizerIdentity,
    pub request: RequestReduction,
    /// `Unavailable` when the act never produced a response record (cancelled before dispatch).
    pub response: Known<ResponseReduction>,
    /// The reasoning budget actually sent. The engine adds it after capture, so older evidence
    /// joins it from `delib.act.reasoning_budgeted` by persona and time.
    pub reasoning_budget: Joined<u32>,
    pub allocation: Joined<AllocationReduction>,
    /// Joined from the engine log and page probes by slot and time; going forward the capture binds
    /// it by request id.
    pub resume: Joined<Resume>,
    /// Tokens of each view item, in order (count and size, no words).
    pub view_items: Vec<u32>,
    pub delivery: Delivery,
    pub outcomes: Vec<OutcomeKind>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(b: u8) -> String {
        format!("{b:02x}").repeat(32)
    }

    fn reduction_json() -> serde_json::Value {
        serde_json::json!({
            "identity": {
                "persona": "18c04c5b-e059-4129-816f-75e8e58fd74c", "room": "5d33e2a7-67b4-521b-9911-f46724af297f",
                "request": "f7b98ca4-2591-4d32-9b8d-54bf30788ac7", "session": "6c359fbd-a7b5-4532-9fca-42a1dd9ffe2a",
                "cycle": 1, "build": "f6f331d98", "captured_at_ms": 1_792_969_747_125_u64, "cause": "ambient", "cause_root": null
            },
            "tokenizer": { "model": "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF", "revision": "1af47f78b1", "artifact": hash(1) },
            "request": {
                "model": "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF", "context_window": 32768,
                "max_tokens": { "state": "recorded", "value": 4096 }, "purpose": "cognition/act",
                "system_prompt": { "tokens": 1200, "hash": hash(2) },
                "messages": [{ "role": "user", "tokens": 300, "hash": hash(3) }],
                "tools": [{ "name": "code/read", "schema_hash": hash(4), "tokens": 80 }]
            },
            "response": { "state": "recorded", "value": {
                "finish": { "state": "recorded", "value": "tool_use" },
                "usage": { "state": "unavailable" },
                "timing": { "state": "recorded", "value": { "cached": 3000, "prefill_tokens": 1500, "prefill_ms": 116000, "decode_tokens": 40, "decode_ms": 9000 } },
                "tool_calls": [{ "name": "code/read", "args": hash(5) }],
                "reasoning": { "state": "unavailable" },
                "text": { "state": "recorded", "value": { "tokens": 40, "hash": hash(6) } }
            }},
            "reasoning_budget": { "join": "ambiguous", "candidates": 2 },
            "allocation": { "join": "unresolved" },
            "resume": { "join": "exact", "value": { "from": "page_refused", "class": "adapter_identity" } },
            "view_items": [1200, 300],
            "delivery": { "outcome": "completed", "elapsed_ms": 125000, "attempts": 1 },
            "outcomes": ["tool_error"]
        })
    }

    // what this catches: the reduction shape drifting from what the reducer writes and the
    // harness reads; one round trip through the one type.
    #[test]
    fn a_reduction_round_trips() {
        let r: ActReduction = serde_json::from_value(reduction_json()).expect("a reduction");
        assert_eq!(r.reasoning_budget.exact(), None, "ambiguous carries no budget");
        let back = serde_json::to_value(&r).expect("ser");
        assert_eq!(serde_json::from_value::<ActReduction>(back).expect("de"), r);
    }

    // what this catches: words surviving into a reduction (P9). Every string slot gets a planted
    // sentence, and one extra field carries one: none may parse. A new free-text field would make
    // its slot accept the sentence, and this test fails there.
    #[test]
    fn planted_words_never_survive() {
        let sentence = serde_json::json!("I think the deploy broke because Joel said so");
        let slots: &[&[&str]] = &[
            &["identity", "build"],
            &["identity", "persona"],
            &["tokenizer", "model"],
            &["tokenizer", "revision"],
            &["request", "purpose"],
            &["request", "system_prompt", "hash"],
            &["request", "tools", "0", "name"],
            &["response", "value", "tool_calls", "0", "name"],
            &["response", "value", "tool_calls", "0", "args"],
            &["resume", "value", "class"],
        ];
        for path in slots {
            let mut v = reduction_json();
            let mut at = &mut v;
            for key in *path {
                at = match key.parse::<usize>() {
                    Ok(i) => &mut at[i],
                    Err(_) => &mut at[*key],
                };
            }
            *at = sentence.clone();
            assert!(serde_json::from_value::<ActReduction>(v).is_err(), "words parsed at {path:?}");
        }
        // An unknown field anywhere is refused, not ignored: the pre-commit check parses the file
        // on disk, so a field serde skipped would pass the check with its words still committed.
        for nest in [&[][..], &["identity"][..], &["request", "messages", "0"][..], &["response", "value", "timing", "value"][..]] {
            let mut v = reduction_json();
            let mut at = &mut v;
            for key in nest {
                at = match key.parse::<usize>() {
                    Ok(i) => &mut at[i],
                    Err(_) => &mut at[*key],
                };
            }
            at["note"] = sentence.clone();
            assert!(serde_json::from_value::<ActReduction>(v).is_err(), "an unknown field carried words at {nest:?}");
        }
    }
}
