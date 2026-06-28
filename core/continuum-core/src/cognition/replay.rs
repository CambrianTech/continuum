//! `cognition/replay` — the factory station that isolates ONE cognition phase.
//!
//! The principle Joel hammered: you must be able to go to ANY part of the
//! persona's cognition assembly line, focus on that one step, feed it a KNOWN
//! input, mutate one variable, and re-measure — deterministically, forever,
//! with a single amnesiac-proof command. `scripts/dev/replay-turn.sh` could only
//! repeat the final inference seam (`ai/generate`). This repeats the seam that
//! actually matters: a *faculty* (`contribute(&Workspace) -> Contribution`).
//!
//! It stands on [`WorkspaceCycle::replay`](super::workspace::WorkspaceCycle::replay):
//! it reconstructs the `Workspace` a faculty saw (either a real captured burst
//! from `~/.continuum/fixtures/workspace-traces/<persona>.jsonl`, or a burst you
//! supply to probe "what does recall surface for THIS text?"), forks the
//! persona's LIVE cycle the humane way (a measured copy — never degrades the
//! living being, [[humane-snapshot-eval]]), and re-runs the faculties against
//! that workspace, each one timed.
//!
//! Lossless by construction for the PERCEPTION phase: the load-bearing input a
//! perception faculty (recall, salience, world-model) reads is `world_state`,
//! and the trace captures `world_state` verbatim. Reconstruct it, replay recall,
//! read exactly what memory surfaced — the half-blind-recall investigation
//! ([[recall-is-semantic-capable-but-underpowered]]) made a one-command probe.
//!
//! No fallback: if neither a supplied `world_state` nor a readable capture turn
//! is available, it fails loud naming the cause — it never replays an empty
//! burst and calls it a result ([[fallbacks-are-illegal-fail-loud]]).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::cognition::persona_workspace;
use crate::cognition::workspace::{FacultyId, Workspace};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Where the live cycle writes its per-persona workspace traces (mirror of
/// `persona_workspace`'s capture wiring). The replay reads the SAME files.
fn traces_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::Path::new(&h).join(".continuum/fixtures/workspace-traces"))
}

