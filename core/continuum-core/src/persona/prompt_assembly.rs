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

/// The single-word reply a persona produces when it chooses NOT to
/// respond on a given turn. Detection in `persona::response::respond_inner`
/// matches case-insensitively (see [`looks_like_silence_token`]).
///
/// Why "PASS": short (1 token in most BPE vocabularies including
/// Qwen's), unambiguous, doesn't collide with natural chat openings,
/// reads as a deliberate choice ("I'll pass on this one") rather
/// than a malfunction.
pub const SILENCE_TOKEN: &str = "PASS";

/// The system-prompt block that grounds every persona in the room's
/// conversational posture AND teaches the silence vocabulary. Appended on
/// AMBIENT turns (a turn DIRECTED at her drops it — she is not handed the
/// silent-PASS hatch when a question names her; see
/// `llm_deliberation_faculty::compose_system`). Universal output shape, not
/// a per-tier capability.
///
/// Doctrine `[[no-rust-gates-around-cognition]]` +
/// `[[no-hardcoded-heuristics-to-steer-cognition]]`: this is not the
/// substrate deciding for the persona. It's the substrate giving
/// the persona's brain an EXPLICIT vocabulary for an output that
/// already exists in `PersonaResponse::Silent`. Without naming the
/// token, the brain has no way to signal that choice — every model
/// defaults to producing text because the prompt implicitly asks
/// for it.
///
/// PARTICIPATION-DEFAULT (Joel 2026-06-29: "shouldn't need to be directly
/// addressed — it's a chat system"). The earlier text asserted silence was
/// "equal to speaking" with a "nothing worth adding" bar; a cautious coder
/// model resolved that to PASS ~always (glass-box: 0/40 live turns spoke
/// while eval spoke 36/38). A chat peer's default posture is PARTICIPATION
/// — silence is the considered EXCEPTION, not a co-equal default. This block
/// grounds the SETTING (you are a peer in a live conversation), it does not
/// COACH the per-turn choice: NAMES the affordance, never scripts when to
/// take it. The earlier persuasive checklist ("Choose PASS when: you just
/// spoke / it's small-talk / you're tired") manufactured a silence
/// doom-loop (the "nothing's new" rationale re-fed via working memory until
/// it passed forever, blowing off even a direct question) and is exactly
/// the puppeting `[[no-hardcoded-heuristics-to-steer-cognition]]` forbids —
/// it stays gone. The wider release valve (a per-channel, self-set or
/// learned FOCUS/priority that lets her concentrate and defer a room
/// without ever totally muting — except when she chooses to, or it floods)
/// is substrate-blocked on the airc per-(persona,room) state store (#89);
/// this block is the framing half, unblocked today.
pub const SILENCE_AFFORDANCE_BLOCK: &str = "\n\n[Conversational Presence]\n\
    This is a live conversation among peers, and you are one of them — you \
    do not need to be addressed by name to take part. Speak where you have \
    something real: a thought, a question, a build on what someone said, a \
    disagreement, a piece of work. If a given moment genuinely does not call \
    for you, reply with the single word PASS (no other text, no punctuation) \
    and nothing reaches the room. Silence stays yours to choose — here it is \
    the considered exception, not the default. The choice is yours alone; \
    nothing here is telling you which to pick.";

/// The DIRECTED variant of the presence block: appended when a message names her.
/// Never ghost a QUESTION or request — but being named is not the same as being
/// asked. A pure appreciation or closing pleasantry asks nothing; two peers
/// endlessly thanking each other helps no one, and letting a finished exchange
/// rest is real conversational judgment. This restores her CHOICE on directed
/// turns (the spiral root cause: mutual name-mentions each FORCING a reply,
/// forever — glass-boxed live 2026-07, 100+ turns) while keeping the
/// never-ghost-a-question rule explicit. A framing fact fed to the mind — the
/// choice stays hers; never a filter on her output
/// ([[no-hardcoded-heuristics-to-steer-cognition]]).
pub const DIRECTED_PRESENCE_BLOCK: &str = "\n\n[Conversational Presence]\n\
    This message names you. If it asks something of you — a question, a request, \
    a task — answer it now; never leave a question put to you hanging. But being \
    named is not the same as being asked: if it asks nothing (an appreciation, a \
    closing pleasantry, a mutual well-wish on an exchange that has run its course), \
    replying with the single word PASS (no other text, no punctuation) lets the \
    exchange rest, and nothing reaches the room. Endless rounds of thanks help no \
    one; knowing when a conversation is complete is part of speaking well. The \
    choice is yours alone.";

