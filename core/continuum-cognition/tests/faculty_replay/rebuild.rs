//! A captured act's request turned back into what the faculty perceives: the workspace (her
//! room's turns, the grounding blocks, the clock, the contracts in force) and her working memory
//! (her last move and its result). The faculty then builds her prompt from them on its own path.
//!
//! The captured messages split at the live framing (`[now …]` / `[In force this turn: …]`, the
//! first message of the act's tail, `deliberation_prompt::compose_live_framing`):
//!
//! - before it, her view as sent: a `[<faculty>]\n<body>` message is a grounding block's unit
//!   (`context_streams/blocks.rs` render), every message of one tag one trailing contribution
//!   whose units are keyed by their content; her own act renderings (`[action …`, `[result …`)
//!   are skipped, since her working memory renders them again; native protocol and anything an
//!   earlier captured tail sent are skipped, since the faculty's own sent tails hold them; every
//!   other message is one turn of her room, appended once, in order;
//! - from it on, this act's tail: the ask (a peer's text) is the newest turn, the working context
//!   (`[What you are working with right now]`) one non-trailing contribution, her words and calls
//!   her action statement, a new result her newest act (`WorkingMemory::record_receipt`).

use std::collections::HashSet;
use std::sync::Arc;

use continuum_cognition::ai::types::{ChatMessage, ContentPart, MessageContent};
use continuum_cognition::cognition::deliberation_prompt::OWN_TIME_BLOCK;
use continuum_cognition::cognition::prompt_assembly::{DIRECTED_PRESENCE_BLOCK, SILENCE_AFFORDANCE_BLOCK, WORKING_PRESENCE_BLOCK};
use continuum_cognition::cognition::rag_budget::RagItem;
use continuum_cognition::cognition::working_memory::WorkingMemory;
use continuum_cognition::cognition::workspace::{Burst, BurstTurn, Cause, Contribution, FacultyId, TurnSource, TurnVoice, Workspace};
use continuum_cognition::identity::ActivityRoom;
use uuid::Uuid;

use super::decode::CapturedAct;
use super::wire;

/// The header her working context opens with (`deliberation_prompt::WORKING_CONTEXT_HEADER`).
const WORKING_CONTEXT: &str = "[What you are working with right now]";
/// A legacy newest result (`WorkingMemory::full_result_message`).
const LEGACY_RESULT: &str = "Full result of your most recent action";

/// One resident's room as the replay has seen it so far: her room's turns (append-only, as a
/// burst's history is), what earlier captured tails sent, and her last recorded act.
#[derive(Default)]
pub struct Room {
    turns: Vec<BurstTurn>,
    seen: HashSet<String>,
    sent_tails: HashSet<String>,
    last_act: Option<String>,
    last_statement: Option<String>,
}

/// What one act's tail carried besides the ask.
#[derive(Default)]
struct Tail {
    ask: Option<String>,
    working_context: Option<String>,
    statement: Option<String>,
    /// The act's result and the key of the calls it answers.
    result: Option<(String, String)>,
}

