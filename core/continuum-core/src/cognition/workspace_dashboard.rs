//! Live dashboard capture — the **"dashboard, not archeological dig"** face of
//! the ONE [`WorkspaceCaptureSink`] seam.
//!
//! Where [`super::workspace_capture::JsonlWorkspaceCaptureSink`] appends forensic
//! lines you read *after* the fact, this publishes the LATEST tick over a
//! `tokio::sync::watch` channel so a live view — a `models/*` command, a TUI, the
//! web client — can render the mind working in real time. Watch semantics are
//! exactly right for a dashboard: a subscriber always sees the current frame, no
//! backlog to drain, the newest tick overwrites the last.
//!
//! Both sinks are siblings behind the same hook (`WorkspaceCycle.capture`), so
//! the brain plumbing is unchanged — this honors "half the cognition work is
//! harnesses; reuse the seam, don't reinvent" ([[cognition-half-the-work-is-harnesses]]).
//! Best-effort by construction: a tick with no dashboard attached just replaces
//! the watched value and moves on; capture NEVER fails a cognition turn
//! (OBSERVABILITY-AS-SUBSTRATE.md).
//!
//! The frame carries the two axes the "focused beats verbose" thesis is proven
//! on ([[persona-brain-reactive-cognition]] / `docs/cognition/REALLY-GOOD-HINTS.md`):
//! the **speed axis** (per-faculty timings + the two-barrier critical path) and
//! the **context-size axis** (`context_chars` — the 16k→Nk tool-surface lever),
//! watched live alongside the decision.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use tokio::sync::watch;
use uuid::Uuid;

use super::workspace::{Decision, FacultyTiming, WorkspaceCaptureSink, WorkspaceTrace};

/// The two concurrent barriers, as arithmetic: the turn waits on the SLOWEST
/// faculty of each phase, not the sum — `max(perception) + max(deliberation)`.
///
/// Pure, and lifted out of [`DashboardCaptureSink::record`] on purpose. The COST
/// claim of the grounding-deferral slice — *"taking a slow source off the
/// perception barrier removes exactly its deliver from the turn's wait"* — is
/// arithmetic over per-faculty timings, so it is provable with the timings as
/// PARAMETERS. Proving it instead with a stopwatch around a live cycle is what
/// made the old `deferring_grounding_..._critical_path` test flaky: on a loaded
/// CI runner a scheduler stall forged a 99,798µs "grounding cost" in the fork
/// that pays no grounding cost at all. Same shape as the pure verdict fns in
/// `inference/llama_server.rs` — decide from parameters, leave the clock outside.
pub(crate) fn critical_path_us(timings: &[FacultyTiming]) -> u128 {
    let slowest_of = |deliberation: bool| {
        timings
            .iter()
            .filter(|t| t.deliberation == deliberation)
            .map(|t| t.elapsed_us)
            .max()
            .unwrap_or(0)
    };
    slowest_of(false) + slowest_of(true)
}

/// One faculty's wall-clock this tick, projected to a serializable shape. Same
/// pattern as the JSONL sink's `TimingRecord`: each sink owns its wire format so
/// the live frame can evolve independently of the in-memory `FacultyTiming`
/// (which is intentionally not `Serialize`).
#[derive(Debug, Clone, Serialize, Default)]
pub struct FacultyTickView {
    pub faculty: String,
    pub elapsed_us: u128,
    /// `false` = perception tier, `true` = deliberation tier.
    pub deliberation: bool,
    /// Produced a bid vs abstained (a slow abstainer is still latency to see).
    pub bid: bool,
}

/// The latest tick, projected for live display — the dashboard's frame.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkspaceLiveState {
    pub persona_id: String,
    pub room_id: String,
    /// The consolidated burst the mind reasoned over this tick.
    pub world_state: String,
    /// Per-faculty timings — the speed axis.
    pub timings: Vec<FacultyTickView>,
    /// Two-barrier critical-path estimate: `max(perception) + max(deliberation)`.
    /// This is the wall-clock the turn actually *waited* on (the concurrent
    /// barriers), NOT the sum — the number that must converge on the LLM alone as
    /// the perception tier is deferred off the critical path.
    pub critical_path_us: u128,
    /// Sum of all faculty time — total work done (incl. off-critical concurrency).
    /// `total_faculty_us` ≫ `critical_path_us` is the win: lots of work, little wait.
    pub total_faculty_us: u128,
    /// The focused context that reached the decider: how many bids won attention,
    pub context_bids: usize,
    /// …and their combined size in chars — the 16k→Nk lever, watched live.
    pub context_chars: usize,
    /// Prompt tokens the decider conditioned on (deliberation bid metrics, if present).
    pub input_tokens: u32,
    /// Tokens the decider generated.
    pub output_tokens: u32,
    /// The decision that emerged (kebab-tagged via Decision's serde tag), if any.
    pub decision: Option<Decision>,
    /// This sink's monotonic count of ticks observed — so a dashboard can detect
    /// a stalled mind (tick not advancing) vs a quiet one.
    pub tick: u64,
}

