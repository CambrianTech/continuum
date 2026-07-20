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
use crate::cognition::token_budget::estimate_prompt_tokens;
use crate::cognition::workspace::{Contribution, FacultyId, Workspace};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Where the live cycle writes its per-persona workspace traces (mirror of
/// `persona_workspace`'s capture wiring). The replay reads the SAME files.
fn traces_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::Path::new(&h).join(".continuum/fixtures/workspace-traces"))
}

/// One captured bid from the assembled `context` (the broadcast the decider saw).
/// Read mirror of the writer's `BidRecord` — we only deserialize the fields a
/// deliberation faculty CONSUMES from the broadcast (faculty / content /
/// salience / reasoning). The verdict's `Decision` payload and `is_decision`
/// are irrelevant here: context bids carry no decision, and a replayed
/// deliberation faculty reads the assembled context, not a prior verdict.
#[derive(Debug, Deserialize)]
struct ContextBid {
    faculty: String,
    salience: f32,
    reasoning: String,
    content: String,
}

/// The fields of an on-disk workspace-trace line we need to RECONSTRUCT the
/// workspace. The writer's record type ([`super::workspace_capture`]) is
/// `Serialize`-only (it owns the wire format); this is the read mirror. Both the
/// raw `world_state` (perception input) AND the assembled `context` (the
/// broadcast deliberation reads) are captured, so a deliberation faculty can be
/// replayed against the SAME context it actually saw — not a blinded empty one.
#[derive(Debug, Deserialize)]
struct TraceLine {
    captured_at_ms: u64,
    room_id: String,
    world_state: String,
    /// The assembled broadcast that reached the decider this tick. Absent on
    /// older traces (pre-context-replay) → empty, handled as "no broadcast".
    #[serde(default)]
    context: Vec<ContextBid>,
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

/// One line of the prompt-budget ledger: a single layer she saw, costed in
/// tokens and as a share of the assembled prompt. This is the "take no prompt
/// layer for granted" instrument — every layer line-itemed, sorted by cost, so
/// "recall ate 4000 of 8000 tokens" is a number you read, not a thing you fear.
#[derive(Debug, Clone, Serialize, TS)]
pub struct BudgetLayer {
    /// The layer's faculty (kebab tag) — `recall`, `roster`, `doctrine`, …
    pub faculty: String,
    /// Estimated prompt-token cost of this layer (the unit the RAG sources budget
    /// against — see `cognition::token_budget`).
    #[ts(type = "number")]
    pub tokens: u32,
    /// This layer's share of the total assembled prompt, 0–100.
    #[ts(type = "number")]
    pub share_pct: f32,
}

/// The prompt-budget ledger for the replayed turn: what each layer of her prompt
/// cost, totalled. `total_tokens = world_state_tokens + context_tokens`, and
/// `layers` accounts the reconstructed broadcast (the RAG layers she actually
/// saw), sorted most-expensive first. Honest about scope: this accounts the
/// load-bearing CONTENT layers (world_state + broadcast), not the fixed system
/// framing — those are the layers you tune, the ones a bad RAG step bloats.
#[derive(Debug, Clone, Serialize, TS)]
pub struct PromptBudget {
    /// Tokens spent on the burst itself (the message/event she's reasoning over).
    #[ts(type = "number")]
    pub world_state_tokens: u32,
    /// Tokens spent across all reconstructed context layers (the broadcast).
    #[ts(type = "number")]
    pub context_tokens: u32,
    /// `world_state_tokens + context_tokens` — the accountable prompt mass.
    #[ts(type = "number")]
    pub total_tokens: u32,
    /// Per-layer ledger of the broadcast, sorted by cost (most expensive first).
    /// Empty when no broadcast was reconstructed (a bare supplied burst).
    pub layers: Vec<BudgetLayer>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionReplayResult {
    pub persona_id: String,
    /// Where the burst came from: `"supplied"` or `"capture@<turn> (<ms>)"`.
    pub source: String,
    /// The exact burst replayed — echoed back so the result is self-explaining.
    pub world_state: String,
    pub room_id: String,
    /// What context the deliberation-tier faculties saw — so a replayed verdict
    /// is NEVER mistaken for a live one. `"reconstructed (N ctx)"` when the
    /// broadcast was rebuilt from the captured turn; `"empty"` when none was
    /// available (a supplied bare burst, or an old trace without context).
    pub broadcast_source: String,
    /// The prompt-budget ledger — every layer she saw, costed (see `PromptBudget`).
    pub budget: PromptBudget,
    /// One entry per faculty that ran (just the isolated one, or all).
    pub bids: Vec<ReplayBidOut>,
}

/// What `resolve_burst` hands back: the burst to reason over, the room scope, a
/// human-readable provenance string, and the reconstructed broadcast (the
/// assembled context a deliberation faculty reads — empty for a supplied burst).
#[derive(Debug)]
struct ResolvedBurst {
    world_state: String,
    room: String,
    source: String,
    broadcast: Vec<Contribution>,
}

/// Build the prompt-budget ledger from the burst + the reconstructed broadcast.
/// Every layer line-itemed in the SAME token unit the RAG sources budget against,
/// sorted most-expensive first — the "obsess over every prompt layer" instrument.
fn build_budget(world_state: &str, broadcast: &[Contribution]) -> PromptBudget {
    let world_state_tokens = estimate_prompt_tokens(world_state);
    let mut layers: Vec<BudgetLayer> = broadcast
        .iter()
        .map(|c| BudgetLayer {
            faculty: c.faculty.as_str().to_string(),
            tokens: estimate_prompt_tokens(&c.content),
            share_pct: 0.0, // filled once we know the total
        })
        .collect();
    let context_tokens: u32 = layers.iter().map(|l| l.tokens).sum();
    let total_tokens = world_state_tokens.saturating_add(context_tokens);
    if total_tokens > 0 {
        for l in &mut layers {
            l.share_pct = (l.tokens as f32 / total_tokens as f32) * 100.0;
        }
    }
    // Most expensive layer first — the one to interrogate when the prompt bloats.
    layers.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    PromptBudget {
        world_state_tokens,
        context_tokens,
        total_tokens,
        layers,
    }
}

/// Resolve the burst (+ room + provenance + broadcast) to replay: a supplied
/// `world_state` wins (with no broadcast — a bare perception probe); otherwise
/// read the chosen captured turn and rebuild the broadcast from its captured
/// `context`. Fails loud — never an empty burst — naming exactly what was missing.
fn resolve_burst(
    p: &CognitionReplayParams,
    persona_id: &Uuid,
) -> Result<ResolvedBurst, CommandError> {
    if let Some(ws) = &p.world_state {
        let room = p.room_id.clone().unwrap_or_else(|| Uuid::nil().to_string());
        return Ok(ResolvedBurst {
            world_state: ws.clone(),
            room,
            source: "supplied".to_string(),
            broadcast: Vec::new(),
        });
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
    // Rebuild the broadcast the decider actually saw from the captured context,
    // so a deliberation faculty replays against its REAL input — not a blind one.
    let broadcast = line
        .context
        .iter()
        .map(|c| {
            Contribution::context(
                FacultyId::from_kebab(&c.faculty),
                c.content.clone(),
                c.salience,
                c.reasoning.clone(),
            )
        })
        .collect();
    Ok(ResolvedBurst {
        world_state: line.world_state.clone(),
        room,
        source: format!("capture@{turn} ({}ms)", line.captured_at_ms),
        broadcast,
    })
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
         plus its wall-clock, AND a prompt-budget ledger (`budget`) costing every \
         layer she saw in tokens and share — the glass-box probe for 'what did this \
         step do, how long did it take, and what did each prompt layer cost?'.";
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

        let burst = resolve_burst(&p, &persona_uuid)?;
        let room = Uuid::parse_str(&burst.room).map_err(|_| {
            CommandError::Invalid(format!("room_id '{}' is not a valid UUID", burst.room))
        })?;

        // Fork the LIVE cycle the humane way: a measured copy, isolated for the
        // duration, paged back out after — her real mind is never touched.
        let cycle = persona_workspace::global()
            // Replay reconstructs a faithful live-like cycle — keep her hands.
            .fork_eval_cycle(&persona_uuid, true, None, false)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "persona {persona_uuid} has no live WorkspaceCycle — spawn her before replaying her cognition"
                ))
            })?;
        let isolation = cycle.isolate_for_eval();

        let only = p.faculty.as_deref().map(FacultyId::from_kebab);

        // Refuse to replay a broadcast-reading (deliberation-tier) faculty against
        // an empty broadcast: it wouldn't abstain, it would emit a CONFIDENT verdict
        // computed from blinded cognition — the silent lie the doctrine forbids.
        // Fail loud, name the cause, point at the fix.
        if let Some(id) = &only {
            if cycle.reacts_to_broadcast(id) == Some(true) && burst.broadcast.is_empty() {
                cycle.page_out();
                return Err(CommandError::Invalid(format!(
                    "faculty '{}' reads the assembled broadcast, but none could be reconstructed \
                     ({}). Replaying it against an empty broadcast would produce a confident-but-wrong \
                     verdict. Replay from a captured turn that has context, or isolate a perception \
                     faculty (recall/salience/world-model).",
                    id.as_str(),
                    if p.world_state.is_some() {
                        "a supplied bare world_state carries no broadcast"
                    } else {
                        "the captured turn has no recorded context"
                    }
                )));
            }
        }

        let broadcast_source = if burst.broadcast.is_empty() {
            "empty".to_string()
        } else {
            format!("reconstructed ({} ctx)", burst.broadcast.len())
        };

        // Cost the prompt BEFORE the broadcast moves into the workspace — the
        // ledger of every layer she saw, in the RAG sources' own token unit.
        let budget = build_budget(&burst.world_state, &burst.broadcast);

        let mut ws = Workspace::in_room(burst.world_state.clone(), room);
        ws.broadcast = burst.broadcast;
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
            source: burst.source,
            world_state: burst.world_state,
            room_id: room.to_string(),
            broadcast_source,
            budget,
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
        let b = resolve_burst(&p, &persona).unwrap();
        assert_eq!(b.world_state, "what was the auth migration codename?");
        assert_eq!(b.source, "supplied");
        assert_eq!(b.room, Uuid::nil().to_string());
        // A bare supplied burst carries NO broadcast — this is the invariant the
        // run() guard relies on to refuse a blind deliberation replay.
        assert!(
            b.broadcast.is_empty(),
            "supplied world_state must reconstruct no broadcast"
        );
    }

    // what this catches: the budget ledger must line-item every broadcast layer,
    // sort most-expensive-first, total = world_state + context, and shares sum to
    // ~100% — the "obsess over every prompt layer" instrument reporting honestly.
    #[test]
    fn budget_ledger_costs_and_ranks_every_layer() {
        let broadcast = vec![
            Contribution::context(
                FacultyId::Recall,
                "x".repeat(400), // ~101 tokens — the expensive layer
                0.3,
                "r",
            ),
            Contribution::context(
                FacultyId::Custom("roster".to_string()),
                "y".repeat(40), // ~11 tokens
                0.2,
                "r",
            ),
        ];
        let b = build_budget("hello there", &broadcast);
        assert_eq!(b.world_state_tokens, estimate_prompt_tokens("hello there"));
        assert_eq!(b.context_tokens, b.layers.iter().map(|l| l.tokens).sum::<u32>());
        assert_eq!(b.total_tokens, b.world_state_tokens + b.context_tokens);
        // sorted most-expensive first → recall (400 chars) leads roster (40).
        assert_eq!(b.layers[0].faculty, "recall");
        assert!(b.layers[0].tokens > b.layers[1].tokens);
        // shares are computed against the total (sum ≈ context share of 100%).
        let layer_share: f32 = b.layers.iter().map(|l| l.share_pct).sum();
        let ws_share = b.world_state_tokens as f32 / b.total_tokens as f32 * 100.0;
        assert!((layer_share + ws_share - 100.0).abs() < 0.5, "shares must sum to ~100%");
    }
}