impl Room {
    /// The workspace for `act`, with her working memory brought to what the act's tail shows.
    pub fn act(&mut self, act: &CapturedAct, memory: &WorkingMemory) -> Result<Workspace, String> {
        let messages = &act.request.messages;
        let framing_at = messages.iter().rposition(|m| is_framing(&m.content_text())).unwrap_or(messages.len()); // unwrap_or: no live framing = no tail, the whole request is her view
        let in_force = messages.get(framing_at).map(|m| in_force(&m.content_text())).unwrap_or_default(); // unwrap_or_default: no framing names no contract
        let contracts = contract_names();

        let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
        for message in &messages[..framing_at] {
            let key = wire::key(message);
            if self.sent_tails.contains(&key) || is_native(message) {
                continue;
            }
            let text = message.content_text();
            if let Some((tag, body)) = block(&text) {
                if contracts.iter().any(|name| *name == tag) || in_force.iter().any(|name| name.as_str() == tag) || body.ends_with(": no longer shown") {
                    continue;
                }
                match blocks.iter_mut().find(|(seen, _)| seen == tag) {
                    Some((_, bodies)) if !bodies.iter().any(|b| b == body) => bodies.push(body.to_owned()),
                    Some(_) => {}
                    None => blocks.push((tag.to_owned(), vec![body.to_owned()])),
                }
                continue;
            }
            if is_own_act(&text) {
                continue;
            }
            self.turn(act.room, message.role == "assistant", text, key);
        }

        let tail = Self::tail(&messages[framing_at.min(messages.len())..]);
        if let Some(ask) = &tail.ask {
            self.turn(act.room, false, ask.clone(), wire::key_of(ask));
        }
        // Her statement is recorded once, when she decides it, as the act path does; a tail that
        // repeats it (it stands until settlement) records nothing new.
        if let Some(statement) = tail.statement.filter(|statement| self.last_statement.as_ref() != Some(statement)) {
            memory.record_action_statement(&statement, act.room);
            self.last_statement = Some(statement);
        }
        match &tail.result {
            Some((calls, result)) if self.last_act.as_ref() != Some(calls) => {
                memory.record_receipt(result);
                self.last_act = Some(calls.clone());
            }
            Some(_) => {}
            // No result in her tail: the act before settled.
            None => {
                if self.last_act.take().is_some() {
                    memory.record_settlement("", Some(act.room));
                }
            }
        }
        self.sent_tails.extend(messages[framing_at.min(messages.len())..].iter().map(wire::key));

        let room = ActivityRoom::from_uuid(act.room).map_err(|error| format!("act at {}: {error:?}", act.at_ms))?;
        let mut ws = Workspace::new(Burst::from_turns_at(room, self.turns.clone(), Some(act.at_ms), Cause::Synthetic));
        ws.self_initiated = in_force.iter().any(|name| Some(name.as_str()) == contract_name(OWN_TIME_BLOCK));
        let directed = in_force.iter().any(|name| Some(name.as_str()) == contract_name(DIRECTED_PRESENCE_BLOCK));
        let mut ws = ws.directed(directed);
        ws.broadcast = blocks.into_iter().map(|(tag, bodies)| grounding(tag, bodies)).collect();
        if let Some(context) = tail.working_context {
            ws.broadcast.push(Contribution::context(FacultyId::Custom("working-context".to_owned()), context, 1.0, "faculty replay"));
        }
        Ok(ws)
    }

    /// One turn of her room, appended once, keyed by its content (a turn's identity in a room is
    /// its event; the capture keeps only the text).
    fn turn(&mut self, room: Uuid, is_self: bool, content: String, key: String) {
        if !self.seen.insert(key.clone()) {
            return;
        }
        // The key is a sha256 in hex: its first 16 bytes name the turn's event, deterministically.
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&key.as_bytes()[..16]);
        let event = Uuid::from_bytes(bytes);
        self.turns.push(BurstTurn {
            is_self,
            // No author: the line renders verbatim, as it was captured (`turn_message_line_addressed`).
            author: String::new(),
            content,
            occurred_at_ms: None,
            voice: TurnVoice::Speech,
            source: Some(TurnSource { room, event, lamport: self.turns.len() as u64 }),
        });
    }

    fn tail(messages: &[ChatMessage]) -> Tail {
        let mut tail = Tail::default();
        for message in messages.iter().skip(1) {
            let text = message.content_text();
            let results: Vec<(&str, &str)> = parts(message)
                .iter()
                .filter_map(|part| match part {
                    ContentPart::ToolResult { tool_use_id, content, .. } => Some((tool_use_id.as_str(), content.as_str())),
                    _ => None,
                })
                .collect();
            if !results.is_empty() {
                let ids: Vec<&str> = results.iter().map(|(id, _)| *id).collect();
                let mut whole: Vec<&str> = results.iter().map(|(_, content)| *content).collect();
                if !text.is_empty() {
                    whole.push(&text);
                }
                tail.result = Some((ids.join(","), whole.join("\n")));
            } else if message.role == "assistant" {
                // Her words with her calls, or her statement alone: her action statement. Calls
                // without words state nothing; the calls themselves are keyed by their result.
                if !text.trim().is_empty() {
                    tail.statement = Some(text);
                }
            } else if let Some(context) = text.strip_prefix(WORKING_CONTEXT) {
                tail.working_context = Some(context.trim_start_matches('\n').to_owned());
            } else if text.starts_with(LEGACY_RESULT) {
                let full = text.split_once('\n').map_or(text.as_str(), |(_, full)| full); // map_or: a one-line result is all result
                tail.result = Some((wire::key_of(&text), full.to_owned()));
            } else if !text.trim().is_empty() {
                tail.ask = Some(text);
            }
        }
        tail
    }
}