/// The fields of an on-disk workspace-trace line we need to RECONSTRUCT the
/// workspace. The writer's record type ([`super::workspace_capture`]) is
/// `Serialize`-only (it owns the wire format); this is the read mirror, and it
/// deliberately reads ONLY the lossless fields a perception replay depends on.
#[derive(Debug, Deserialize)]
struct TraceLine {
    captured_at_ms: u64,
    room_id: String,
    world_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionReplayParams {
    /// The persona (UUID) whose faculties + live cycle to replay. Must be
    /// spawned (have a live `WorkspaceCycle`) — replay drives a measured COPY of
    /// her real cognition, never a stand-in.
    pub persona_id: String,
    /// Isolate ONE faculty by kebab tag (`recall`, `salience`, `world-model`,
    /// `deliberation`, …). Omit to replay every faculty in her cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub faculty: Option<String>,
    /// Override the burst the faculties reason over — the one-variable knob:
    /// supply a hand-crafted `world_state` to probe "what does recall surface for
    /// THIS?". When omitted, the burst comes from a captured turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub world_state: Option<String>,
    /// Which captured turn to reconstruct when `world_state` is not supplied.
    /// Negative counts from the end (`-1` = most recent, the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub turn: Option<i64>,
    /// Room scope for the reconstructed workspace. Overrides the captured turn's
    /// room when set; defaults to the captured room, or the nil room for a
    /// supplied burst.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room_id: Option<String>,
}

/// One faculty's replayed bid, projected to the wire — the same shape the
/// capture writes, plus the per-faculty wall-clock that makes replay a
/// measurement and not just an inspection.
#[derive(Debug, Clone, Serialize, TS)]
pub struct ReplayBidOut {
    /// Which faculty produced this bid (kebab tag).
    pub faculty: String,
    /// True when the faculty abstained this run (`contribute` returned `None`) —
    /// it had nothing to surface for this burst.
    pub abstained: bool,
    /// The faculty's self-assigned salience (0 when it abstained).
    #[ts(type = "number")]
    pub salience: f32,
    /// The content it surfaced — for recall, THIS is the engram text the decider
    /// would have seen (the load-bearing "was memory present?" field).
    pub content: String,
    /// The faculty's audit reasoning.
    pub reasoning: String,
    /// True for the deliberation verdict bid (the one carrying a `Decision`).
    pub is_decision: bool,
    /// Wall-clock this single faculty took, in microseconds — attributable
    /// because `replay` runs faculties sequentially on the measurement path.
    #[ts(type = "number")]
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionReplayResult {
    pub persona_id: String,
    /// Where the burst came from: `"supplied"` or `"capture@<turn> (<ms>)"`.
    pub source: String,
    /// The exact burst replayed — echoed back so the result is self-explaining.
    pub world_state: String,
    pub room_id: String,
    /// One entry per faculty that ran (just the isolated one, or all).
    pub bids: Vec<ReplayBidOut>,
}

/// Resolve the burst (+ room + provenance) to replay: a supplied `world_state`
/// wins; otherwise read the chosen captured turn. Fails loud — never an empty
/// burst — naming exactly what was missing.
fn resolve_burst(
    p: &CognitionReplayParams,
    persona_id: &Uuid,
) -> Result<(String, String, String), CommandError> {
    if let Some(ws) = &p.world_state {
        let room = p.room_id.clone().unwrap_or_else(|| Uuid::nil().to_string());
        return Ok((ws.clone(), room, "supplied".to_string()));
    }

    let dir = traces_dir()
        .ok_or_else(|| CommandError::Invalid("HOME unset — cannot locate workspace traces; pass `world_state` to replay against a supplied burst".into()))?;
    let path = dir.join(format!("{persona_id}.jsonl"));
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        CommandError::Invalid(format!(
            "no workspace trace for persona {persona_id} at {} ({e}); pass `world_state` to replay a supplied burst",
            path.display()
        ))
    })?;
    let lines: Vec<TraceLine> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<TraceLine>(l).ok())
        .collect();
    if lines.is_empty() {
        return Err(CommandError::Invalid(format!(
            "workspace trace {} has no usable turns; pass `world_state` to replay a supplied burst",
            path.display()
        )));
    }
    let turn = p.turn.unwrap_or(-1);
    let idx = if turn < 0 {
        lines.len() as i64 + turn
    } else {
        turn
    };
    if idx < 0 || idx as usize >= lines.len() {
        return Err(CommandError::Invalid(format!(
            "turn {turn} out of range — trace has {} turns (use -1 for the most recent)",
            lines.len()
        )));
    }
    let line = &lines[idx as usize];
    let room = p.room_id.clone().unwrap_or_else(|| line.room_id.clone());
    Ok((
        line.world_state.clone(),
        room,
        format!("capture@{turn} ({}ms)", line.captured_at_ms),
    ))
}

#[derive(Default)]
pub struct CognitionReplay;

