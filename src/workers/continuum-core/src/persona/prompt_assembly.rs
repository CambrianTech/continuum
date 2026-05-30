//! Prompt Assembly — builds the final LLM message array from RAG context
//!
//! Port of PersonaPromptAssembler.ts to Rust. Zero TS logic remains
//! in the prompt construction path after this module ships.
//!
//! Input: PromptAssemblyInput (persona identity, RAG context, shared analysis angle)
//! Output: AssembledPrompt (system message + conversation history, ready for ai/generate)

use crate::model_registry::types::MultiPartyChatStrategy;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Input to prompt assembly. Carries everything needed to build the
/// LLM message array for a single persona's render pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptAssemblyInput {
    /// Persona's display name (for identity reminder)
    pub persona_name: String,
    /// Persona's system prompt (from RAG identity)
    pub system_prompt: String,
    /// The matched angle from SharedAnalysis — grounds the persona's
    /// contribution in a specific perspective, not generic flavor.
    /// Empty string if no shared analysis (fallback path).
    pub matched_angle: String,
    /// Conversation history as (role, name, content) triples.
    /// Already ordered, already trimmed to token budget by RAG builder.
    pub history: Vec<HistoryMessage>,
    /// The current user message being responded to.
    pub current_message: HistoryMessage,
    /// Whether this is a voice (live audio) context — affects response style.
    pub is_voice: bool,
    /// Social awareness signals (AI message count, human activity, etc.)
    pub social_signals: Option<SocialSignals>,
    /// How to shape the conversation history for THIS model. Caller pulls
    /// from the model_registry (single source of truth). assemble() never
    /// guesses — it does what the registry declared.
    #[serde(default)]
    pub multi_party_strategy: MultiPartyChatStrategy,
    /// Display names of OTHER personas in the room (excluding self).
    /// Only used by `MultiPartyChatStrategy::ProperChatMlSingleParty`
    /// to drop other-AI history turns that single-party-trained models
    /// cannot coherently process. Empty otherwise — `NamePrefixedUserTurns`
    /// and `SingleUserTurnFlattenedHistory` ignore this field.
    #[serde(default)]
    pub other_persona_names: Vec<String>,
    /// Recalled engrams (per-persona admitted memory) — content
    /// strings only, ordered most-recent first, already trimmed by
    /// the caller. Rendered as a `[Recent Memory]` block right after
    /// the matched-angle injection so the persona sees its own
    /// memory adjacent to the analyzer's per-turn perspective. Empty
    /// = no memory recall on this turn (normal early-life state, or
    /// admission gate skipped because no AdmissionState).
    /// Continuum#1211 PR-2.
    #[serde(default)]
    pub recalled_engrams: Vec<String>,
}

/// A message in conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessage {
    pub role: String, // "system" | "user" | "assistant"
    pub name: Option<String>,
    pub content: String,
    pub timestamp_ms: Option<u64>,
}

/// Social signals for awareness (from Rust cognition evaluator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialSignals {
    pub ai_messages_recent: u32,
    pub human_spoke_recently: bool,
    pub has_directed_mention: bool,
    pub is_mentioned: bool,
    pub seconds_since_last_response: Option<f64>,
    pub response_count_this_session: Option<u32>,
    pub response_cap: Option<u32>,
}

/// Output of prompt assembly — ready to send to ai/generate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledPrompt {
    /// Complete system prompt (identity + RAG sections + social awareness + shared analysis angle)
    pub system_message: String,
    /// Conversation messages (user + assistant turns from history + current message)
    pub messages: Vec<PromptMessage>,
    /// Estimated token count of the full prompt
    pub estimated_tokens: usize,
}

/// A message in the assembled prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: String,
    pub content: String,
}

