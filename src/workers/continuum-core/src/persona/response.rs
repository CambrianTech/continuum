//! Per-persona response orchestration in Rust. The Rust replacement for
//! `PersonaResponseGenerator.ts` — owns the cognitive verb of "this
//! persona, given this message in this room, produces this response."
//!
//! See docs/architecture/SHARED-COGNITION.md for the architectural
//! picture and docs/architecture/PERSONA-COGNITION-RUST-MIGRATION.md
//! for the migration discipline this module is the first rung of.
//!
//! Pipeline (per persona, per inbound message):
//!
//!   1. cognition::analyze(...)   — shared, cached. Run once per
//!                                  message; this persona's call hits
//!                                  the cache after the first.
//!   2. cognition::score_persona(...) — local. Just THIS persona's
//!                                       relevance; no need to know
//!                                       about others.
//!   3. If !should_respond → return Silent { reason }. First-class
//!      outcome — silence with an observable reason, not a hidden skip.
//!   4. prompt_assembly::build(...) — persona-specific prompt: voice,
//!                                    LoRA-rendered specialty, RAG
//!                                    context interleaving. (TODO:
//!                                    memento's persona/prompt_assembly
//!                                    module ships in this PR.)
//!   5. ai_provider::generate_text(...) — inference (DMR or whatever
//!                                        adapter the registry picks).
//!   6. strip_thinks_emit_events(...) — extract <think>...</think>
//!                                       blocks, emit them as
//!                                       cognition:think-block events
//!                                       for the (future) hippocampus
//!                                       to consume, return clean
//!                                       speech for posting.
//!   7. Return Spoke { text, ... } with timing + diagnostic fields.
//!
//! Why this is in Rust (not just a port):
//!   - Cognition is where the mind/machine line gets drawn — concurrency
//!     primitives matter (Joel, 2026-04-19).
//!   - SharedAnalysis cache lives here; needs lock-free DashMap for
//!     concurrent personas hitting the same message.
//!   - Per-persona renders run in parallel tokio tasks; Node's single
//!     event loop blocks every persona on every other persona's
//!     inference call.
//!   - <think> parsing is a hot path on every response; regex/str
//!     manipulation in Rust is ~100x what TS does on the same input.

use crate::cognition::{
    analyze, score_persona, AnalysisInput, PersonaSlot, RecentMessage, SharedAnalysis,
    DEFAULT_RELEVANCE_THRESHOLD,
};
use crate::cognition::types::ResponderDecision;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use ts_rs::TS;
use uuid::Uuid;

/// Input to `respond()`. Caller (chat path / PRG.ts shim) collects this
/// from the room state. Carries everything needed for ONE persona's
/// response cycle — analysis is shared via cache, so no need to pass
/// other personas in.
#[derive(Debug, Clone)]
pub struct RespondInput {
    /// THIS persona's identity + specialty for scoring.
    pub persona: PersonaSlot,
    pub room_id: Uuid,
    pub message_id: Uuid,
    /// The new message that triggered this response cycle.
    pub message_text: String,
    /// Recent messages for analysis context. Most-recent last.
    pub recent_history: Vec<RecentMessage>,
    /// Stable specialty identifiers in the room (all personas in the
    /// room, not just this one). The analyzer uses this list to know
    /// which `suggested_angles` keys to populate. This persona's own
    /// specialty must appear here.
    pub known_specialties: Vec<String>,
}

/// What `respond()` returns.
///
/// `Silent` is a first-class outcome: the persona considered the message,
/// found nothing additive to add, and chose not to speak. The reason is
/// observable for trainability + the persona's own meta-cognitive trace.
/// Not the same as a failure.
///
/// `Spoke` is the response that should be posted to the room.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export, export_to = "../../../shared/generated/cognition/PersonaResponse.ts")]
pub enum PersonaResponse {
    /// Persona chose silence. Reason carried for observability + training.
    Silent {
        #[ts(type = "string")]
        persona_id: Uuid,
        reason: String,
        /// Relevance score that drove the decision. 0.0..1.0. Carried so
        /// downstream telemetry can analyze the silence-rate by score
        /// distribution.
        relevance_score: f32,
    },
    /// Persona produced a response. The text is the CLEANED visible
    /// speech (`<think>` blocks already stripped + emitted as events).
    Spoke {
        #[ts(type = "string")]
        persona_id: Uuid,
        /// Cleaned visible speech to post to the room. `<think>` blocks
        /// have been stripped; the visible response is what the user
        /// sees in chat.
        text: String,
        /// Model that produced the response (post-routing).
        model_used: String,
        /// Duration of the inference call itself (not including
        /// analysis or scoring — those are separate).
        inference_ms: u64,
        /// Total duration end-to-end (analysis + scoring + inference +
        /// parsing + event emission).
        total_ms: u64,
        /// Number of `<think>` blocks extracted (for telemetry —
        /// the actual content was emitted as events for hippocampus).
        think_blocks_emitted: u32,
    },
}

