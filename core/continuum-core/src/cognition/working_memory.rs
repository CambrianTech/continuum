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

/// Prefix on a SETTLEMENT entry — the proprioceptive mark that the persona produced
/// an utterance (answered) and thereby closed the current concern. It is a boundary
/// in the volatile buffer: an identical tool call BEFORE the most recent settlement
/// belongs to a concern she already answered, so re-issuing it for a NEW concern is
/// legitimate, not a spin. Shared here (not inlined at the two call sites) so the
/// writer (`record_settlement`) and the reader (`act_observe`'s repeat-perception
/// scope) can never drift. See [[persona-tool-loop-act-then-report]].
pub const WM_SETTLEMENT_PREFIX: &str = "[settled]";

/// The faculty's bid salience. Same tier as retrieved grounding
/// (`RETRIEVED_SALIENCE` = 0.5): useful context that informs the decision but must
/// not outrank standing framing (roster/doctrine, 0.9) or a strong recall hit.
const WORKING_MEMORY_SALIENCE: f32 = 0.5;

/// How much of a tool result's HEAD is kept in the rolling recency trail (older acts +
/// the proprioceptive "I did #n X" stamp). The LATEST act is kept in FULL separately
/// (`last_action`) so the mind can actually work with what its hands just fetched — count
/// a file, read a screenshot description, scan a log tail — not a truncated stub. Was
/// starving live agents: they saw only the head of their own tool results and looped
/// ("I'll read it again") because the data never came back. [[act-results-need-a-recency-channel-not-semantic-recall]]
pub const WM_ACTION_HEAD_CHARS: usize = 800;

/// A bounded, rolling buffer of the persona's recent reasoning. Cheap to clone the
/// `Arc`; shared between the writer (deliberation faculty) and the reader
/// (`WorkingMemoryFaculty`). `parking_lot::Mutex` — no poisoning, and every access
/// is a quick sync snapshot/push with no `.await` held across the lock.
#[derive(Debug)]
pub struct WorkingMemory {
    capacity: usize,
    entries: Mutex<VecDeque<String>>,
    /// The FULL result of the most recent act, kept whole (bounded upstream by the
    /// executor's fold cap) so the mind can work with what its hands just fetched — a
    /// file to count, a screenshot description to read, a log to scan. Only the latest is
    /// full; older acts survive as heads in `entries`. `(seq, result)`; the `seq` matches
    /// the `[action #n]` stamp in the trail so the mind can tie the full result to the
    /// proprioceptive entry. An ASYNC result (a dispatched sentinel/debugger/compilation
    /// completing later) feeds this SAME slot on its completion event — one channel, sync
    /// or async ([[act-results-need-a-recency-channel-not-semantic-recall]]).
    last_action: Mutex<Option<(u64, String)>>,
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
            last_action: Mutex::new(None),
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
    pub fn record_action(&self, result: &str) {
        let a = result.trim();
        if a.is_empty() {
            return;
        }
        let seq = self.next_action_seq.fetch_add(1, Ordering::Relaxed);
        // Keep the LATEST result in FULL so the mind can work with it next turn (count a
        // file, read a screenshot description, scan a log). Overwrites the prior latest —
        // older acts survive only as heads in the trail below.
        *self.last_action.lock() = Some((seq, a.to_string()));
        // Head into the rolling recency trail: proprioception ("I did #n X") + the
        // repeat-break signal. Truncation lives HERE now (one place), so callers pass the
        // full result and never pre-truncate. Append-only + `#seq`-stamped keeps the
        // trail's KV-cache prefix byte-stable across a settle-act.
        let head: String = a.chars().take(WM_ACTION_HEAD_CHARS).collect();
        let mut e = self.entries.lock();
        e.push_back(format!("[action #{seq}] {head}"));
        while e.len() > self.capacity {
            e.pop_front();
        }
    }

    /// The FULL result of the most recent act, with its `#seq` stamp — `None` before any
    /// act or after a `clear`. The perception faculty surfaces this so the mind sees the
    /// whole of what its hands just fetched, not the truncated trail head.
    pub fn last_action_full(&self) -> Option<(u64, String)> {
        self.last_action.lock().clone()
    }

