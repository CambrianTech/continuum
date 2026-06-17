//! Replay A/B Bench — orthogonal slice atop M5's `WorkspaceCaptureSink`.
//!
//! Lane #2 per the cross-tab assignment: record a real persona turn through
//! the [`WorkspaceCycle`], replay through a MUTATED cycle (swap embedder,
//! tune RELEVANCE_WEIGHT, change deliberation prompt, perturb world_state),
//! and DIFF the traces. The diff IS the clue into recall-quality work
//! (Joel: "#2 gives us clues into #1"):
//!
//! - if RELEVANCE_WEIGHT change shifts which memory wins phase-1 attention,
//!   AND the assembled context_broadcast shifts as a result,
//!   AND the deliberation's Decision text shifts in response,
//!   then the recall change DID propagate through to behavior;
//! - if recall picks differ but the Decision is identical, the LLM isn't
//!   grounding on the recalled context — exposes a prompt-tuning gap, not
//!   a recall-quality gap.
//!
//! This module is the bench harness — primitives the caller composes:
//! [`LastTraceCaptureSink`] (capture a single replay's trace), [`diff_traces`]
//! (compute a structured diff between two traces). The MUTATION is the
//! caller's responsibility (construct a new `WorkspaceCycle` differently);
//! the bench has no knowledge of WHAT changed, only HOW the change shows up
//! across the two traces. That keeps the harness orthogonal to faculties.
//!
//! ## Scope discipline
//!
//! This slice lands the DIFF primitives + the single-replay capture sink.
//! Out of scope (intentional follow-ups, named for the next slice):
//!
//! - On-disk trace persistence (JSONL recording sink — same shape as
//!   `RagCaptureSink`'s `JsonlRagCaptureSink`)
//! - Typed mutation helpers (`RelevanceWeightMutation`, `EmbedderSwapMutation`
//!   etc.) — these need M5's `RELEVANCE_WEIGHT` exposed as a configurable
//!   field on `RecallFaculty`; for slice 1 the caller hand-constructs the
//!   mutated `WorkspaceCycle`.
//! - The cross-tab "bench-of-benches" comparing Mac vs M5 results — that
//!   needs the cross-grid embedding lane (slice 3) wired and is way later.

use crate::cognition::workspace::{
    Contribution, Decision, FacultyId, WorkspaceCaptureSink, WorkspaceTrace,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ─── Capture sink ────────────────────────────────────────────────────────

/// Captures the most recent trace into a thread-safe slot. Use one per
/// replay: install on the mutated [`crate::cognition::workspace::WorkspaceCycle`],
/// run it, [`Self::take`] to retrieve the trace.
///
/// Per-replay instances mean the caller never confuses traces between
/// replays (which would defeat the diff). The `take()` API also prevents
/// silent re-use of a stale trace.
pub struct LastTraceCaptureSink {
    slot: Mutex<Option<WorkspaceTrace>>,
}

impl LastTraceCaptureSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            slot: Mutex::new(None),
        })
    }

    /// Retrieve the captured trace, consuming it. Returns `None` if no
    /// trace has been recorded (the cycle wasn't run, or the sink was
    /// already drained).
    pub fn take(&self) -> Option<WorkspaceTrace> {
        self.slot.lock().expect("poisoned").take()
    }

    /// Peek without consuming. Useful for assertions; prefer [`Self::take`]
    /// for normal flow so reuse can't accidentally diff stale traces.
    pub fn peek(&self) -> Option<WorkspaceTrace> {
        self.slot.lock().expect("poisoned").clone()
    }
}

impl Default for LastTraceCaptureSink {
    fn default() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }
}

impl WorkspaceCaptureSink for LastTraceCaptureSink {
    fn record(&self, trace: &WorkspaceTrace) {
        *self.slot.lock().expect("poisoned") = Some(trace.clone());
    }
}

// ─── Diff primitives ─────────────────────────────────────────────────────

/// What changed for a single (faculty, content) pair across two traces.
#[derive(Debug, Clone)]
pub enum BidChange {
    /// Bid present in original but not replay (replay didn't surface it).
    OnlyOriginal(Contribution),
    /// Bid present in replay but not original (replay newly surfaced it —
    /// e.g., mutation gave a faculty different inputs).
    OnlyReplay(Contribution),
    /// Same (faculty, content) bid present in both; salience changed by
    /// more than [`SALIENCE_EPSILON`]. Captures bid-level intensity drift.
    SalienceChanged {
        faculty: FacultyId,
        /// First 80 chars of the content for diff identity in logs.
        content_excerpt: String,
        original_salience: f32,
        replay_salience: f32,
    },
}

/// Salience deltas below this threshold are treated as equivalent — both
/// floating-point noise and meaningless tweaks (the bench is for signal,
/// not microscopic perturbation).
pub const SALIENCE_EPSILON: f32 = 0.001;