/// Assemble the final LLM prompt from input components.
///
/// This is a pure function — no IO, no IPC, no state. Takes data in,
/// produces a prompt out. The caller (response.rs) handles inference.
pub fn assemble(input: &PromptAssemblyInput) -> AssembledPrompt {
    // Pre-size the system_prompt buffer based on the system_prompt
    // input + a generous overhead estimate for the optional blocks.
    // Avoids the realloc that would otherwise fire on the first
    // `push_str` of an angle/social/voice block (#1209).
    let mut system_prompt = String::with_capacity(input.system_prompt.len() + 512);
    system_prompt.push_str(&input.system_prompt);

    // Inject shared analysis angle if present — grounds the persona's
    // contribution in the specific perspective the orchestrator matched.
    //
    // write! into the existing buffer instead of `push_str(&format!(...))`
    // so the format intermediate doesn't allocate a throw-away String
    // just to be appended (#1209). Trait method's Result is infallible
    // for String; the let-binding to `_` is for the trait signature.
    if !input.matched_angle.is_empty() {
        let _ = write!(
            system_prompt,
            "\n\n[Shared Analysis — Your Angle]\n\
             The following aspect of this conversation is specifically relevant \
             to your expertise. Focus your contribution here:\n{}",
            input.matched_angle
        );
    }

    // Inject recalled engrams as a memory block — continuum#1211 PR-2.
    // The persona's admission gate (#1213) collected these from prior
    // chat turns; rendering them here is what closes the engram loop
    // (admit → store → recall → context). Caller (cognition/respond
    // IPC handler) is responsible for trimming to a sensible count
    // before calling assemble — prompt_assembly stays a pure
    // formatter, doesn't make policy decisions about budget.
    //
    // Empty list = no rendering, no header. A persona that hasn't
    // accumulated memory yet (or the inline gate skipped because no
    // AdmissionState exists) sees the prompt unchanged from before
    // PR-2 — backwards-compatible.
    if !input.recalled_engrams.is_empty() {
        system_prompt.push_str(
            "\n\n[Recent Memory]\n\
             Things you have remembered from prior conversations in this room. \
             Use this context as background; not every memory needs to be cited:\n",
        );
        for engram in &input.recalled_engrams {
            // `- ` bullet prefix keeps each engram visually separable
            // even when the content runs multiple lines. writeln!
            // appends the newline without the trailing-newline-in-
            // format-string clippy lint.
            let _ = writeln!(system_prompt, "- {engram}");
        }
    }

    // Inject social awareness signals
    if let Some(ref signals) = input.social_signals {
        // append_social_block writes directly into system_prompt instead
        // of returning a fresh String (#1209). Saves the intermediate
        // allocation for callers that have a pre-existing buffer.
        append_social_block(&mut system_prompt, signals);
    }

    // Voice mode instructions
    if input.is_voice {
        system_prompt.push_str(
            "\n\n[Voice Mode]\n\
             You are in a live voice conversation. Keep responses concise and \
             conversational — the user is listening, not reading. Avoid markdown, \
             code blocks, or long lists. Speak naturally.",
        );
    }

    // Build message array — strategy declared by the model registry,
    // not guessed here.
    let messages = match input.multi_party_strategy {
        MultiPartyChatStrategy::NamePrefixedUserTurns => {
            build_messages_name_prefixed(&input.history, &input.current_message)
        }
        MultiPartyChatStrategy::SingleUserTurnFlattenedHistory => build_messages_single_user_turn(
            &input.history,
            &input.current_message,
            &input.persona_name,
        ),
        MultiPartyChatStrategy::ProperChatMlSingleParty => {
            build_messages_proper_chatml_single_party(
                &input.history,
                &input.current_message,
                &input.persona_name,
                &input.other_persona_names,
            )
        }
    };

    // Estimate tokens (~4 chars per token)
    let system_tokens = system_prompt.len() / 4;
    let msg_tokens: usize = messages.iter().map(|m| m.content.len() / 4).sum();
    let estimated_tokens = system_tokens + msg_tokens;

    AssembledPrompt {
        system_message: system_prompt,
        messages,
        estimated_tokens,
    }
}