/// A grounding block as the faculty receives it: a trailing contribution of keyed units.
fn grounding(tag: String, bodies: Vec<String>) -> Contribution {
    let units: Vec<RagItem> = bodies.iter().map(|body| RagItem::whole(wire::key_of(body), body.clone(), serde_json::Value::Null)).collect();
    let mut contribution = Contribution::context(FacultyId::from_kebab(&tag), bodies.join("\n"), 1.0, "faculty replay").with_units(units);
    contribution.trailing = true;
    contribution
}

fn parts(message: &ChatMessage) -> &[ContentPart] {
    match &message.content {
        MessageContent::Parts(parts) => parts,
        MessageContent::Text(_) => &[],
    }
}

fn is_native(message: &ChatMessage) -> bool {
    parts(message).iter().any(|part| !matches!(part, ContentPart::Text { .. }))
}

fn is_framing(text: &str) -> bool {
    text.starts_with("[now ") || text.starts_with("[In force this turn: ")
}

/// Her own acts as her actions stream renders them (`context_streams/actions.rs`).
fn is_own_act(text: &str) -> bool {
    text.starts_with("[actions") || text.starts_with("[action #") || text.starts_with("[result ") || text.starts_with(LEGACY_RESULT)
}

/// `[tag]\nbody`: a block's unit, its tag one line with no other bracket.
fn block(text: &str) -> Option<(&str, &str)> {
    let (head, body) = text.split_once('\n')?;
    let tag = head.strip_prefix('[')?.strip_suffix(']')?;
    (!tag.is_empty() && !tag.contains(['[', ']'])).then_some((tag, body))
}

/// The contracts the framing names in force this act.
fn in_force(framing: &str) -> Vec<String> {
    framing
        .lines()
        .find_map(|line| line.strip_prefix("[In force this turn: "))
        .and_then(|rest| rest.strip_suffix(" (stated in your view)]"))
        .map(|names| names.split("; ").map(str::to_owned).collect())
        .unwrap_or_default() // unwrap_or_default: no in-force line names no contract
}

/// A contract block's name (`deliberation_prompt::contract_of`): `\n\n[Name]\nbody`.
fn contract_name(block: &'static str) -> Option<&'static str> {
    block.trim_start().strip_prefix('[')?.split_once("]\n").map(|(name, _)| name)
}

/// Every contract the faculty adds to her view itself: never a captured block too.
fn contract_names() -> Vec<&'static str> {
    [OWN_TIME_BLOCK, SILENCE_AFFORDANCE_BLOCK, DIRECTED_PRESENCE_BLOCK, WORKING_PRESENCE_BLOCK].into_iter().filter_map(contract_name).collect()
}

/// The shared memory the faculty is built with, for the step to hand both the faculty and the room.
pub fn memory() -> Arc<WorkingMemory> {
    Arc::new(WorkingMemory::new(continuum_cognition::cognition::context_budget::ContextBudget::WORKING_MEMORY_STEPS))
}