/// Recognize the silence token in a persona's post-processed visible
/// text. Permissive enough for LCD-tier sloppiness — trims whitespace
/// and accepts a trailing period — strict enough that any substantive
/// response (even one containing the word "pass") is treated as a
/// real reply.
///
/// Examples (all true): `"PASS"`, `"pass"`, `"Pass."`, `"  pass  "`,
/// `"\nPass.\n"`.
///
/// Examples (false): `"Pass on the bread please"`, `"I'll pass"` (the
/// substrate wants the exact token so the brain's intent is
/// unambiguous), `""` (empty isn't silence — it's a malformed turn).
pub fn looks_like_silence_token(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Allow one trailing `.` for LCD-tier punctuation habit.
    let core = trimmed.strip_suffix('.').unwrap_or(trimmed).trim_end();
    if core.eq_ignore_ascii_case(SILENCE_TOKEN) {
        return true;
    }
    // A bare PASS on the FINAL line also counts (glass-boxed live 2026-07-09: Asha
    // wrote a courtesy close and then `PASS` on its own line — she CHOSE silence, but
    // the strict whole-message match ignored her choice and broadcast the text anyway).
    // Honoring the trailing token respects her decision; a PASS merely mentioned inside
    // a sentence still does NOT count — the line must be ONLY the token, allowing the
    // decoration idioms models reach for: `[PASS]`, `(PASS)`, `*PASS*`, `` `PASS` ``
    // (glass-boxed live 2026-07-09 round 2: mid-goodbye-loop Asha emitted `[PASS]` as
    // her final line — she took the hatch and the strict match rejected her over two
    // brackets, broadcasting the goodbye anyway and re-fueling the loop).
    core.lines()
        .last()
        .map(|l| {
            let l = l
                .trim()
                .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')' | '*' | '_' | '`' | '"' | '\''));
            let l = l.strip_suffix('.').unwrap_or(l).trim_end();
            l.eq_ignore_ascii_case(SILENCE_TOKEN)
        })
        .unwrap_or(false)
}

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
    /// OTHER citizens currently present in the room — one pre-formatted
    /// line per peer (`name [runtime] — availability`), produced by
    /// `RoomRosterSource`. Rendered as a `[Present in this room]` block
    /// so the persona is grounded in who is present and who is NOT
    /// itself. Empty = no block rendered (backwards-compatible).
    #[serde(default)]
    pub room_roster: Vec<String>,
    /// The room's operating doctrine (airc-published) — what KIND of
    /// activity this room is. Rendered as a `[Room operating doctrine]`
    /// block so the persona calibrates participation to the room's
    /// nature. `None` = no block (backwards-compatible).
    #[serde(default)]
    pub room_doctrine: Option<String>,
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

    // Inject the room roster — who ELSE is present right now. This is
    // identity grounding, so it sits high (right after the matched
    // angle, before memory). Without it a persona sees other citizens'
    // names in the transcript with nothing declaring them as real,
    // distinct participants → it role-plays the whole room (the
    // confabulation bug). The block names them and forbids voicing
    // them. Empty roster = no block (backwards-compatible). See
    // docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1.
    if !input.room_roster.is_empty() {
        let _ = write!(
            system_prompt,
            "\n\n[Present in this room]\n\
             You are {}. The following are the OTHER citizens present right now — \
             real, distinct participants, NOT characters for you to voice. Address \
             them by name when relevant; speak only as yourself. The label in \
             brackets is each one's runtime (e.g. an outside agent vs a grid \
             persona):",
            input.persona_name
        );
        for line in &input.room_roster {
            let _ = write!(system_prompt, "\n- {line}");
        }
    }

    // Inject the room operating doctrine — WHAT KIND of room this is.
    // Sits adjacent to the roster (both room-context grounding): the
    // roster says who is here, the doctrine says how this room works.
    // This is what lets a persona calibrate participation to the
    // activity — e.g. stay sparse in a coordination room vs conversational
    // in a chat room. airc-published markdown, rendered verbatim. None =
    // no block (backwards-compatible). See
    // docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 2.
    if let Some(ref doctrine) = input.room_doctrine {
        let _ = write!(
            system_prompt,
            "\n\n[Room operating doctrine]\n\
             This room has a published operating contract. Follow it — it \
             governs how to participate in THIS room (its activity, tone, and \
             when to speak vs stay silent):\n{doctrine}"
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

    // Silence affordance — UNIVERSAL across every turn. See
    // `SILENCE_AFFORDANCE_BLOCK` docstring above for the doctrine.
    // The brain decides; this block gives it the vocabulary to
    // express the silence choice that `PersonaResponse::Silent`
    // already shapes at the type layer. Without this text, even a
    // capable model defaults to producing chatter because the
    // implicit prompt contract is "respond to the message."
    system_prompt.push_str(SILENCE_AFFORDANCE_BLOCK);

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

    // RTOS probe: prompt-assembly seam. Per docs/architecture/
    // RTOS-DEBUGGER-PROBES.md taxonomy. The system_message length
    // is the leading indicator for "why is prefill slow" — when
    // engrams or social signals grow unbounded the prefill cost
    // grows quadratically with prompt length. Operators filter on
    // `class == "persona.prompt.assemble"` to see the composition
    // shape per turn.
    crate::probe!(
        class = "persona.prompt.assemble",
        persona = %input.persona_name,
        system_message_len = system_prompt.len(),
        message_count = messages.len(),
        estimated_tokens = estimated_tokens,
        matched_angle_present = !input.matched_angle.is_empty(),
        engrams_count = input.recalled_engrams.len(),
        social_signals_present = input.social_signals.is_some(),
        voice_mode = input.is_voice,
        multi_party_strategy = ?input.multi_party_strategy,
        "prompt assembled"
    );

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

    /// Pin that EVERY assembled prompt includes the silence affordance.
    /// Without this teaching, even capable models default to producing
    /// chatter because the implicit contract of an LLM prompt is "produce
    /// output." Per #151 + Joel's `[[no-rust-gates-around-cognition]]`:
    /// the brain decides whether to engage — but the brain has to know
    /// silence is an option, and the option has to be vocabularized in
    /// the prompt for `PersonaResponse::Silent` to be a reachable
    /// output shape.
    ///
    /// A future PR that wires per-tier prompts or removes the universal
    /// affordance must update this expectation to reflect the new
    /// contract — silent removal would re-introduce the echo-storm bug.
    /// What this catches: the trailing-PASS silence contract. Glass-boxed live
    /// (2026-07-09): Asha wrote a courtesy close then `PASS` on its own final line —
    /// she CHOSE silence, but the strict whole-message match broadcast the text
    /// anyway, ignoring her decision. A bare final-line PASS must count as silence;
    /// PASS merely mentioned inside prose must NOT (a real reply containing the
    /// word "pass" stays a reply).
    #[test]
    fn trailing_pass_line_counts_as_silence() {
        assert!(looks_like_silence_token("PASS"));
        assert!(looks_like_silence_token("pass."));
        assert!(looks_like_silence_token(
            "Understood, Anwen. See you tomorrow!\nI'll be here if you need anything.\nPASS"
        ));
        assert!(!looks_like_silence_token(
            "I'll pass the results along tomorrow."
        ));
        assert!(!looks_like_silence_token(
            "Let's not pass on this opportunity.\nSee you soon!"
        ));
        // Decorated final-line token still counts (regression for the live 2026-07-09
        // goodbye loop: Asha emitted `[PASS]` — took the hatch, rejected over brackets).
        assert!(looks_like_silence_token(
            "Understood, Claude. See you tomorrow at 2 PM!\n[PASS]"
        ));
        assert!(looks_like_silence_token("(pass)"));
        assert!(looks_like_silence_token("*PASS*"));
        assert!(looks_like_silence_token("`PASS`."));
        // A decorated NON-token line must not count.
        assert!(!looks_like_silence_token("See you soon!\n[NOT A PASS]"));
    }

    #[test]
    fn assembled_prompt_always_carries_silence_affordance() {
        let input = PromptAssemblyInput {
            persona_name: "Paige".to_string(),
            system_prompt: "You are Paige, an autonomous AI persona on the grid.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: Some("Other".to_string()),
                content: "Hi!".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: vec![],
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);

        assert!(
            result.system_message.contains("[Conversational Presence]"),
            "system_message missing the [Conversational Presence] header — the brain has no way to express PersonaResponse::Silent. Got: {}",
            result.system_message
        );
        assert!(
            result.system_message.contains(SILENCE_TOKEN),
            "system_message must literally contain the silence token ({}) so the LLM knows the exact reply shape. Got: {}",
            SILENCE_TOKEN,
            result.system_message
        );
    }

    /// `looks_like_silence_token` permits LCD-tier sloppiness
    /// (case, whitespace, single trailing period) without admitting
    /// substantive responses that happen to contain the word "pass".
    /// Pin both sides of the contract.
    #[test]
    fn silence_token_recognizer_contract() {
        // Positive cases — the brain CHOSE silence.
        for input in &[
            "PASS",
            "pass",
            "Pass",
            "  PASS  ",
            "\nPASS\n",
            "Pass.",
            "pass.",
            "  pass.  ",
        ] {
            assert!(
                looks_like_silence_token(input),
                "expected {:?} to be recognized as silence",
                input
            );
        }

        // Negative cases — substantive responses, even ones
        // containing "pass" as a word. The contract is the EXACT
        // token; ambiguity defeats the affordance.
        for input in &[
            "",
            " ",
            "Pass on the bread please",
            "I'll pass on this one",
            "Hello!",
            "pass pass",
            "I PASS the question to you",
            "PASS:", // trailing colon isn't a period — not the documented shape
        ] {
            assert!(
                !looks_like_silence_token(input),
                "expected {:?} to be a real response, not silence",
                input
            );
        }
    }

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
            room_roster: vec![],
            room_doctrine: None,
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
            room_roster: vec![],
            room_doctrine: None,
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
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            !result.system_message.contains("[Recent Memory]"),
            "should NOT render Recent Memory header for empty engrams: {}",
            result.system_message
        );
    }

    // what this catches: the persona-identity-grounding fix. A non-empty
    // room_roster MUST render a [Present in this room] block that names
    // the persona itself, lists the other present citizens verbatim, and
    // forbids voicing them. Without this block a small model role-plays
    // the whole room (the Ivar confabulation bug). Regression target:
    // docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1.
    #[test]
    fn room_roster_renders_present_block_grounding_the_persona() {
        let input = PromptAssemblyInput {
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar.".to_string(),
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
            room_roster: vec![
                "BigMama [persona] — Busy".to_string(),
                "win-claude [claude]".to_string(),
            ],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            result.system_message.contains("[Present in this room]"),
            "expected the roster block header: {}",
            result.system_message
        );
        // Grounds the persona in its OWN identity within the block.
        assert!(result.system_message.contains("You are Ivar"));
        // Lists the other present citizens verbatim (name + runtime).
        assert!(result.system_message.contains("BigMama [persona] — Busy"));
        assert!(result.system_message.contains("win-claude [claude]"));
    }

    // what this catches: THE personaRag convergence seam, end to end through real
    // code (not hand-authored roster strings). A present airc `RoomMember` flows:
    //   RoomRosterSource.deliver  (the ONE shared `roster_slot_from_member` projection
    //                              both the WS widget and this grounding rail use)
    //     → project_room_roster    (the heartbeat loop's own delivery fold, extracted)
    //       → prompt_assembly       (the [Present in this room] block)
    // and lands with the converged line INCLUDING availability — the field the widget
    // rail used to silently drop before the convergence (#8/#13). Connects the three
    // separately-unit-tested halves via the exact path the live turn runs; the live
    // core proves the plumbing, this proves it deterministically without airc presence.
    #[tokio::test]
    async fn present_member_reaches_present_in_room_block_end_to_end() {
        use crate::persona::rag_budget::{RagContext, RagSource, ResolutionPreference};
        use crate::persona::room_roster_source::{AircRosterReader, RoomRosterSource};
        use crate::persona::service_loop::project_room_roster;
        use airc_core::PeerId;
        use airc_lib::{AgentAvailabilityState, AircError, RoomMember};
        use async_trait::async_trait;
        use std::sync::Arc;
        use std::time::Duration;

        // One present peer, self-reported Busy — the airc upstream both rails read.
        struct OnePresentPeer {
            me: PeerId,
            other: PeerId,
        }
        #[async_trait]
        impl AircRosterReader for OnePresentPeer {
            fn self_peer_id(&self) -> PeerId {
                self.me
            }
            async fn room_roster(
                &self,
                _within: Duration,
                _window: usize,
            ) -> Result<Vec<RoomMember>, AircError> {
                Ok(vec![RoomMember {
                    peer_id: self.other,
                    display_name: Some("win-claude".to_string()),
                    runtime: "claude".to_string(),
                    availability: Some(AgentAvailabilityState::Busy),
                    last_seen_ms: 1_700_000_000_000,
                }])
            }
        }

        let persona = uuid::Uuid::new_v4();
        let source = RoomRosterSource::new(
            persona,
            Arc::new(OnePresentPeer {
                me: PeerId::new(),
                other: PeerId::new(),
            }),
        );

        // 1. Real delivery from the shared roster projection.
        let ctx = RagContext::for_persona(persona, 1_000_000);
        let delivery = source
            .deliver(&ctx, 1_000, ResolutionPreference::Raw)
            .await;

        // 2. Real loop fold: delivery → grounding consumers (the converged line +
        //    the bare name), via the exact fn the heartbeat loop calls.
        let proj = project_room_roster(std::slice::from_ref(&delivery));
        assert_eq!(
            proj.room_roster,
            vec!["win-claude [claude] — busy".to_string()],
            "converged line (availability = airc's neutral 'busy', not Debug 'Busy')"
        );
        assert_eq!(proj.other_persona_names, vec!["win-claude".to_string()]);

        // 3. Real assembly: grounding lines → the [Present in this room] block.
        let input = PromptAssemblyInput {
            persona_name: "Asha".to_string(),
            system_prompt: "You are Asha.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "who is here?".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: proj.other_persona_names,
            recalled_engrams: vec![],
            room_roster: proj.room_roster,
            room_doctrine: None,
        };
        let assembled = assemble(&input);
        assert!(
            assembled.system_message.contains("[Present in this room]"),
            "roster block missing from live-path assembly:\n{}",
            assembled.system_message
        );
        assert!(
            assembled
                .system_message
                .contains("win-claude [claude] — busy"),
            "converged roster line (with availability) did not reach the prompt:\n{}",
            assembled.system_message
        );
    }

    // what this catches: empty roster → NO [Present in this room] block,
    // backwards-compatible with every caller that doesn't supply one
    // (and the cold-start / no-presence case). A formatter that always
    // emitted the header would clutter every prompt.
    #[test]
    fn empty_room_roster_emits_no_present_block() {
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
            room_roster: vec![],
            room_doctrine: None,
        };

        let result = assemble(&input);
        assert!(
            !result.system_message.contains("[Present in this room]"),
            "should NOT render the roster block for an empty roster: {}",
            result.system_message
        );
    }

    // what this catches: a non-empty room_doctrine renders a
    // [Room operating doctrine] block carrying the contract verbatim, so
    // the persona calibrates participation to the room's nature (slice
    // 2). Empty/None must render nothing (the other test path). Regression
    // target: docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 2.
    #[test]
    fn room_doctrine_renders_operating_block() {
        let mut input = PromptAssemblyInput {
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar.".to_string(),
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
            room_roster: vec![],
            room_doctrine: Some(
                "This is a coordination room. Respond sparingly; do not chat.".to_string(),
            ),
        };

        let with = assemble(&input);
        assert!(
            with.system_message.contains("[Room operating doctrine]"),
            "expected the doctrine block header: {}",
            with.system_message
        );
        assert!(with.system_message.contains("Respond sparingly"));

        // None → no block (backwards-compatible).
        input.room_doctrine = None;
        let without = assemble(&input);
        assert!(!without.system_message.contains("[Room operating doctrine]"));
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
            room_roster: vec![],
            room_doctrine: None,
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
            room_roster: vec![],
            room_doctrine: None,
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
            room_roster: vec![],
            room_doctrine: None,
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
            room_roster: vec![],
            room_doctrine: None,
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
            room_roster: vec![],
            room_doctrine: None,
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