/// THE per-persona response cycle. Public entry point.
///
/// Called by the chat path (or the PRG.ts shim that the chat path
/// currently calls). Each call is for ONE persona; the shared analysis
/// is cached at the cognition layer, so M personas calling this
/// concurrently for the same message do M renders + 1 analysis (not M).
///
/// Returns `Result` because inference can genuinely fail (model down,
/// timeout, bad output we can't parse). Failure should propagate to
/// the caller for proper user-facing error reporting; we don't
/// silently fall back to "Silent" because that would hide real bugs.
pub async fn respond(input: RespondInput) -> Result<PersonaResponse, String> {
    let total_start = now_ms();

    // 1. Shared analysis (cached per message+room+history fingerprint).
    let analysis = analyze(AnalysisInput {
        message_id: input.message_id,
        room_id: input.room_id,
        text: input.message_text.clone(),
        recent_history: input.recent_history.clone(),
        known_specialties: input.known_specialties.clone(),
    })
    .await?;

    // 2. Local score for THIS persona only. No need to know about others.
    let decision = score_persona(&analysis, &input.persona, DEFAULT_RELEVANCE_THRESHOLD);

    // 3. Silent path is first-class.
    if !decision.should_respond {
        return Ok(PersonaResponse::Silent {
            persona_id: input.persona.persona_id,
            reason: decision.explanation,
            relevance_score: decision.relevance_score,
        });
    }

    // 4–6. Build prompt, run inference, parse <think>.
    //
    // The prompt-assembly + inference + parse work is the next chunk
    // of this PR. Memento is taking persona/prompt_assembly.rs (port
    // of PersonaPromptAssembler.ts logic). My piece here calls into
    // his module + ai_provider + strip_thinks_emit_events.
    //
    // Stubbed for now so this file compiles + the shape is reviewable.
    // Will be filled in (no port debt — this is the final-form code,
    // just incomplete) before the chat-validation gate.
    let inference_start = now_ms();
    let raw_response = run_render(&input, &analysis, &decision).await?;
    let inference_ms = now_ms().saturating_sub(inference_start);

    let (visible_text, think_count) = strip_thinks_emit_events(
        &raw_response.text,
        input.persona.persona_id,
        input.message_id,
    );

    Ok(PersonaResponse::Spoke {
        persona_id: input.persona.persona_id,
        text: visible_text,
        model_used: raw_response.model_used,
        inference_ms,
        total_ms: now_ms().saturating_sub(total_start),
        think_blocks_emitted: think_count,
    })
}

/// What the render step returns internally (private — public type is
/// `PersonaResponse`).
struct RawRenderOutput {
    text: String,
    model_used: String,
}

/// Runs the prompt-assembly + inference for one persona's render.
///
/// **STUB** — to be filled in alongside `persona/prompt_assembly.rs`
/// (memento's slice). Final shape: builds a short specialty-grounded
/// prompt using the analysis's `suggested_angles` for this persona,
/// includes `relevant_context` (cleaned distillation, no `<think>`
/// pollution), runs `ai_provider::generate_text`, returns the raw
/// model output.
///
/// The current stub returns an error so the rest of this file
/// compiles + downstream callers (IPC command, PRG.ts shim) can be
/// wired to the final shape now without waiting for the inference
/// integration. End-state shape from day one; just incomplete.
async fn run_render(
    _input: &RespondInput,
    _analysis: &SharedAnalysis,
    _decision: &ResponderDecision,
) -> Result<RawRenderOutput, String> {
    // TODO(A.3 integration): wire to persona/prompt_assembly.rs +
    // ai_provider::generate_text. Stub returns error so the missing
    // wiring fails loud rather than silently returning empty.
    Err("persona/response.rs::run_render not yet wired to prompt_assembly + ai_provider".to_string())
}

/// Extract `<think>...</think>` blocks from the model's output. Emits
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
fn strip_thinks_emit_events(raw: &str, persona_id: Uuid, message_id: Uuid) -> (String, u32) {
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! Pure-logic tests for the parts that don't require live inference.
    //! End-to-end inference test happens through chat-validation.
    use super::*;

    #[test]
    fn strip_thinks_extracts_single_block() {
        let raw = "<think>I should be helpful</think>Hello there.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "Hello there.");
        assert_eq!(count, 1);
    }

    #[test]
    fn strip_thinks_extracts_multiple_blocks() {
        let raw = "<think>plan</think>First sentence. <think>double-check</think>Second.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "First sentence. Second.");
        assert_eq!(count, 2);
    }

    #[test]
    fn strip_thinks_handles_multiline_thinks() {
        let raw = "<think>\nLine one\nLine two\n</think>\nVisible response.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "Visible response.");
        assert_eq!(count, 1);
    }

    #[test]
    fn strip_thinks_no_block_returns_unchanged() {
        let raw = "Just a normal response with no thinking.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "Just a normal response with no thinking.");
        assert_eq!(count, 0);
    }

    #[test]
    fn strip_thinks_unterminated_keeps_text() {
        // Model truncated mid-think (rare but real). Don't lose data.
        let raw = "<think>This was cut off because max_tokens";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert!(visible.contains("<think>"));
        assert_eq!(count, 0);
    }
}