/// Publishes the latest [`WorkspaceTrace`] as a [`WorkspaceLiveState`] over a
/// `watch` channel. Construct one per persona, register it as the persona's
/// `WorkspaceCaptureSink`, and hand `subscribe()` to any live view.
pub struct DashboardCaptureSink {
    persona_id: Uuid,
    tx: watch::Sender<WorkspaceLiveState>,
    tick: AtomicU64,
}

impl DashboardCaptureSink {
    pub fn new(persona_id: Uuid) -> Self {
        let (tx, _rx) = watch::channel(WorkspaceLiveState::default());
        Self {
            persona_id,
            tx,
            tick: AtomicU64::new(0),
        }
    }

    /// Subscribe to live tick frames. Each receiver always sees the current frame
    /// (watch semantics — no per-subscriber backlog, always the newest tick).
    pub fn subscribe(&self) -> watch::Receiver<WorkspaceLiveState> {
        self.tx.subscribe()
    }
}

impl WorkspaceCaptureSink for DashboardCaptureSink {
    fn record(&self, trace: &WorkspaceTrace) {
        let tick = self.tick.fetch_add(1, Ordering::Relaxed) + 1;

        let total_faculty_us: u128 = trace.timings.iter().map(|t| t.elapsed_us).sum();

        let context_chars: usize = trace
            .context_broadcast
            .iter()
            .map(|c| c.content.len())
            .sum();

        // Token counts come from the deliberation bid (the one carrying a Decision).
        let (input_tokens, output_tokens) = trace
            .bids
            .iter()
            .find(|c| c.decision.is_some())
            .and_then(|c| c.metrics.as_ref())
            .map(|m| (m.input_tokens, m.output_tokens))
            .unwrap_or((0, 0));

        let state = WorkspaceLiveState {
            persona_id: self.persona_id.to_string(),
            room_id: trace.room_id.to_string(),
            world_state: trace.world_state.clone(),
            timings: trace
                .timings
                .iter()
                .map(|t| FacultyTickView {
                    faculty: t.faculty.as_str().to_string(),
                    elapsed_us: t.elapsed_us,
                    deliberation: t.deliberation,
                    bid: t.bid,
                })
                .collect(),
            critical_path_us: critical_path_us(&trace.timings),
            total_faculty_us,
            context_bids: trace.context_broadcast.len(),
            context_chars,
            input_tokens,
            output_tokens,
            decision: trace.decision.clone(),
            tick,
        };

        // send_replace publishes even with no receiver attached (a dashboard may
        // not be subscribed) — best-effort, never fails the turn.
        let _ = self.tx.send_replace(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::workspace::{
        Contribution, CycleId, FacultyId, FacultyTiming, TurnMetrics,
    };

    // what this catches: the live dashboard frame projects a tick's load-bearing
    // axes correctly — the two-barrier critical path (max-perception + max-delib,
    // NOT the sum), the focused-context size (the 16k→Nk lever), token counts from
    // the deliberation bid, and the decision — and a subscriber sees it live. If
    // critical_path regresses to a sum, "focused beats verbose" becomes unmeasurable.
    #[tokio::test]
    async fn publishes_live_frame_with_critical_path_and_context_size() {
        let persona = Uuid::new_v4();
        let room = Uuid::new_v4();
        let sink = DashboardCaptureSink::new(persona);
        let mut rx = sink.subscribe();

        let recall = Contribution {
            faculty: FacultyId::Recall,
            cycle: CycleId::UNSTAMPED,
            content: "abcdefghij".to_string(), // 10 chars
            salience: 0.6,
            reasoning: "engram".to_string(),
            decision: None,
            metrics: None,
            stable: false,
            fault: None,
            raw_generation: None,
            trailing: false,
            parts: Vec::new(),
            expand_command: None,
        };
        let verdict = Contribution {
            faculty: FacultyId::Deliberation,
            cycle: CycleId::UNSTAMPED,
            content: "Rolling back.".to_string(),
            salience: 0.9,
            reasoning: "decider".to_string(),
            decision: Some(Decision::Speak {
                text: "Rolling back.".to_string(),
            }),
            metrics: Some(TurnMetrics {
                input_tokens: 1500,
                output_tokens: 40,
                ..Default::default()
            }),
            stable: false,
            fault: None,
            raw_generation: None,
            trailing: false,
            parts: Vec::new(),
            expand_command: None,
        };
        let trace = WorkspaceTrace {
            world_state: "what's the call?".to_string(),
            room_id: room,
            bids: vec![recall.clone(), verdict.clone()],
            context_broadcast: vec![recall.clone()],
            broadcast: vec![recall, verdict],
            decision: Some(Decision::Speak {
                text: "Rolling back.".to_string(),
            }),
            timings: vec![
                // perception tier: slowest perception faculty = 200µs
                FacultyTiming {
                    faculty: FacultyId::Recall,
                    elapsed_us: 200,
                    deliberation: false,
                    bid: true,
                },
                FacultyTiming {
                    faculty: FacultyId::WorldModel,
                    elapsed_us: 50,
                    deliberation: false,
                    bid: false,
                },
                // deliberation tier: the LLM = 5000µs
                FacultyTiming {
                    faculty: FacultyId::Deliberation,
                    elapsed_us: 5000,
                    deliberation: true,
                    bid: true,
                },
            ],
        };

        sink.record(&trace);

        // The subscriber sees the latest frame live.
        assert!(rx.has_changed().unwrap());
        let frame = rx.borrow_and_update().clone();

        assert_eq!(frame.persona_id, persona.to_string());
        assert_eq!(frame.room_id, room.to_string());
        assert_eq!(frame.tick, 1);
        // Critical path = max(perception=200) + max(deliberation=5000) = 5200,
        // NOT the sum (5250) — the barrier waits on the slowest of each phase.
        assert_eq!(frame.critical_path_us, 5200);
        assert_eq!(frame.total_faculty_us, 5250);
        // The focused context that reached the decider: 1 bid, 10 chars.
        assert_eq!(frame.context_bids, 1);
        assert_eq!(frame.context_chars, 10);
        // Token counts pulled from the deliberation bid's metrics.
        assert_eq!(frame.input_tokens, 1500);
        assert_eq!(frame.output_tokens, 40);
        // The decision rides along, kebab-tagged.
        assert!(matches!(frame.decision, Some(Decision::Speak { .. })));
    }

    // what this catches: the COST half of the grounding-deferral claim — that
    // moving a slow grounding source OFF the perception barrier removes exactly
    // its deliver from the turn's wait — as arithmetic over the timings instead
    // of a stopwatch around a live cycle. The structural half (that the deferred
    // tick genuinely never awaits `deliver()`) is proven clock-free in
    // `persona_workspace.rs`. Together they replace the wall-clock test that CI
    // load could forge either way.
    #[test]
    fn taking_the_slow_grounding_faculty_off_the_barrier_removes_exactly_its_cost() {
        let deliberation = FacultyTiming {
            faculty: FacultyId::Deliberation,
            elapsed_us: 5_000,
            deliberation: true,
            bid: true,
        };
        let fast_perception = FacultyTiming {
            faculty: FacultyId::Recall,
            elapsed_us: 200,
            deliberation: false,
            bid: true,
        };
        let grounding = |elapsed_us| FacultyTiming {
            faculty: FacultyId::Custom("grounding".to_string()),
            elapsed_us,
            deliberation: false,
            bid: true,
        };

        // Synchronous: the 60ms deliver IS the perception max, so the turn waits on it.
        let on_barrier = critical_path_us(&[
            fast_perception.clone(),
            grounding(60_000),
            deliberation.clone(),
        ]);
        assert_eq!(on_barrier, 60_000 + 5_000);

        // Deferred: the tick serves reprojected last-good, so the grounding faculty
        // reads ~0 and the perception max collapses back to the fast faculty.
        let off_barrier = critical_path_us(&[
            fast_perception.clone(),
            grounding(3),
            deliberation.clone(),
        ]);
        assert_eq!(off_barrier, 200 + 5_000);
        assert_eq!(
            on_barrier - off_barrier,
            59_800,
            "deferral must remove the grounding deliver, and nothing else, from the wait"
        );

        // The barrier is a MAX, not a sum: a second slow perception faculty means
        // deferring grounding buys nothing, and the arithmetic must say so rather
        // than crediting the slice with a win it didn't earn.
        let two_slow = critical_path_us(&[
            FacultyTiming {
                elapsed_us: 60_000,
                ..fast_perception.clone()
            },
            grounding(60_000),
            deliberation.clone(),
        ]);
        let one_deferred = critical_path_us(&[
            FacultyTiming {
                elapsed_us: 60_000,
                ..fast_perception.clone()
            },
            grounding(3),
            deliberation.clone(),
        ]);
        assert_eq!(two_slow, one_deferred, "the barrier waits on the slowest");

        // An all-perception tick has no deliberation term, and vice versa — the
        // `unwrap_or(0)` legs, so a half-populated trace can't underflow the frame.
        assert_eq!(critical_path_us(&[grounding(700)]), 700);
        assert_eq!(critical_path_us(&[deliberation]), 5_000);
        assert_eq!(critical_path_us(&[]), 0);
    }
}