#[async_trait]
impl ActionCommand for CognitionReplay {
    const NAME: &'static str = "cognition/replay";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Replay ONE step of a persona's cognition in isolation, timed and \
         deterministic. Reconstructs the workspace a faculty saw — from a captured \
         turn or a `world_state` you supply — forks a MEASURED COPY of her live \
         cycle (never degrades the living persona), and re-runs the faculties. \
         Isolate one with `faculty` (recall/salience/world-model/deliberation/…) \
         or replay all. Returns each faculty's bid (content, salience, reasoning) \
         plus its wall-clock — the glass-box probe for 'what did this step do, and \
         how long did it take?'.";
    type Params = CognitionReplayParams;
    type Output = CognitionReplayResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: CognitionReplayParams,
    ) -> Result<CognitionReplayResult, CommandError> {
        let persona_uuid = Uuid::parse_str(&p.persona_id).map_err(|_| {
            CommandError::Invalid(format!("persona_id '{}' is not a valid UUID", p.persona_id))
        })?;

        let (world_state, room_str, source) = resolve_burst(&p, &persona_uuid)?;
        let room = Uuid::parse_str(&room_str)
            .map_err(|_| CommandError::Invalid(format!("room_id '{room_str}' is not a valid UUID")))?;

        // Fork the LIVE cycle the humane way: a measured copy, isolated for the
        // duration, paged back out after — her real mind is never touched.
        let cycle = persona_workspace::global()
            .fork_eval_cycle(&persona_uuid)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "persona {persona_uuid} has no live WorkspaceCycle — spawn her before replaying her cognition"
                ))
            })?;
        let isolation = cycle.isolate_for_eval();

        let only = p.faculty.as_deref().map(FacultyId::from_kebab);
        let ws = Workspace::in_room(world_state.clone(), room);
        let bids = cycle.replay(&ws, only.as_ref()).await;

        drop(isolation);
        cycle.page_out();

        // Asked to isolate a faculty she doesn't have? Fail loud naming it —
        // never return an empty result that reads as "the faculty did nothing".
        if let Some(id) = &only {
            if bids.is_empty() {
                return Err(CommandError::Invalid(format!(
                    "faculty '{}' is not in persona {persona_uuid}'s cycle — nothing to replay",
                    id.as_str()
                )));
            }
        }

        let bids = bids
            .into_iter()
            .map(|b| match b.contribution {
                Some(c) => ReplayBidOut {
                    faculty: c.faculty.as_str().to_string(),
                    abstained: false,
                    salience: c.salience,
                    content: c.content,
                    reasoning: c.reasoning,
                    is_decision: c.decision.is_some(),
                    elapsed_us: b.elapsed_us as u64,
                },
                None => ReplayBidOut {
                    faculty: b.faculty.as_str().to_string(),
                    abstained: true,
                    salience: 0.0,
                    content: String::new(),
                    reasoning: String::new(),
                    is_decision: false,
                    elapsed_us: b.elapsed_us as u64,
                },
            })
            .collect();

        Ok(CognitionReplayResult {
            persona_id: persona_uuid.to_string(),
            source,
            world_state,
            room_id: room.to_string(),
            bids,
        })
    }
}

crate::register_stateless_command!(CognitionReplay);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the burst resolver must FAIL LOUD (never an empty burst)
    // when neither a supplied world_state nor a readable capture exists — the
    // no-fallback contract for the factory's input step.
    #[test]
    fn resolve_burst_fails_loud_with_no_source() {
        let persona = Uuid::nil();
        // No world_state supplied; nil persona has no trace file → must error,
        // and the error must name the missing input (point the operator at the fix).
        let p = CognitionReplayParams {
            persona_id: persona.to_string(),
            faculty: None,
            world_state: None,
            turn: None,
            room_id: None,
        };
        let err = resolve_burst(&p, &persona).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("world_state"),
            "error must name the missing burst input, got: {msg}"
        );
    }

    // what this catches: a supplied world_state must be used verbatim (the
    // one-variable knob) and tagged "supplied", with the nil room as default.
    #[test]
    fn resolve_burst_uses_supplied_world_state() {
        let persona = Uuid::nil();
        let p = CognitionReplayParams {
            persona_id: persona.to_string(),
            faculty: Some("recall".to_string()),
            world_state: Some("what was the auth migration codename?".to_string()),
            turn: None,
            room_id: None,
        };
        let (ws, room, source) = resolve_burst(&p, &persona).unwrap();
        assert_eq!(ws, "what was the auth migration codename?");
        assert_eq!(source, "supplied");
        assert_eq!(room, Uuid::nil().to_string());
    }
}
