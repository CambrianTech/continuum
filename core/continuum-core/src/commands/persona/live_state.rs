//! `persona/live-state`: what one citizen is doing right now and recently, in one call
//! (card 3b1fc113).
//!
//! 2026-09-28: the team spent an afternoon deciding Kimi was stuck, from @-mentions she did
//! not answer. She was heads-down on a benchmark card she held, and resolved it 6/6. The
//! picture that showed it was assembled by hand from probe queries (turn starts, perceived
//! inputs, acts, silence reasons). This verb is that picture, so it is never guesswork again.
//!
//! What it reports, all from the probe ledger this node writes:
//! - the turn in flight, if one is (a `persona.turn.start` with no terminal row for its
//!   lamport yet);
//! - the last turns, each with how it ended: spoke, silent (and whether a GATE refused her
//!   draft), failed, or ended with no outcome recorded;
//! - the inputs she perceived, per room (so an ask in another room is visible as perceived
//!   or not);
//! - her acts: how many, which tools, and whether any wrote;
//! - the unknowns, named: an empty window is NOT "idle", a truncated scan is NOT "all of it".
//!
//! What it never reports: her private model content. No message text, no thought text, and
//! no pass reason in her own words; only system facts (classes, rooms, tool names, gate
//! names, timings).
//!
//! It answers for citizens HOSTED on this node, because the ledger is this node's. For a
//! citizen on another node, send the same verb there: `grid/send {nodeId, command:
//! "persona/live-state", params}`. Benchmark work and real work read through the same rows,
//! because both are turns and acts of the same citizen.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use airc_core::RoomId;

use crate::identity::PeerId;
use crate::modules::probe_query::{scan_ledger, ProbeRow, MAX_LIMIT};
use crate::routing::probe_file_sink::ENV_PROBE_DIR;
use crate::sdk_codegen::CommandError;

/// How far back the verb looks when the caller does not say: long enough to cover a slow
/// node's hour-long turns several times over.
// derived-or-floor: a floor — six hours holds several of the IntelMac's longest turns.
const DEFAULT_WINDOW_MS: u64 = 6 * 60 * 60 * 1000;

/// Turns listed, newest last.
const DEFAULT_TURNS: u32 = 10;

/// Wire shape for `persona/live-state`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaLiveStateParams.ts")]
pub struct PersonaLiveStateParams {
    /// The citizen: her name or peer id, as the roster shows it.
    pub persona: String,
    /// Look back from this epoch-ms (default: the last six hours).
    #[ts(optional, type = "number")]
    pub since_ms: Option<u64>,
    /// How many recent turns to list (default 10).
    #[ts(optional)]
    pub turns: Option<u32>,
}

/// How a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaTurnOutcome.ts")]
pub enum PersonaTurnOutcome {
    InFlight,
    Spoke,
    Silent,
    Failed,
    // A terminal row with no outcome recorded (only metrics).
    Ended,
}

/// Why a turn ended as it did, in system terms only: never her own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaTurnDetail.ts")]
pub enum PersonaTurnDetail {
    // She chose silence.
    Chosen,
    // A gate refused her draft; the gate's name.
    Gated { gate: String },
    // A gate refused her draft, and its reason carries no gate name this can show.
    GatedUnnamed,
    // A silence logged before the probe carried `gated` (#4533): which it was is unknown.
    PredatesGatedField,
    // Inference failed.
    Inference,
}

/// Something `persona/live-state` could not determine. None of these means idle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaLiveStateUnknown.ts")]
pub enum PersonaLiveStateUnknown {
    // The window holds more matching rows than one scan returns; older turns are not shown.
    Truncated { matched: u32, read: u32 },
    // Not hosted here: her turns are in another node's ledger (grid/send the verb there).
    HostedElsewhere,
    // No rows for her in the window (the ledger may have rotated).
    NoRowsInWindow,
}