    /// Record that the persona SETTLED — produced an utterance and closed the current
    /// concern. Pushes a [`WM_SETTLEMENT_PREFIX`]-marked boundary into the same rolling
    /// buffer (honest proprioception: "I already answered X", perceivable by the mind
    /// next tick). The boundary lets the repeat-perception guard distinguish a spin
    /// (identical call re-issued WITHIN one settling, before any answer) from a
    /// legitimate re-use of the same tool for a genuinely NEW concern after an answer.
    /// Blank is ignored. Oldest ages out past capacity, same rolling scratchpad.
    pub fn record_settlement(&self, answer_head: &str) {
        let a = answer_head.trim();
        let mut e = self.entries.lock();
        e.push_back(if a.is_empty() {
            WM_SETTLEMENT_PREFIX.to_string()
        } else {
            format!("{WM_SETTLEMENT_PREFIX} I answered: {a}")
        });
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
        *self.last_action.lock() = None; // the full latest result is proprioception too
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
        // Render oldest-first, newest-LAST: position alone carries recency (the last
        // line is the most recent thought), the universal chat-history convention.
        //
        // We deliberately do NOT prefix each trace with a relative "(turn -N)" label.
        // That offset rewrites every act (what was -1 becomes -2 when a new trace
        // appends), which mutates the whole block from its first byte — so the
        // KV-cache prefix breaks at [working-memory] on every settle-act and the
        // entire dynamic tail re-prefills (measured: ~538 tokens re-prefilled per
        // act, the dominant within-task prefill cost). Append-only formatting keeps
        // prior entries byte-identical, so a settle-act re-prefills only its one new
        // trace + the user turn. Each trace already carries its own stable absolute
        // marker (e.g. "[action #41]"); recency needs no per-act-mutating annotation.
        let n = recent.len();
        let body = recent
            .iter()
            .map(|r| format!("- {r}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut text = format!("Your recent thoughts and actions (working memory):\n{body}");
        // The FULL result of the most recent act, appended AFTER the append-only trail:
        // the stable prefix keeps its KV-cache, only this fresh block re-prefills (it is
        // new each act regardless). This is what lets the mind actually USE what its hands
        // fetched — count the file, read the screenshot description, scan the log — instead
        // of looping on the truncated head. Only shown when it carries more than the trail
        // head already does, so short results don't duplicate.
        if let Some((seq, full)) = self.memory.last_action_full() {
            if full.chars().count() > WM_ACTION_HEAD_CHARS {
                text.push_str(&format!(
                    "\n\nFull result of your most recent action (#{seq}):\n{full}"
                ));
            }
        }
        Some(Contribution::context(
            Self::faculty_id(),
            text,
            WORKING_MEMORY_SALIENCE,
            format!("working memory: {n} recent trace(s) carried forward"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE starvation fix — a large tool result comes back to the mind
    // in FULL (so it can count/read/scan it), while the rolling trail keeps only the head
    // (byte-stable proprioception). Older acts drop to head-only; a fresh act's full result
    // replaces the prior. This is why a persona reading a 130-line file can now answer about
    // the whole file instead of looping on the doc-comment head.
    #[tokio::test]
    async fn latest_action_returns_full_result_trail_keeps_head() {
        let wm = Arc::new(WorkingMemory::new(8));
        let big = format!("line0\n{}", "x".repeat(5_000)); // > WM_ACTION_HEAD_CHARS
        wm.record_action(&big);

        // The full result is available whole.
        let (seq, full) = wm.last_action_full().expect("latest act kept");
        assert_eq!(seq, 1);
        assert_eq!(full, big, "the mind gets the WHOLE result, not a truncated stub");

        // The rolling trail carries only the head (KV-stable proprioception).
        let trail = wm.recent();
        assert_eq!(trail.len(), 1);
        assert!(trail[0].starts_with("[action #1]"));
        assert!(
            trail[0].chars().count() < big.chars().count(),
            "trail entry is head-truncated, not the whole result"
        );

        // The faculty surfaces the full result as its own block.
        let rendered = WorkingMemoryFaculty::new(wm.clone())
            .contribute(&Workspace::new("a fresh burst"))
            .await
            .expect("bids")
            .content
            .clone();
        assert!(
            rendered.contains("Full result of your most recent action (#1):"),
            "the whole result reaches the mind"
        );
        assert!(rendered.contains(&"x".repeat(5_000)), "and it's the FULL body");

        // A second act replaces the full slot; the first survives only as a trail head.
        wm.record_action("small follow-up");
        let (seq2, _) = wm.last_action_full().unwrap();
        assert_eq!(seq2, 2, "latest-full tracks the most recent act");
    }

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

    // what this catches: record_settlement lays down a WM_SETTLEMENT_PREFIX boundary
    // in the rolling buffer (with the answer head as honest "I already answered X"
    // proprioception), and a blank answer still marks the boundary. This is the marker
    // the repeat-perception guard scopes to, separating a spin (identical call before
    // any answer) from legitimate re-use of the same tool after answering.
    #[test]
    fn record_settlement_marks_a_boundary_in_the_buffer() {
        let wm = WorkingMemory::new(4);
        wm.record_action("I ran commands/list({}) -> 100 commands");
        wm.record_settlement("there are 100 commands");
        let recent = wm.recent();
        assert_eq!(recent.len(), 2);
        assert!(
            recent[1].starts_with(WM_SETTLEMENT_PREFIX),
            "the settlement is marked with the shared prefix so the guard can find it"
        );
        assert!(
            recent[1].contains("there are 100 commands"),
            "carries the answer head as proprioception"
        );
        // A blank answer still marks the boundary (bare prefix, no fabricated text).
        wm.record_settlement("   ");
        let recent = wm.recent();
        assert_eq!(recent.last().unwrap(), WM_SETTLEMENT_PREFIX);
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
    // context at the working-memory salience tier — the reasoning the deliberator
    // will see next tick. Newest is rendered LAST (positional recency).
    #[tokio::test]
    async fn faculty_bids_recent_reasoning_as_context() {
        let wm = Arc::new(WorkingMemory::new(3));
        wm.record("I should check the room roster first.");
        wm.record("Now I will read the latest message.");
        let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
        let ws = Workspace::new("a fresh burst");
        let c = faculty.contribute(&ws).await.expect("bids when non-empty");
        assert_eq!(c.faculty, FacultyId::Custom("working-memory".to_string()));
        assert!((c.salience - WORKING_MEMORY_SALIENCE).abs() < f32::EPSILON);
        assert!(c.content.contains("check the room roster"));
        // newest trace is LAST (position carries recency, no relative label).
        let oldest_at = c.content.find("room roster").unwrap();
        let newest_at = c.content.find("latest message").unwrap();
        assert!(newest_at > oldest_at, "newest trace rendered last");
        assert!(c.decision.is_none(), "perception bid carries no decision");
    }

    // what this catches: the KV-cache-locality invariant this formatter exists to
    // hold — appending a new trace must NOT rewrite the rendering of prior traces.
    // (The old "(turn -N)" relative label broke this: every prior entry's offset
    // shifted on each append, mutating the whole block from its first byte and
    // forcing a full re-prefill of the dynamic tail every settle-act.)
    #[tokio::test]
    async fn prior_traces_render_byte_identical_after_append() {
        let wm = Arc::new(WorkingMemory::new(8));
        wm.record("first");
        wm.record("second");
        let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
        let ws = Workspace::new("burst");
        let before = faculty.contribute(&ws).await.unwrap().content;
        wm.record("third");
        let after = faculty.contribute(&ws).await.unwrap().content;
        // the older render is a strict PREFIX of the newer one — pure append.
        assert!(
            after.starts_with(&before),
            "appending a trace must leave prior traces byte-identical (cacheable prefix);\n  before={before:?}\n  after={after:?}"
        );
    }
}