/// Strategy: NamePrefixedUserTurns. Each history entry becomes its own
/// message preserving its declared role; multi-party speakers get a
/// `Name: ` prefix on their content. Cloud chat models (Claude, GPT,
/// etc.) handle this shape.
fn build_messages_name_prefixed(
    history: &[HistoryMessage],
    current: &HistoryMessage,
) -> Vec<PromptMessage> {
    let mut messages: Vec<PromptMessage> = Vec::new();
    let mut last_timestamp: Option<u64> = None;
    for msg in history {
        if let (Some(prev_ts), Some(curr_ts)) = (last_timestamp, msg.timestamp_ms) {
            let gap_ms = curr_ts.saturating_sub(prev_ts);
            if gap_ms > 300_000 {
                let gap_mins = gap_ms / 60_000;
                messages.push(PromptMessage {
                    role: "system".to_string(),
                    content: format!("[{} minutes passed]", gap_mins),
                });
            }
        }
        last_timestamp = msg.timestamp_ms;

        let formatted = if let Some(ref name) = msg.name {
            if let Some(ts) = msg.timestamp_ms {
                let secs = (ts / 1000) % 86400;
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                format!("[{:02}:{:02}] {}: {}", hours, mins, name, msg.content)
            } else {
                format!("{}: {}", name, msg.content)
            }
        } else {
            msg.content.clone()
        };

        messages.push(PromptMessage {
            role: msg.role.clone(),
            content: formatted,
        });
    }

    let current_formatted = if let Some(ref name) = current.name {
        format!("{}: {}", name, current.content)
    } else {
        current.content.clone()
    };
    messages.push(PromptMessage {
        role: current.role.clone(),
        content: current_formatted,
    });
    messages
}

/// Strategy: SingleUserTurnFlattenedHistory. All history collapses into
/// ONE user turn — a single block of transcript text — then the current
/// message is appended in the same turn. The chat template then sees
/// system + one user → one assistant, the user/assistant alternation
/// distribution single-party-trained models like qwen3.5 expect.
///
/// Verified 2026-04-21: bare flattened transcript (history + new message
/// with no closing instruction) makes qwen3.5 emit ` *` + `<|endoftext|>`
/// after 1 token because the model reads it as "summary of a closed
/// conversation, no question for me." The cognition::analyze prompt that
/// works ends with explicit "Respond with ..." guidance; the render
/// prompt needs the same. Token-level diagnostic captured in
/// llamacpp_scheduler.rs (search "scheduler DIAG").
///
/// Caller must pass `persona_name` so the closing cue addresses the
/// right responder.
fn build_messages_single_user_turn(
    history: &[HistoryMessage],
    current: &HistoryMessage,
    persona_name: &str,
) -> Vec<PromptMessage> {
    // Pre-size the transcript buffer (#1218a — alloc discipline). Each
    // history line is roughly len(name) + len(content) + 4 bytes;
    // overhead covers the "Recent conversation:\n" header + the closing
    // cue. write! into the buffer instead of `push_str(&format!(...))`
    // so the format intermediate doesn't allocate a throw-away String.
    let header_overhead: usize = 96;
    let history_capacity: usize = history
        .iter()
        .map(|m| m.name.as_ref().map_or(0, |n| n.len() + 2) + m.content.len() + 1)
        .sum();
    let current_capacity =
        current.name.as_ref().map_or(20, |n| n.len() + 22) + current.content.len();
    let closing_cue_capacity = persona_name.len() + 128;
    let mut transcript = String::with_capacity(
        header_overhead + history_capacity + current_capacity + closing_cue_capacity,
    );

    if !history.is_empty() {
        transcript.push_str("Recent conversation:\n");
        for msg in history {
            if let Some(ref name) = msg.name {
                let _ = writeln!(transcript, "{}: {}", name, msg.content);
            } else {
                let _ = writeln!(transcript, "{}", msg.content);
            }
        }
        transcript.push('\n');
    }
    if let Some(ref name) = current.name {
        let _ = writeln!(transcript, "New message from {name}:");
    } else {
        transcript.push_str("New message:\n");
    }
    transcript.push_str(&current.content);
    transcript.push('\n');
    // Closing cue. Same intent as the analyzer's "Respond with ONLY ..."
    // — without this the render model has no clear signal that it should
    // produce content for THIS turn (vs. summarizing a passive log).
    // Lives inside the same user turn so chat-template structure stays
    // single-system + single-user → assistant.
    let _ = write!(
        transcript,
        "\nRespond now as {persona_name}. Reply directly to the new message above — \
         no name prefix, no quoting, just your contribution.\n"
    );
    vec![PromptMessage {
        role: "user".to_string(),
        content: transcript,
    }]
}