/// One turn, as the ledger shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaTurnView.ts")]
pub struct PersonaTurnView {
    pub lamport: String,
    #[ts(optional, type = "string")]
    pub room_id: Option<RoomId>,
    #[ts(type = "number")]
    pub started_ms: u64,
    #[ts(optional, type = "number")]
    pub ended_ms: Option<u64>,
    pub outcome: PersonaTurnOutcome,
    #[ts(optional)]
    pub detail: Option<PersonaTurnDetail>,
}

/// Inputs she perceived in one room.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaPerceivedRoom.ts")]
pub struct PersonaPerceivedRoom {
    // absent: the probe named no room, or not one that parses as a room id
    #[ts(optional, type = "string")]
    pub room_id: Option<RoomId>,
    pub count: u32,
    #[ts(type = "number")]
    pub last_ms: u64,
}

/// Her acts in the window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaActSummary.ts")]
pub struct PersonaActSummary {
    /// Act batches observed.
    pub count: u32,
    /// How many of them wrote (changed something).
    pub wrote: u32,
    #[ts(optional, type = "number")]
    pub last_ms: Option<u64>,
    /// Tool name, times used, most used first.
    pub tools: Vec<(String, u32)>,
}

/// What `persona/live-state` reports.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaLiveState.ts")]
pub struct PersonaLiveState {
    pub persona: String,
    #[ts(optional, type = "string")]
    pub peer_id: Option<PeerId>,
    /// Hosted on this node (her turns are in this node's ledger).
    pub hosted_here: bool,
    #[ts(type = "number")]
    pub since_ms: u64,
    #[ts(type = "number")]
    pub as_of_ms: u64,
    #[ts(optional)]
    pub current_turn: Option<PersonaTurnView>,
    /// Newest last.
    pub recent_turns: Vec<PersonaTurnView>,
    pub perceived: Vec<PersonaPerceivedRoom>,
    pub acts: PersonaActSummary,
    /// What could not be determined, and why. An empty window is not "idle".
    pub unknowns: Vec<PersonaLiveStateUnknown>,
    /// Where the answer came from: ledger files and how many rows were read.
    pub source: String,
}

fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Is this row hers? Probes key her by name in some classes and by peer id in others, always
/// under `persona` or `persona_id`. Never `peer_id`: on a turn start that is the SENDER of the
/// message she is answering, so matching it would count another citizen's turn, triggered by
/// her message, as hers.
fn is_hers(row: &ProbeRow, name: &str, peer: Option<PeerId>) -> bool {
    ["persona", "persona_id"].iter().any(|k| {
        text(row.fields.get(*k)).is_some_and(|v| v.eq_ignore_ascii_case(name) || peer.is_some_and(|p| v.eq_ignore_ascii_case(&p.to_string())))
    })
}

/// A room id as a probe wrote it; text that is not a room id names no room.
fn room(v: Option<&Value>) -> Option<RoomId> {
    text(v).and_then(|t| uuid::Uuid::parse_str(&t).ok()).map(RoomId::from_uuid)
}