/// Structured difference between an original trace and a replayed trace.
#[derive(Debug, Clone)]
pub struct WorkspaceTraceDiff {
    pub original: WorkspaceTrace,
    pub replay: WorkspaceTrace,
    /// All bid changes across BOTH phases (perception + deliberation).
    pub bid_changes: Vec<BidChange>,
    /// Did the assembled context (what the decider saw) differ?
    pub context_broadcast_changed: bool,
    /// Did the final Decision differ? (Variant change OR text change.)
    pub decision_changed: bool,
}

impl WorkspaceTraceDiff {
    /// Quick yes/no: did this mutation propagate to the persona's
    /// behaviour, or was it a silent change at intermediate layers?
    pub fn behaviorally_significant(&self) -> bool {
        self.decision_changed
    }

    /// Did the mutation affect what the deliberation faculty actually
    /// reasoned over? Important for distinguishing "recall changed but
    /// decision didn't" (prompt gap) from "nothing changed" (mutation
    /// was a no-op).
    pub fn context_significant(&self) -> bool {
        self.context_broadcast_changed
    }
}

/// Compute the structured diff between two traces. Inputs are CONSUMED
/// (moved into the returned diff) so callers can't accidentally diff
/// stale data; the trace clones live inside [`WorkspaceTraceDiff`].
pub fn diff_traces(original: WorkspaceTrace, replay: WorkspaceTrace) -> WorkspaceTraceDiff {
    let bid_changes = compute_bid_changes(&original.bids, &replay.bids);
    let context_broadcast_changed =
        !contributions_equiv(&original.context_broadcast, &replay.context_broadcast);
    let decision_changed = !decisions_equiv(&original.decision, &replay.decision);
    WorkspaceTraceDiff {
        original,
        replay,
        bid_changes,
        context_broadcast_changed,
        decision_changed,
    }
}

fn compute_bid_changes(orig: &[Contribution], rep: &[Contribution]) -> Vec<BidChange> {
    // Key by (faculty.as_str(), content) — FacultyId doesn't derive Hash
    // (it has a `Custom(String)` variant); the structural `as_str()`
    // projection gives the same identity for the same faculty.
    let orig_keyed: HashMap<(&str, &str), &Contribution> = orig
        .iter()
        .map(|c| ((c.faculty.as_str(), c.content.as_str()), c))
        .collect();
    let rep_keyed: HashMap<(&str, &str), &Contribution> = rep
        .iter()
        .map(|c| ((c.faculty.as_str(), c.content.as_str()), c))
        .collect();

    let mut changes = Vec::new();
    for (key, orig_c) in &orig_keyed {
        match rep_keyed.get(key) {
            None => changes.push(BidChange::OnlyOriginal((*orig_c).clone())),
            Some(rep_c) => {
                if (orig_c.salience - rep_c.salience).abs() > SALIENCE_EPSILON {
                    changes.push(BidChange::SalienceChanged {
                        faculty: orig_c.faculty.clone(),
                        content_excerpt: truncate_excerpt(&orig_c.content, 80),
                        original_salience: orig_c.salience,
                        replay_salience: rep_c.salience,
                    });
                }
            }
        }
    }
    for (key, rep_c) in &rep_keyed {
        if !orig_keyed.contains_key(key) {
            changes.push(BidChange::OnlyReplay((*rep_c).clone()));
        }
    }
    changes
}

fn contributions_equiv(a: &[Contribution], b: &[Contribution]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Order matters in broadcast (it's the spotlight ordering by salience),
    // so equivalence requires same sequence by (faculty, content).
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| x.faculty == y.faculty && x.content == y.content)
}

fn decisions_equiv(a: &Option<Decision>, b: &Option<Decision>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Decision::Pass), Some(Decision::Pass)) => true,
        (Some(Decision::Speak { text: ta }), Some(Decision::Speak { text: tb })) => ta == tb,
        (
            Some(Decision::RaiseUnprompted { text: ta }),
            Some(Decision::RaiseUnprompted { text: tb }),
        ) => ta == tb,
        _ => false,
    }
}