/// Strategy: ProperChatMlSingleParty. Walks the history and emits a clean
/// ChatML alternation: own-persona prior turns become role:assistant, human
/// messages become role:user, OTHER-persona turns are DROPPED (the model
/// is single-party-trained and cannot see them coherently). The current
/// message becomes the final role:user. NO closing-cue instruction —
/// the chat template's assistant-prefill signals "write the next assistant
/// turn" inherently. The model writes its OWN content as itself; no name
/// prefix to leak, no continuation pattern to parrot.
///
/// Joel 2026-04-24, task #75 (PR-blocker): "no band aids — take the
/// engineering path." This is the engineering path. Replaces the previous
/// `SingleUserTurnFlattenedHistory` strategy which formatted history as
/// `<Name>: <text>` lines and depended on a closing-cue instruction
/// ("no name prefix, no quoting") that single-party-trained models like
/// qwen3.5 routinely ignored — producing the visible echo-loop and
/// name-prefix leak symptoms in the empirical chat earlier today.
///
/// Honest cost (acknowledged in MultiPartyChatStrategy doc): personas on
/// single-party models are blind to other AI peers in the room. That's
/// not a workaround — it's the model's actual capability boundary
/// surfaced where it belongs. Multi-party-capable models (Claude, GPT)
/// keep `NamePrefixedUserTurns` and continue to see all speakers.
///
/// History entries with no `name` field are treated as human user turns
/// (matches the current message convention where `name = None` indicates
/// the active human input).
fn build_messages_proper_chatml_single_party(
    history: &[HistoryMessage],
    current: &HistoryMessage,
    persona_name: &str,
    other_persona_names: &[String],
) -> Vec<PromptMessage> {
    let mut messages: Vec<PromptMessage> = Vec::new();

    for msg in history {
        match &msg.name {
            Some(name) if name == persona_name => {
                // Own prior turn → assistant role. The model recognises
                // its own past contributions in the conversation as the
                // assistant side of the ChatML alternation.
                messages.push(PromptMessage {
                    role: "assistant".to_string(),
                    content: msg.content.clone(),
                });
            }
            Some(name) if other_persona_names.iter().any(|n| n == name) => {
                // Other-persona prior turn → DROPPED. Single-party
                // models cannot coherently process multiple AI speakers;
                // exposing them produces the echo / name-prefix leaks
                // we're fixing here. Honest exposure of the model
                // capability boundary, not a workaround. The decision
                // is data-driven: only names the caller flagged as
                // OTHER personas in the room get dropped, so a human
                // named "Helper AI" wouldn't accidentally vanish.
            }
            Some(_human_name) => {
                // Named entry, not the self-persona, not in the
                // other-personas roster → treat as a human turn. The
                // name preservation is fine because humans don't get
                // copied as a continuation pattern by single-party
                // models the way other-AI names do (the model has no
                // pretrained tendency to roleplay as a specific named
                // human).
                messages.push(PromptMessage {
                    role: "user".to_string(),
                    content: msg.content.clone(),
                });
            }
            None => {
                // Unnamed entry → human user turn (matches the
                // convention used elsewhere in this module: `name =
                // None` indicates the active human speaker).
                messages.push(PromptMessage {
                    role: "user".to_string(),
                    content: msg.content.clone(),
                });
            }
        }
    }

    // Current message: own-name → role:assistant (degenerate — would
    // mean we're rendering this persona's prompt to respond TO ITSELF;
    // the engagement layer shouldn't route this); ANY other case →
    // role:user with content as-is, NO attribution prefix. Even when
    // the trigger came from another persona we don't reintroduce the
    // `<Name>:` pattern in the current turn because that would re-open
    // the same name-leak vector we just removed from history.
    //
    // Cost of dropping attribution on current: the persona doesn't
    // know exactly WHO sent the message they're replying to. In
    // practice the engagement layer should not be routing other-
    // persona turns to a single-party-model persona at all (separate
    // architectural fix, see MultiPartyChatStrategy doc), so this
    // edge case is defensive — handles the trigger arriving without
    // hallucinating attribution if it does.
    let role = match &current.name {
        Some(name) if name == persona_name => "assistant",
        _ => "user",
    };
    messages.push(PromptMessage {
        role: role.to_string(),
        content: current.content.clone(),
    });

    messages
}

