//! Working memory — the persona's recent chain-of-thought, fed forward across turns.
//!
//! ## Why this exists (two memory tiers, not one)
//!
//! The reasoning-model adapter now SEPARATES a turn's `<think>` into
//! `TextGenerationResponse.reasoning` ([[ai::openai_adapter::extract_reasoning]]) —
//! captured, never leaked to the room. This module is the CONSUME side: a
//! short-lived scratchpad of "how I just thought," fed into the next turn so the
//! deliberator can pick up its own train of thought.
//!
//! This is deliberately a SEPARATE tier from the long-term engram store
//! (`AdmissionState` / `engrams.sqlite`):
//!
//! - **Working memory (here):** volatile, rolling last-N reasonings AND actions, fed
//!   forward, aged out. The persona's "now I'm thinking about… and here's what my
//!   hands just did". Lost on restart — it's scratch, not self.
//! - **Long-term memory (engrams):** curated turns/facts, persisted, the durable
//!   self ([[persona-persistence-self-determination]]).
//!
//! Raw chain-of-thought is NOT admitted as an engram on purpose: a small reasoning
//! model's CoT rambles (it would pollute relevance recall), and it would contaminate
//! the LoRA training pairs (the coordination↔learning flywheel wants clean ShareGPT
//! turns, not raw CoT). Keep the tiers separate — the human working-vs-long-term split.
//!
//! ## How it wires into the Workspace brain
//!
//! - [`WorkingMemoryFaculty`] is a PERCEPTION-tier faculty: each tick it bids the
//!   recent reasoning into the workspace so the phase-2 deliberator conditions on it.
//! - The deliberation faculty WRITES its reasoning here after producing a verdict
//!   (the thing that thinks records its thinking). Read at phase 1, written at phase
//!   2 → next tick reads the updated buffer. That's the rolling working memory.
//!
//! ## Pairs with the thinking toggle
//!
//! When thinking is suppressed (`ThinkingMode::Suppress`, the unsloth-gateway
//! default), `reasoning` is `None` → nothing is recorded → the faculty abstains. So
//! working memory SELF-ACTIVATES only where thinking is enabled — "reason when it
//! helps, remember what you reasoned." No config flag needed.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use super::workspace::{Contribution, Faculty, FacultyId, Workspace};

/// How many recent reasoning traces to carry forward. Small — working memory is a
/// scratchpad, not a log; older thinking ages out (rolling).
pub const DEFAULT_WORKING_MEMORY_CAPACITY: usize = 3;

/// The faculty's bid salience. Same tier as retrieved grounding
/// (`RETRIEVED_SALIENCE` = 0.5): useful context that informs the decision but must
/// not outrank standing framing (roster/doctrine, 0.9) or a strong recall hit.
const WORKING_MEMORY_SALIENCE: f32 = 0.5;