fn truncate_excerpt(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = s.chars().take(max_chars).collect::<String>();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::workspace::Contribution;

    fn ctx(faculty: FacultyId, content: &str, salience: f32) -> Contribution {
        Contribution::context(faculty, content, salience, "test")
    }

    fn verdict(decision: Decision) -> Contribution {
        Contribution::verdict(decision, 0.9, "test")
    }

    fn trace(
        world_state: &str,
        bids: Vec<Contribution>,
        context_broadcast: Vec<Contribution>,
        broadcast: Vec<Contribution>,
        decision: Option<Decision>,
    ) -> WorkspaceTrace {
        WorkspaceTrace {
            world_state: world_state.to_string(),
            bids,
            context_broadcast,
            broadcast,
            decision,
        }
    }

    #[test]
    fn diff_detects_only_original_bid() {
        let original = trace(
            "ws",
            vec![ctx(FacultyId::Recall, "remember A", 0.5)],
            vec![ctx(FacultyId::Recall, "remember A", 0.5)],
            vec![ctx(FacultyId::Recall, "remember A", 0.5)],
            Some(Decision::Pass),
        );
        let replay = trace(
            "ws",
            vec![],
            vec![],
            vec![],
            Some(Decision::Pass),
        );
        let d = diff_traces(original, replay);
        assert_eq!(d.bid_changes.len(), 1);
        match &d.bid_changes[0] {
            BidChange::OnlyOriginal(c) => assert_eq!(c.content, "remember A"),
            other => panic!("expected OnlyOriginal, got {other:?}"),
        }
    }

    #[test]
    fn diff_detects_only_replay_bid() {
        let original = trace("ws", vec![], vec![], vec![], Some(Decision::Pass));
        let replay = trace(
            "ws",
            vec![ctx(FacultyId::Recall, "newly surfaced", 0.7)],
            vec![],
            vec![],
            Some(Decision::Pass),
        );
        let d = diff_traces(original, replay);
        assert_eq!(d.bid_changes.len(), 1);
        match &d.bid_changes[0] {
            BidChange::OnlyReplay(c) => assert_eq!(c.content, "newly surfaced"),
            other => panic!("expected OnlyReplay, got {other:?}"),
        }
    }

    #[test]
    fn diff_detects_salience_change_above_epsilon() {
        let original = trace(
            "ws",
            vec![ctx(FacultyId::Recall, "same content", 0.3)],
            vec![],
            vec![],
            Some(Decision::Pass),
        );
        let replay = trace(
            "ws",
            vec![ctx(FacultyId::Recall, "same content", 0.8)],
            vec![],
            vec![],
            Some(Decision::Pass),
        );
        let d = diff_traces(original, replay);
        assert_eq!(d.bid_changes.len(), 1);
        match &d.bid_changes[0] {
            BidChange::SalienceChanged {
                original_salience,
                replay_salience,
                ..
            } => {
                assert!((*original_salience - 0.3).abs() < 1e-6);
                assert!((*replay_salience - 0.8).abs() < 1e-6);
            }
            other => panic!("expected SalienceChanged, got {other:?}"),
        }
    }

    #[test]
    fn diff_ignores_salience_jitter_under_epsilon() {
        let original = trace(
            "ws",
            vec![ctx(FacultyId::Recall, "same", 0.5)],
            vec![],
            vec![],
            Some(Decision::Pass),
        );
        let replay = trace(
            "ws",
            vec![ctx(FacultyId::Recall, "same", 0.5 + SALIENCE_EPSILON / 2.0)],
            vec![],
            vec![],
            Some(Decision::Pass),
        );
        let d = diff_traces(original, replay);
        assert!(
            d.bid_changes.is_empty(),
            "jitter below epsilon shouldn't show as a change: {:?}",
            d.bid_changes
        );
    }

    #[test]
    fn diff_decision_changed_when_speak_text_differs() {
        let original = trace(
            "ws",
            vec![],
            vec![],
            vec![],
            Some(Decision::Speak {
                text: "hello".into(),
            }),
        );
        let replay = trace(
            "ws",
            vec![],
            vec![],
            vec![],
            Some(Decision::Speak {
                text: "hi there".into(),
            }),
        );
        let d = diff_traces(original, replay);
        assert!(d.decision_changed);
        assert!(d.behaviorally_significant());
    }

    #[test]
    fn diff_context_changed_when_broadcast_picks_differ() {
        let original = trace(
            "ws",
            vec![],
            vec![ctx(FacultyId::Recall, "memory A", 0.7)],
            vec![],
            None,
        );
        let replay = trace(
            "ws",
            vec![],
            vec![ctx(FacultyId::Recall, "memory B", 0.8)],
            vec![],
            None,
        );
        let d = diff_traces(original, replay);
        assert!(d.context_broadcast_changed);
        assert!(d.context_significant());
    }

    #[test]
    fn diff_no_change_when_traces_equivalent() {
        let bids = vec![ctx(FacultyId::Recall, "same", 0.5)];
        let original = trace(
            "ws",
            bids.clone(),
            bids.clone(),
            bids.clone(),
            Some(Decision::Pass),
        );
        let replay = trace("ws", bids.clone(), bids.clone(), bids, Some(Decision::Pass));
        let d = diff_traces(original, replay);
        assert!(d.bid_changes.is_empty());
        assert!(!d.context_broadcast_changed);
        assert!(!d.decision_changed);
        assert!(!d.behaviorally_significant());
    }

    #[test]
    fn last_trace_capture_sink_round_trip() {
        let sink = LastTraceCaptureSink::new();
        let t = trace(
            "ws",
            vec![],
            vec![],
            vec![ctx(FacultyId::Recall, "x", 0.5), verdict(Decision::Pass)],
            Some(Decision::Pass),
        );
        sink.record(&t);
        let taken = sink.take().expect("should have a trace");
        assert_eq!(taken.broadcast.len(), 2);
        // take() consumes — second take returns None.
        assert!(sink.take().is_none());
    }
}