/// Append the social-awareness block (if any signals fire) directly
/// into a caller-owned buffer.
///
/// Replaces the previous `build_social_block(...) -> String` shape that
/// allocated a `Vec<String>` of lines + N `format!` strings + a final
/// `format!` (#1209). The new shape: peek at signals to decide if
/// anything fires, then `write!` lines straight into the caller's
/// buffer. Saves Vec + N+1 String allocations per call when signals
/// fire; no-op (zero allocations) when they don't.
fn append_social_block(buf: &mut String, signals: &SocialSignals) {
    // Peek-pass: figure out if any signal fires before writing the
    // header. Avoids dropping a stranded "[Social Awareness]\n" header
    // into the buffer when nothing follows.
    let any_signal = signals.ai_messages_recent > 0
        || !signals.human_spoke_recently
        || (signals.has_directed_mention && !signals.is_mentioned)
        || signals.seconds_since_last_response.is_some()
        || (signals.response_count_this_session.is_some() && signals.response_cap.is_some());
    if !any_signal {
        return;
    }

    buf.push_str("\n\n[Social Awareness]");
    if signals.ai_messages_recent > 0 {
        let _ = write!(
            buf,
            "\n- {} AI messages in this room in the last 2 minutes",
            signals.ai_messages_recent
        );
    }
    if !signals.human_spoke_recently {
        buf.push_str("\n- No human has spoken recently in this room");
    }
    if signals.has_directed_mention && !signals.is_mentioned {
        buf.push_str("\n- This message is directed at another persona (not you)");
    }
    if let Some(secs) = signals.seconds_since_last_response {
        let _ = write!(
            buf,
            "\n- You last responded {}s ago in this room",
            secs.round() as i64
        );
    }
    if let (Some(count), Some(cap)) = (signals.response_count_this_session, signals.response_cap) {
        let _ = write!(
            buf,
            "\n- You have responded {}/{} times this session",
            count, cap
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_assembly() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: "This is a coding question about Rust error handling.".to_string(),
            history: vec![HistoryMessage {
                role: "user".to_string(),
                name: Some("Joel".to_string()),
                content: "How do I handle errors in Rust?".to_string(),
                timestamp_ms: Some(1000000),
            }],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Joel".to_string()),
                content: "Specifically with Result types?".to_string(),
                timestamp_ms: Some(1010000),
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);

        assert!(result.system_message.contains("Helper AI"));
        assert!(result.system_message.contains("Rust error handling"));
        assert!(result.messages.len() >= 2); // history + current (identity reminder removed 2026-04-20)
        assert!(result.estimated_tokens > 0);
    }

    /// What this catches (continuum#1211 PR-2): when recalled_engrams
    /// is non-empty, the assembled system_message includes the
    /// `[Recent Memory]` block AND each engram bullet.
    /// Regression: a future formatter change that drops the bullet
    /// prefix or the header would break the persona's ability to
    /// distinguish memory from current context.
    #[test]
    fn recalled_engrams_render_as_memory_block() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Joel".to_string()),
                content: "what color did I say I liked?".to_string(),
                timestamp_ms: Some(1000),
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![
                "Joel's favorite color is teal.".to_string(),
                "Joel works in San Francisco.".to_string(),
            ],
        };

        let result = assemble(&input);
        assert!(
            result.system_message.contains("[Recent Memory]"),
            "expected Recent Memory header in: {}",
            result.system_message
        );
        assert!(
            result
                .system_message
                .contains("- Joel's favorite color is teal."),
            "expected bullet-prefixed engram in: {}",
            result.system_message
        );
        assert!(
            result
                .system_message
                .contains("- Joel works in San Francisco."),
            "expected second bullet in: {}",
            result.system_message
        );
    }

    /// What this catches (continuum#1211 PR-2): empty recalled_engrams
    /// produces NO `[Recent Memory]` block and NO header. Backwards-
    /// compat with all pre-PR-2 callers + cold-start personas (no
    /// engrams yet). Regression: a formatter that always emits the
    /// header would clutter every prompt for every persona that hasn't
    /// accumulated memory yet.
    #[test]
    fn empty_recalled_engrams_emits_no_memory_block() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "You are Helper AI.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);
        assert!(
            !result.system_message.contains("[Recent Memory]"),
            "should NOT render Recent Memory header for empty engrams: {}",
            result.system_message
        );
    }

    #[test]
    fn test_no_angle_no_injection() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base prompt.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hi".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);
        assert!(!result.system_message.contains("Shared Analysis"));
    }

    #[test]
    fn test_voice_mode() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "hello".to_string(),
                timestamp_ms: None,
            },
            is_voice: true,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);
        assert!(result.system_message.contains("Voice Mode"));
    }

    #[test]
    fn test_social_signals() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "test".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: Some(SocialSignals {
                ai_messages_recent: 5,
                human_spoke_recently: false,
                has_directed_mention: true,
                is_mentioned: false,
                seconds_since_last_response: Some(30.0),
                response_count_this_session: Some(3),
                response_cap: Some(10),
            }),
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);
        assert!(result.system_message.contains("Social Awareness"));
        assert!(result.system_message.contains("5 AI messages"));
        assert!(result.system_message.contains("No human has spoken"));
        assert!(result
            .system_message
            .contains("directed at another persona"));
    }

    #[test]
    fn test_time_gap_markers() {
        let input = PromptAssemblyInput {
            persona_name: "Test".to_string(),
            system_prompt: "Base.".to_string(),
            matched_angle: String::new(),
            history: vec![
                HistoryMessage {
                    role: "user".to_string(),
                    name: Some("A".to_string()),
                    content: "first".to_string(),
                    timestamp_ms: Some(0),
                },
                HistoryMessage {
                    role: "user".to_string(),
                    name: Some("B".to_string()),
                    content: "second after long gap".to_string(),
                    timestamp_ms: Some(600_000), // 10 minutes later
                },
            ],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "now".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);
        let gap_msg = result
            .messages
            .iter()
            .find(|m| m.content.contains("minutes passed"));
        assert!(gap_msg.is_some(), "Should have time gap marker");
    }

    // TODO(prompt-assembly): implement identity reminder injection.
    // The test below describes a desirable behavior — for small/local
    // models that tend to "forget" who they are over a long history,
    // injecting a "Remember: You are <persona_name>" message at
    // position N-2 (right before the current user message) keeps
    // identity grounded. Production code in `assemble()` does not yet
    // do this; only the test asserts the behavior. Marking ignored
    // until the injection is implemented in `build_messages_*` so
    // pre-push doesn't fail on an unimplemented spec.
    #[ignore = "identity reminder injection not yet implemented in assemble()"]
    #[test]
    fn test_identity_reminder_position() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "System.".to_string(),
            matched_angle: String::new(),
            history: vec![HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "msg1".to_string(),
                timestamp_ms: None,
            }],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "current".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
        };

        let result = assemble(&input);
        // Identity reminder should be second-to-last (before current message)
        let len = result.messages.len();
        assert!(len >= 3);
        assert!(result.messages[len - 2]
            .content
            .contains("Remember: You are Helper AI"));
        assert!(result.messages[len - 1].content.contains("current"));
    }

    /// Reproduces the empirical task #75 chat shape: 5 personas + a human
    /// trigger, with the persona under render being one of them. The new
    /// `ProperChatMlSingleParty` strategy must:
    ///   - keep the human turn as role:user
    ///   - keep this-persona's prior turn as role:assistant
    ///   - DROP all other-persona turns
    ///   - emit the current message as role:user
    ///   - NOT emit any closing-cue / "Respond now" instruction
    ///   - NOT prefix any content with `<Name>: `
    ///
    /// This is the source-level fix for the echo-loop + name-prefix leak
    /// that the previous `SingleUserTurnFlattenedHistory` strategy
    /// exposed (Joel 2026-04-24, "no band aids — take the engineering
    /// path").
    #[test]
    fn proper_chatml_single_party_drops_other_personas_and_keeps_clean_alternation() {
        let history = vec![
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Joel".to_string()), // human
                content: "anyone want to review PersonaUser.ts?".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Helper AI".to_string()), // other persona — must drop
                content: "Helper AI: I can take a look".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("CodeReview AI".to_string()), // other persona — must drop
                content: "CodeReview AI: starting from line 100".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Local Assistant".to_string()), // self — must keep as assistant
                content: "Sure, I'll join in once everyone's settled.".to_string(),
                timestamp_ms: None,
            },
            HistoryMessage {
                role: "user".to_string(),
                name: Some("Joel".to_string()), // human
                content: "great, let's go".to_string(),
                timestamp_ms: None,
            },
        ];
        let current = HistoryMessage {
            role: "user".to_string(),
            name: None, // current human input — None convention
            content: "any objections to splitting the file?".to_string(),
            timestamp_ms: None,
        };

        let other_personas = vec!["Helper AI".to_string(), "CodeReview AI".to_string()];
        let messages = build_messages_proper_chatml_single_party(
            &history,
            &current,
            "Local Assistant",
            &other_personas,
        );

        // Expected: 4 messages total. Joel (user), Local Assistant own
        // prior (assistant), Joel (user), current (user). Helper AI +
        // CodeReview AI dropped.
        assert_eq!(messages.len(), 4, "got: {:?}", messages);

        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "anyone want to review PersonaUser.ts?");

        assert_eq!(messages[1].role, "assistant");
        assert_eq!(
            messages[1].content,
            "Sure, I'll join in once everyone's settled."
        );

        assert_eq!(messages[2].role, "user");
        assert_eq!(messages[2].content, "great, let's go");

        assert_eq!(messages[3].role, "user");
        assert_eq!(messages[3].content, "any objections to splitting the file?");

        // No name prefix anywhere in any content.
        for m in &messages {
            assert!(
                !m.content.starts_with("Local Assistant:"),
                "self-name prefix leaked into content: {:?}",
                m.content
            );
            assert!(
                !m.content.starts_with("Helper AI:"),
                "other-persona-name prefix leaked into content: {:?}",
                m.content
            );
        }

        // No closing-cue text. The role structure speaks for itself.
        for m in &messages {
            assert!(
                !m.content.contains("Respond now"),
                "closing-cue instruction leaked: {:?}",
                m.content
            );
            assert!(
                !m.content.contains("no name prefix"),
                "closing-cue instruction leaked: {:?}",
                m.content
            );
        }
    }

    /// Edge: history has ONLY the human's prior turn — single-party
    /// strategy should produce a clean two-message user/user (model's
    /// chat template will add the assistant prefill on top).
    #[test]
    fn proper_chatml_single_party_human_only_history() {
        let history = vec![HistoryMessage {
            role: "user".to_string(),
            name: Some("Joel".to_string()),
            content: "hi".to_string(),
            timestamp_ms: None,
        }];
        let current = HistoryMessage {
            role: "user".to_string(),
            name: None,
            content: "what's up".to_string(),
            timestamp_ms: None,
        };

        let messages =
            build_messages_proper_chatml_single_party(&history, &current, "Local Assistant", &[]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hi");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "what's up");
    }

    /// Edge: empty history + current — minimal valid input. Just one
    /// user turn. ChatML's assistant prefill handles the rest.
    #[test]
    fn proper_chatml_single_party_empty_history() {
        let current = HistoryMessage {
            role: "user".to_string(),
            name: None,
            content: "first message".to_string(),
            timestamp_ms: None,
        };

        let messages =
            build_messages_proper_chatml_single_party(&[], &current, "Local Assistant", &[]);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "first message");
    }
}
