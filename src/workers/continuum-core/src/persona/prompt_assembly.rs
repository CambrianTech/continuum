//! Prompt Assembly — builds the final LLM message array from RAG context
//!
//! Port of PersonaPromptAssembler.ts to Rust. Zero TS logic remains
//! in the prompt construction path after this module ships.
//!
//! Input: PromptAssemblyInput (persona identity, RAG context, shared analysis angle)
//! Output: AssembledPrompt (system message + conversation history, ready for ai/generate)

use serde::{Deserialize, Serialize};

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
}

/// A message in conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessage {
    pub role: String,       // "system" | "user" | "assistant"
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
    let mut system_prompt = input.system_prompt.clone();

    // Inject shared analysis angle if present — grounds the persona's
    // contribution in the specific perspective the orchestrator matched.
    if !input.matched_angle.is_empty() {
        system_prompt.push_str(&format!(
            "\n\n[Shared Analysis — Your Angle]\n\
             The following aspect of this conversation is specifically relevant \
             to your expertise. Focus your contribution here:\n{}",
            input.matched_angle
        ));
    }

    // Inject social awareness signals
    if let Some(ref signals) = input.social_signals {
        let social_block = build_social_block(signals);
        if !social_block.is_empty() {
            system_prompt.push_str(&social_block);
        }
    }

    // Voice mode instructions
    if input.is_voice {
        system_prompt.push_str(
            "\n\n[Voice Mode]\n\
             You are in a live voice conversation. Keep responses concise and \
             conversational — the user is listening, not reading. Avoid markdown, \
             code blocks, or long lists. Speak naturally."
        );
    }

    // Build message array
    let mut messages: Vec<PromptMessage> = Vec::new();

    // Add conversation history with time gaps
    let mut last_timestamp: Option<u64> = None;
    for msg in &input.history {
        // Insert time gap marker if >5 minutes between messages
        if let (Some(prev_ts), Some(curr_ts)) = (last_timestamp, msg.timestamp_ms) {
            let gap_ms = curr_ts.saturating_sub(prev_ts);
            if gap_ms > 300_000 {
                // >5 min gap
                let gap_mins = gap_ms / 60_000;
                messages.push(PromptMessage {
                    role: "system".to_string(),
                    content: format!("[{} minutes passed]", gap_mins),
                });
            }
        }
        last_timestamp = msg.timestamp_ms;

        // Format: "[HH:MM] Name: content" for multi-party awareness
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

    // Identity reminder at end (recency bias — model pays most attention to recent tokens).
    //
    // Silence is NOT mentioned here. Whether to speak is decided upstream by
    // score_persona() in the orchestrator; by the time we're assembling a
    // prompt the decision is "this persona will respond." Telling the model
    // about silence-as-an-option leaks into text (e.g. qwen3.5-4b with
    // enable_thinking=false literally outputs "stay silent" or "[stay silent]"
    // as its response). The render model's job is to produce the contribution,
    // not second-guess the participation decision.
    messages.push(PromptMessage {
        role: "system".to_string(),
        content: format!(
            "Remember: You are {}. Respond as yourself — no name prefix, \
             no speaking for others. Contribute the perspective your specialty \
             adds to this conversation.",
            input.persona_name
        ),
    });

    // Current message
    let current_formatted = if let Some(ref name) = input.current_message.name {
        format!("{}: {}", name, input.current_message.content)
    } else {
        input.current_message.content.clone()
    };
    messages.push(PromptMessage {
        role: input.current_message.role.clone(),
        content: current_formatted,
    });

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

/// Build social awareness block from signals.
fn build_social_block(signals: &SocialSignals) -> String {
    let mut lines = Vec::new();

    if signals.ai_messages_recent > 0 {
        lines.push(format!(
            "- {} AI messages in this room in the last 2 minutes",
            signals.ai_messages_recent
        ));
    }
    if !signals.human_spoke_recently {
        lines.push("- No human has spoken recently in this room".to_string());
    }
    if signals.has_directed_mention && !signals.is_mentioned {
        lines.push("- This message is directed at another persona (not you)".to_string());
    }
    if let Some(secs) = signals.seconds_since_last_response {
        lines.push(format!("- You last responded {}s ago in this room", secs.round() as i64));
    }
    if let (Some(count), Some(cap)) = (signals.response_count_this_session, signals.response_cap) {
        lines.push(format!("- You have responded {}/{} times this session", count, cap));
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("\n\n[Social Awareness]\n{}", lines.join("\n"))
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
            history: vec![
                HistoryMessage {
                    role: "user".to_string(),
                    name: Some("Joel".to_string()),
                    content: "How do I handle errors in Rust?".to_string(),
                    timestamp_ms: Some(1000000),
                },
            ],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Joel".to_string()),
                content: "Specifically with Result types?".to_string(),
                timestamp_ms: Some(1010000),
            },
            is_voice: false,
            social_signals: None,
        };

        let result = assemble(&input);

        assert!(result.system_message.contains("Helper AI"));
        assert!(result.system_message.contains("Rust error handling"));
        assert!(result.messages.len() >= 3); // history + identity reminder + current
        assert!(result.estimated_tokens > 0);
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
        };

        let result = assemble(&input);
        assert!(result.system_message.contains("Social Awareness"));
        assert!(result.system_message.contains("5 AI messages"));
        assert!(result.system_message.contains("No human has spoken"));
        assert!(result.system_message.contains("directed at another persona"));
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
        };

        let result = assemble(&input);
        let gap_msg = result.messages.iter().find(|m| m.content.contains("minutes passed"));
        assert!(gap_msg.is_some(), "Should have time gap marker");
    }

    #[test]
    fn test_identity_reminder_position() {
        let input = PromptAssemblyInput {
            persona_name: "Helper AI".to_string(),
            system_prompt: "System.".to_string(),
            matched_angle: String::new(),
            history: vec![
                HistoryMessage {
                    role: "user".to_string(),
                    name: None,
                    content: "msg1".to_string(),
                    timestamp_ms: None,
                },
            ],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "current".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
        };

        let result = assemble(&input);
        // Identity reminder should be second-to-last (before current message)
        let len = result.messages.len();
        assert!(len >= 3);
        assert!(result.messages[len - 2].content.contains("Remember: You are Helper AI"));
        assert!(result.messages[len - 1].content.contains("current"));
    }
}