/// The gate named in a gated pass reason (`gate-refused/<gate>:...`), and nothing else of it.
/// Only a reason that carries the gate prefix AND a plain identifier as the gate is named; any
/// other text is her draft's business, so it reads as an unnamed gate (Fable on #4534: a
/// reason without the prefix must not leak its text before the first colon).
fn gate_name(reason: Option<&str>) -> PersonaTurnDetail {
    let gate = reason
        .and_then(|r| r.strip_prefix(crate::cognition::workspace::GATE_REFUSAL_PREFIX))
        .and_then(|rest| rest.split(':').next())
        .filter(|g| !g.is_empty() && g.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
    match gate {
        Some(gate) => PersonaTurnDetail::Gated { gate: gate.to_string() },
        None => PersonaTurnDetail::GatedUnnamed,
    }
}

/// THE PROJECTION, pure: her rows (chronological) into the view. Private model content never
/// enters: only classes, rooms, lamports, timings, tool names and gate names are read.
pub(crate) fn project(rows: &[ProbeRow], turns_wanted: u32) -> (Option<PersonaTurnView>, Vec<PersonaTurnView>, Vec<PersonaPerceivedRoom>, PersonaActSummary) {
    let mut turns: Vec<PersonaTurnView> = Vec::new();
    let mut by_lamport: BTreeMap<String, usize> = BTreeMap::new();
    let mut perceived: BTreeMap<Option<uuid::Uuid>, (u32, u64)> = BTreeMap::new();
    let mut acts = PersonaActSummary::default();
    let mut tools: BTreeMap<String, u32> = BTreeMap::new();
    for row in rows {
        let lamport = text(row.fields.get("lamport"));
        match row.class.as_str() {
            "persona.turn.start" => {
                let lamport = lamport.unwrap_or_default();
                by_lamport.insert(lamport.clone(), turns.len());
                turns.push(PersonaTurnView {
                    lamport,
                    room_id: room(row.fields.get("room_id")),
                    started_ms: row.captured_at_ms,
                    ended_ms: None,
                    outcome: PersonaTurnOutcome::InFlight,
                    detail: None,
                });
            }
            class @ ("persona.turn.spoke" | "persona.turn.silent" | "persona.turn.inference_failed" | "persona.turn.metrics") => {
                let Some(i) = lamport.as_ref().and_then(|l| by_lamport.get(l)).copied() else { continue };
                let turn = &mut turns[i];
                turn.ended_ms = Some(turn.ended_ms.map_or(row.captured_at_ms, |e| e.max(row.captured_at_ms)));
                match class {
                    "persona.turn.spoke" => turn.outcome = PersonaTurnOutcome::Spoke,
                    "persona.turn.silent" => {
                        turn.outcome = PersonaTurnOutcome::Silent;
                        // a gate's name is the system's; her own pass reason is private. A row
                        // from before the probe carried `gated` (#4533) cannot say which it was.
                        let gated = row.fields.get("gated").map(|g| g.as_bool() == Some(true) || g.as_str() == Some("true"));
                        turn.detail = Some(match gated {
                            None => PersonaTurnDetail::PredatesGatedField,
                            Some(false) => PersonaTurnDetail::Chosen,
                            Some(true) => gate_name(text(row.fields.get("pass_reason")).as_deref()),
                        });
                    }
                    "persona.turn.inference_failed" => {
                        turn.outcome = PersonaTurnOutcome::Failed;
                        turn.detail = Some(PersonaTurnDetail::Inference);
                    }
                    _ => {
                        if turn.outcome == PersonaTurnOutcome::InFlight {
                            turn.outcome = PersonaTurnOutcome::Ended;
                        }
                    }
                }
            }
            "persona.turn.input_perceived" => {
                // a perceived input with no room still counts, under an absent room
                let at = room(row.fields.get("input_room")).or_else(|| room(row.fields.get("active_room")));
                let e = perceived.entry(at.map(|r| r.as_uuid())).or_insert((0, 0));
                e.0 += 1;
                e.1 = e.1.max(row.captured_at_ms);
            }
            "persona.act.observed" => {
                acts.count += 1;
                acts.last_ms = Some(row.captured_at_ms);
                if row.fields.get("wrote").is_some_and(|w| w.as_bool() == Some(true) || w.as_str() == Some("true")) {
                    acts.wrote += 1;
                }
                if let Some(t) = text(row.fields.get("tools")) {
                    for tool in t.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty()) {
                        *tools.entry(tool.to_string()).or_insert(0) += 1;
                    }
                }
            }
            _ => {}
        }
    }
    let mut tools: Vec<(String, u32)> = tools.into_iter().collect();
    tools.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    acts.tools = tools;
    let current = turns.iter().rev().find(|t| t.outcome == PersonaTurnOutcome::InFlight).cloned();
    let keep = turns.len().saturating_sub(turns_wanted as usize);
    let recent = turns.split_off(keep);
    let mut perceived: Vec<PersonaPerceivedRoom> =
        perceived.into_iter().map(|(at, (count, last_ms))| PersonaPerceivedRoom { room_id: at.map(RoomId::from_uuid), count, last_ms }).collect();
    perceived.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
    (current, recent, perceived, acts)
}