/// A bounded, rolling buffer of the persona's recent reasoning. Cheap to clone the
/// `Arc`; shared between the writer (deliberation faculty) and the reader
/// (`WorkingMemoryFaculty`). `parking_lot::Mutex` — no poisoning, and every access
/// is a quick sync snapshot/push with no `.await` held across the lock.
#[derive(Debug)]
pub struct WorkingMemory {
    capacity: usize,
    entries: Mutex<VecDeque<String>>,
    /// Monotonic stamp applied to ACTION entries. Two reasons: (1) it makes each
    /// recorded action a DISTINCT string even when the persona repeats the identical
    /// tool call — so the working-memory window the perception faculty bids next tick
    /// CHANGES across a repeat instead of being byte-identical. Under greedy decoding
    /// a stationary perception re-emits the stationary verdict forever; a sliding
    /// window of stamped actions is what lets the mind notice "I keep doing this" and
    /// break out organically. (2) it reads as a recency ordinal in the bid. Not used
    /// for reasoning entries (chain-of-thought already varies turn to turn).
    next_action_seq: AtomicU64,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Mutex::new(VecDeque::new()),
            next_action_seq: AtomicU64::new(1),
        }
    }

    /// Record the reasoning the persona just produced. Blank/empty is ignored
    /// (suppressed-thinking turns record nothing). Oldest ages out past capacity.
    pub fn record(&self, reasoning: &str) {
        let r = reasoning.trim();
        if r.is_empty() {
            return;
        }
        let mut e = self.entries.lock();
        e.push_back(r.to_string());
        while e.len() > self.capacity {
            e.pop_front();
        }
    }

    /// Record an ACTION the persona's hands just took (a tool call + its result
    /// head) — proprioception, NOT chain-of-thought. This fires regardless of the
    /// thinking toggle: an act HAPPENED whether or not the model emitted `<think>`,
    /// and the mind must perceive its own hands to avoid re-issuing the identical
    /// act blind ([[act-results-need-a-recency-channel-not-semantic-recall]]). Each
    /// entry is stamped with a monotonic `#n` so a repeated identical act still
    /// changes the perception window (see `next_action_seq`). Oldest ages out past
    /// capacity, same rolling scratchpad as reasoning.
    pub fn record_action(&self, action: &str) {
        let a = action.trim();
        if a.is_empty() {
            return;
        }
        let seq = self.next_action_seq.fetch_add(1, Ordering::Relaxed);
        let mut e = self.entries.lock();
        e.push_back(format!("[action #{seq}] {a}"));
        while e.len() > self.capacity {
            e.pop_front();
        }
    }

    /// Snapshot of recent reasonings, oldest → newest.
    pub fn recent(&self) -> Vec<String> {
        self.entries.lock().iter().cloned().collect()
    }

    /// Drop all volatile traces — the scratchpad goes dark. Used at the boundary
    /// between genuinely disjoint concerns (e.g. each independent task in a
    /// `cognition/eval` pass, presented back-to-back by a grader with no temporal
    /// continuity): carrying the prior concern's proprioception into the next would
    /// be contamination, not memory. The monotonic action stamp is NOT reset, so
    /// stamps never collide across a clear (a stale reference can't alias a fresh
    /// act). The live heartbeat never calls this — there, concerns flow
    /// continuously and old traces age out naturally past `capacity`.
    pub fn clear(&self) {
        self.entries.lock().clear();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

/// Perception-tier faculty that bids the persona's recent reasoning into the
/// workspace — so the deliberator can resume its own train of thought rather than
/// re-deriving it cold each turn. Abstains when there's nothing recorded (first
/// turns, or thinking suppressed).
pub struct WorkingMemoryFaculty {
    memory: Arc<WorkingMemory>,
}

impl WorkingMemoryFaculty {
    pub fn new(memory: Arc<WorkingMemory>) -> Self {
        Self { memory }
    }

    fn faculty_id() -> FacultyId {
        FacultyId::Custom("working-memory".to_string())
    }
}

#[async_trait]
impl Faculty for WorkingMemoryFaculty {
    fn id(&self) -> FacultyId {
        Self::faculty_id()
    }

    // Perception tier (default): reacts to the raw world-state, bidding the recent
    // reasoning into phase 1 so the deliberator conditions on it in phase 2.
    async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
        let recent = self.memory.recent();
        if recent.is_empty() {
            return None;
        }
        // Label each trace by how many turns back it was (newest = -1), so the model
        // reads them as a recency-ordered scratchpad, not standing fact.
        let n = recent.len();
        let body = recent
            .iter()
            .enumerate()
            .map(|(i, r)| format!("- (turn -{}) {}", n - i, r))
            .collect::<Vec<_>>()
            .join("\n");
        Some(Contribution::context(
            Self::faculty_id(),
            format!("Your recent thoughts and actions (working memory):\n{body}"),
            WORKING_MEMORY_SALIENCE,
            format!("working memory: {n} recent trace(s) carried forward"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: record/recent round-trips, blank reasoning is ignored, and
    // the buffer ROLLS at capacity (oldest dropped) — it's a scratchpad, not a log.
    #[test]
    fn records_recent_and_rolls_at_capacity() {
        let wm = WorkingMemory::new(2);
        assert!(wm.is_empty());
        wm.record("first thought");
        wm.record("   "); // blank → ignored
        wm.record("second thought");
        assert_eq!(wm.recent(), vec!["first thought", "second thought"]);
        // third pushes out the oldest (capacity 2).
        wm.record("third thought");
        assert_eq!(wm.recent(), vec!["second thought", "third thought"]);
    }

    // what this catches: THE loop-break property. A repeated identical action must
    // produce DISTINCT working-memory entries (monotonic #n stamp) so the perception
    // window the faculty bids changes across a repeat — otherwise greedy decode
    // re-emits the identical Act forever (the deterministic acting loop that the
    // engram dedup + suppressed-thinking left invisible). Regression for
    // [[act-results-need-a-recency-channel-not-semantic-recall]].
    #[test]
    fn repeated_action_yields_distinct_stamped_entries() {
        let wm = WorkingMemory::new(3);
        wm.record_action("I ran code/search(pattern=foo) -> 0 matches");
        wm.record_action("I ran code/search(pattern=foo) -> 0 matches"); // identical act
        let recent = wm.recent();
        assert_eq!(recent.len(), 2);
        assert_ne!(
            recent[0], recent[1],
            "identical repeated act must read DISTINCT in working memory"
        );
        assert!(recent[0].starts_with("[action #1] "));
        assert!(recent[1].starts_with("[action #2] "));
        // blank action ignored (no fabricated proprioception).
        wm.record_action("   ");
        assert_eq!(wm.recent().len(), 2);
    }

    // what this catches: clear() empties the volatile scratch (disjoint-concern
    // boundary, e.g. between eval tasks) but does NOT reset the monotonic action
    // stamp — so a post-clear act can never alias a pre-clear stamp.
    #[test]
    fn clear_empties_buffer_but_keeps_stamps_monotonic() {
        let wm = WorkingMemory::new(3);
        wm.record_action("ran A");
        wm.record_action("ran B");
        assert_eq!(wm.recent().len(), 2);
        wm.clear();
        assert!(wm.is_empty(), "clear drops the disjoint concern's traces");
        wm.record_action("ran C");
        let recent = wm.recent();
        assert_eq!(recent.len(), 1);
        assert!(
            recent[0].starts_with("[action #3] "),
            "stamp stays monotonic across clear (no alias with pre-clear #1/#2), got {:?}",
            recent[0]
        );
    }

    // what this catches: capacity is floored at 1 — a 0 never wedges the buffer into
    // dropping everything.
    #[test]
    fn capacity_floored_at_one() {
        let wm = WorkingMemory::new(0);
        wm.record("a");
        wm.record("b");
        assert_eq!(wm.recent(), vec!["b"], "floor-1 buffer keeps the latest");
    }

    // what this catches: the faculty ABSTAINS with no recorded reasoning (first
    // turns / suppressed thinking) — it never bids empty noise into the workspace.
    #[tokio::test]
    async fn faculty_abstains_when_empty() {
        let wm = Arc::new(WorkingMemory::new(3));
        let faculty = WorkingMemoryFaculty::new(wm);
        let ws = Workspace::new("a fresh burst");
        assert!(faculty.contribute(&ws).await.is_none());
    }

    // what this catches: once reasoning is recorded, the faculty bids it as
    // recency-labeled context at the working-memory salience tier — the reasoning the
    // deliberator will see next tick.
    #[tokio::test]
    async fn faculty_bids_recent_reasoning_as_context() {
        let wm = Arc::new(WorkingMemory::new(3));
        wm.record("I should check the room roster first.");
        let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
        let ws = Workspace::new("a fresh burst");
        let c = faculty.contribute(&ws).await.expect("bids when non-empty");
        assert_eq!(c.faculty, FacultyId::Custom("working-memory".to_string()));
        assert!((c.salience - WORKING_MEMORY_SALIENCE).abs() < f32::EPSILON);
        assert!(c.content.contains("check the room roster"));
        assert!(c.content.contains("turn -1"), "newest trace labeled -1");
        assert!(c.decision.is_none(), "perception bid carries no decision");
    }
}