crate::action_command! {
    /// What one citizen is doing now and recently: the turn in flight, her last turns and how
    /// each ended (spoke, silent and whether a gate refused her draft, failed), the inputs she
    /// perceived per room, and her acts by tool. System facts only, never her private model
    /// content; unknowns are named, never read as idle. For a citizen on another node, send
    /// this verb there with grid/send.
    pub struct PersonaLiveStateCommand;
    name: "persona/live-state",
    access: Privileged,
    params: PersonaLiveStateParams,
    output: PersonaLiveState,
    run(_this, _ctx, p) => {
        let wanted = p.persona.trim().to_string();
        if wanted.is_empty() {
            return Err(CommandError::Invalid("persona/live-state: name her (name or peer id)".into()));
        }
        let roster = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .map(|reg| reg.roster_snapshot())
            .unwrap_or_default(); // unwrap_or_default: no registry means no citizen is hosted here, reported as such below
        // an exact name or full id wins; a short id prefix must name exactly one citizen
        let exact = roster.iter().find(|(name, id)| name.eq_ignore_ascii_case(&wanted) || id.to_string() == wanted);
        let hosted = match exact {
            Some(hit) => Some(hit),
            None => {
                let by_prefix: Vec<&(String, uuid::Uuid)> = roster.iter().filter(|(_, id)| id.to_string().starts_with(&wanted)).collect();
                if by_prefix.len() > 1 {
                    let names: Vec<&str> = by_prefix.iter().map(|(n, _)| n.as_str()).collect();
                    return Err(CommandError::Invalid(format!(
                        "persona/live-state: '{wanted}' is the start of {} citizens' ids ({}); give more of it or her name",
                        by_prefix.len(),
                        names.join(", ")
                    )));
                }
                by_prefix.into_iter().next()
            }
        };
        let (name, peer) = match hosted {
            Some((name, id)) => (name.clone(), Some(PeerId::from_uuid(*id))),
            None => (wanted.clone(), None),
        };
        let as_of_ms = crate::persona::trace::now_ms();
        let since_ms = p.since_ms.unwrap_or_else(|| as_of_ms.saturating_sub(DEFAULT_WINDOW_MS));
        let dir = std::env::var(ENV_PROBE_DIR).map_err(|_| {
            CommandError::Invalid(format!(
                "persona/live-state reads the probe ledger, and it is not being written here ({ENV_PROBE_DIR} is unset): \
                 this is a configuration state, not an idle citizen"
            ))
        })?;
        let dir = PathBuf::from(dir);
        let classes: HashSet<String> = ["persona.turn", "persona.act"].iter().map(|s| s.to_string()).collect();
        let (needles, turns_wanted) = (
            [Some(name.to_lowercase()), peer.map(|p| p.to_string())],
            p.turns.unwrap_or(DEFAULT_TURNS),
        );
        let scans = tokio::task::spawn_blocking(move || {
            needles
                .iter()
                .flatten()
                .map(|needle| scan_ledger(&dir, &classes, Some(since_ms), Some(needle.as_str()), None, MAX_LIMIT))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| CommandError::Internal(format!("probe ledger scan panicked: {e}")))?;
        let mut rows: Vec<ProbeRow> = Vec::new();
        let mut unknowns = Vec::new();
        let mut sources: Vec<String> = Vec::new();
        let mut scanned = 0u32;
        let mut seen: HashSet<(u64, String, String)> = HashSet::new();
        for scan in scans {
            let scan = scan?;
            if scan.matched > scan.events.len() as u32 {
                unknowns.push(PersonaLiveStateUnknown::Truncated { matched: scan.matched, read: scan.events.len() as u32 });
            }
            scanned = scanned.max(scan.scanned);
            for s in scan.sources {
                if !sources.contains(&s) {
                    sources.push(s);
                }
            }
            for row in scan.events {
                if is_hers(&row, &name, peer)
                    && seen.insert((row.captured_at_ms, row.class.clone(), serde_json::to_string(&row.fields).unwrap_or_default())) // unwrap_or_default: a field map always serializes; an empty key only weakens dedupe
                {
                    rows.push(row);
                }
            }
        }
        rows.sort_by_key(|r| r.captured_at_ms);
        let (current_turn, recent_turns, perceived, acts) = project(&rows, turns_wanted);
        if peer.is_none() {
            unknowns.push(PersonaLiveStateUnknown::HostedElsewhere);
        }
        if rows.is_empty() {
            unknowns.push(PersonaLiveStateUnknown::NoRowsInWindow);
        }
        Ok(PersonaLiveState {
            persona: name,
            peer_id: peer,
            hosted_here: peer.is_some(),
            since_ms,
            as_of_ms,
            current_turn,
            recent_turns,
            perceived,
            acts,
            unknowns,
            source: format!("probe ledger on this node: {scanned} rows read from {}", sources.join(", ")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(ms: u64, class: &str, fields: Value) -> ProbeRow {
        let Value::Object(fields) = fields else { unreachable!("test rows are objects") };
        ProbeRow { captured_at_ms: ms, class: class.into(), message: String::new(), fields }
    }

    // what this catches (card 3b1fc113): an afternoon of guessing that Kimi was stuck. One read
    // must show the turn in flight, how each turn ended (a chosen silence apart from a GATE
    // eating her draft), the inputs she perceived per room, and her acts by tool, and it must
    // never carry her private words (the pass reason text is replaced by the gate's name or
    // "chosen").
    #[test]
    fn one_read_shows_turns_silences_inputs_and_acts_without_her_words() {
        let rows = vec![
            row(1, "persona.turn.input_perceived", json!({"persona": "k", "input_room": "cb2e21a1-999a-5a03-a184-df06e4ee7097", "active_room": "5dee0000-0000-4000-8000-000000000001"})),
            row(2, "persona.turn.start", json!({"persona": "Kimi", "lamport": "10", "room_id": "5dee0000-0000-4000-8000-000000000001"})),
            row(3, "persona.act.observed", json!({"persona": "Kimi", "tools": "code/shell,code/read", "wrote": false})),
            row(4, "persona.turn.silent", json!({"persona": "Kimi", "lamport": "10", "gated": false, "pass_reason": "my private reasoning"})),
            row(5, "persona.turn.start", json!({"persona": "Kimi", "lamport": "11", "room_id": "5dee0000-0000-4000-8000-000000000001"})),
            row(6, "persona.turn.silent", json!({"persona": "Kimi", "lamport": "11", "gated": true, "pass_reason": "gate-refused/not_speech:bare_call: echo"})),
            row(7, "persona.turn.start", json!({"persona": "Kimi", "lamport": "12", "room_id": "5dee0000-0000-4000-8000-000000000001"})),
            row(8, "persona.act.observed", json!({"persona": "Kimi", "tools": "code/shell", "wrote": true})),
        ];
        let (current, recent, perceived, acts) = project(&rows, 10);
        assert_eq!(current.as_ref().map(|t| t.lamport.as_str()), Some("12"), "the turn in flight");
        let outcomes: Vec<(PersonaTurnOutcome, Option<PersonaTurnDetail>)> = recent.iter().map(|t| (t.outcome, t.detail.clone())).collect();
        assert_eq!(
            outcomes,
            vec![
                (PersonaTurnOutcome::Silent, Some(PersonaTurnDetail::Chosen)),
                (PersonaTurnOutcome::Silent, Some(PersonaTurnDetail::Gated { gate: "not_speech".into() })),
                (PersonaTurnOutcome::InFlight, None),
            ]
        );
        let ask_room = RoomId::from_uuid(uuid::Uuid::parse_str("cb2e21a1-999a-5a03-a184-df06e4ee7097").expect("room uuid"));
        assert_eq!(perceived, vec![PersonaPerceivedRoom { room_id: Some(ask_room), count: 1, last_ms: 1 }], "the ask's room, perceived");
        assert_eq!(acts.count, 2);
        assert_eq!(acts.wrote, 1);
        assert_eq!(acts.tools.first(), Some(&("code/shell".to_string(), 2)));
        let wire = serde_json::to_string(&(current, recent)).expect("serialize");
        assert!(!wire.contains("private reasoning"), "her own words never leave: {wire}");
    }

    // what this catches: a silence logged before the probe carried `gated` (#4533) read as a
    // CHOSEN silence. It cannot say whether a gate ate her draft, so it says it cannot.
    #[test]
    fn a_silence_from_before_the_gated_field_is_unknown_not_chosen() {
        let rows = vec![
            row(1, "persona.turn.start", json!({"persona": "Kimi", "lamport": "9"})),
            row(2, "persona.turn.silent", json!({"persona": "Kimi", "lamport": "9", "reason": "workspace-pass"})),
        ];
        let (current, recent, _, _) = project(&rows, 10);
        assert!(current.is_none());
        assert_eq!(recent[0].detail, Some(PersonaTurnDetail::PredatesGatedField));
    }

    // what this catches (Fable on #4534): a gated reason WITHOUT the gate prefix leaking her
    // own text up to its first colon. Only a prefixed reason with a plain identifier as the
    // gate is named; everything else is an unnamed gate.
    #[test]
    fn a_gated_reason_names_only_a_prefixed_plain_gate() {
        assert_eq!(gate_name(Some("gate-refused/not_speech:bare_call: x")), PersonaTurnDetail::Gated { gate: "not_speech".into() });
        assert_eq!(gate_name(Some("my private plan: do x")), PersonaTurnDetail::GatedUnnamed, "no prefix: her words stay hers");
        assert_eq!(gate_name(Some("gate-refused/Her Words:x")), PersonaTurnDetail::GatedUnnamed, "not a plain identifier");
        assert_eq!(gate_name(None), PersonaTurnDetail::GatedUnnamed);
    }

    // what this catches: probes key a citizen by name in some classes and by peer id in others;
    // a filter on one of them silently loses half her rows (Codex's UUID-only filter missed
    // Kimi's name-keyed act rows on 2026-09-28). And a turn start's `peer_id` is the SENDER:
    // Iris answering Kimi's message is Iris's turn, never Kimi's.
    #[test]
    fn her_rows_are_found_by_name_or_by_peer_id() {
        let kimi = PeerId::from_uuid(uuid::Uuid::parse_str("e2f0e022-04ac-4d5e-9f10-1a2b3c4d5e6f").expect("peer uuid"));
        let by_name = row(1, "persona.turn.start", json!({"persona": "Kimi"}));
        let by_id = row(2, "persona.turn.input_perceived", json!({"persona": kimi.to_string()}));
        let other = row(3, "persona.turn.start", json!({"persona": "Iris"}));
        let answering_her = row(4, "persona.turn.start", json!({"persona": "Iris", "persona_id": "0000", "peer_id": kimi.to_string()}));
        assert!(is_hers(&by_name, "kimi", Some(kimi)));
        assert!(is_hers(&by_id, "kimi", Some(kimi)));
        assert!(!is_hers(&other, "kimi", Some(kimi)));
        assert!(!is_hers(&answering_her, "kimi", Some(kimi)), "her message's sender id on Iris's turn is not her turn");
    }
}
