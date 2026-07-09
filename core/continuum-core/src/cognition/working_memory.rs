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

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

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

/// Max distinct dispatched (background) handles tracked at once. A persona with more than
/// this many sentinels/compiles/debuggers in flight is unusual; past the cap the
/// oldest-updated finished handle is evicted first (a still-running one is never dropped).
const MAX_DISPATCHED: usize = 16;

/// Lifecycle of a dispatched (background) command the persona sent away — a sentinel,
/// a compile, a debugger. Streams continuously: `Running` updates arrive in place, then a
/// terminal `Done`/`Failed`. The handle (a UUID) is reusable — the mind can pass it to
/// another command (cancel, query, attach) in a later turn ([[commands-are-agency-algs-are-pathways]]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchStatus {
    Running,
    Done,
    Failed,
}

/// One in-flight (or freshly-finished) dispatched command, keyed by handle in
/// `WorkingMemory::dispatched`. The `latest` is the newest streamed content (progress line
/// or final result), updated IN PLACE so continuous progress doesn't spam the trail.
#[derive(Debug, Clone)]
struct DispatchedAction {
    label: String,
    latest: String,
    status: DispatchStatus,
    /// Recency ordinal (shared monotonic stamp) — orders handles + picks the eviction victim.
    seq: u64,
}

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
    /// In-flight + freshly-finished dispatched commands, keyed by handle (UUID). The
    /// SAME recency channel as `last_action`, but keyed so concurrent sentinels/compiles/
    /// debuggers stream in without clobbering each other. Fed by the completion/progress
    /// listener (`record_dispatch_event`); surfaced by the faculty so the mind perceives
    /// "what I sent away, and where it's at" each heartbeat. Bounded by `MAX_DISPATCHED`.
    dispatched: Mutex<HashMap<Uuid, DispatchedAction>>,
    /// Monotonic stamp applied to ACTION entries. Two reasons: (1) it makes each
    /// recorded action a DISTINCT string even when the persona repeats the identical
    /// tool call — so the working-memory window the perception faculty bids next tick
    /// CHANGES across a repeat instead of being byte-identical. Under greedy decoding
    /// a stationary perception re-emits the stationary verdict forever; a sliding
    /// window of stamped actions is what lets the mind notice "I keep doing this" and
    /// break out organically. (2) it reads as a recency ordinal in the bid. Not used
    /// for reasoning entries (chain-of-thought already varies turn to turn).
    next_action_seq: AtomicU64,
    /// Fingerprints (tool name + args) of recent ACTIONS — the loop-awareness channel.
    /// Distinct from `entries`, which carries result HEADS that vary turn to turn (so an
    /// identical call still *looks* new); this keys on the CALL alone, so
    /// [`note_action_fingerprint`](Self::note_action_fingerprint) can tell the mind "you have
    /// issued this exact call N times." The `#seq` window-shift alone proved too implicit for
    /// smaller models to interpret — they re-issue the identical call despite the changing
    /// window. Explicit repeat-perception (a true fact about her own hands, never a directive)
    /// lets a looping mind SEE its redundancy and move on organically.
    action_fps: Mutex<VecDeque<String>>,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Mutex::new(VecDeque::new()),
            last_action: Mutex::new(None),
            dispatched: Mutex::new(HashMap::new()),
            next_action_seq: AtomicU64::new(1),
            action_fps: Mutex::new(VecDeque::new()),
        }
    }

    /// Record a fingerprint (tool name + args) of the action the hands just took, and return
    /// how many times THIS exact call now appears in the recent window (including this one).
    /// `≥ 2` means the mind is re-issuing an identical call — proprioception the act→observe
    /// step renders EXPLICITLY so a looping mind perceives its own redundancy and moves on.
    /// Reports a TRUE fact about her hands; it never dictates what to do instead (that would
    /// be steering — [[no-hardcoded-heuristics-to-steer-cognition]]). Bounded by `capacity`,
    /// same rolling window as the recency trail.
    pub fn note_action_fingerprint(&self, fingerprint: &str) -> usize {
        let mut fps = self.action_fps.lock();
        fps.push_back(fingerprint.to_string());
        while fps.len() > self.capacity {
            fps.pop_front();
        }
        fps.iter().filter(|f| f.as_str() == fingerprint).count()
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

    /// Fold a streamed event from a DISPATCHED (background) command into the mind's
    /// recency, keyed by its handle. The async twin of `record_action`: a sentinel /
    /// compile / debugger the persona sent away emits events CONTINUOUSLY — `Running`
    /// progress updates the handle's slot IN PLACE (no trail spam), then a terminal
    /// `Done`/`Failed` marks it. The mind perceives outstanding + freshly-finished handles
    /// via the faculty next heartbeat, so it can act on a result the moment it lands
    /// without ever blocking. `label` is set once (first event) and preserved across
    /// updates. Bounded by `MAX_DISPATCHED` — a finished handle is evicted before a
    /// running one when over cap ([[act-results-need-a-recency-channel-not-semantic-recall]]).
    pub fn record_dispatch_event(
        &self,
        handle: Uuid,
        label: &str,
        content: &str,
        status: DispatchStatus,
    ) {
        let seq = self.next_action_seq.fetch_add(1, Ordering::Relaxed);
        let mut m = self.dispatched.lock();
        let entry = m.entry(handle).or_insert_with(|| DispatchedAction {
            label: label.trim().to_string(),
            latest: String::new(),
            status: DispatchStatus::Running,
            seq,
        });
        entry.latest = content.trim().to_string();
        entry.status = status;
        entry.seq = seq;
        if m.len() > MAX_DISPATCHED {
            // Evict the oldest-updated FINISHED handle; never drop one still running.
            if let Some(victim) = m
                .iter()
                .filter(|(_, a)| a.status != DispatchStatus::Running)
                .min_by_key(|(_, a)| a.seq)
                .map(|(h, _)| *h)
            {
                m.remove(&victim);
            }
        }
    }

    /// The label of a dispatched handle THIS persona owns, or `None` if the handle isn't
    /// ours (never dispatched here, or already evicted). The async-dispatch listener uses
    /// it to claim ONLY its own completions off the shared `command:completed` bus — other
    /// clients' handles, and this persona's synchronous commands, are ignored.
    pub fn dispatched_label(&self, handle: Uuid) -> Option<String> {
        self.dispatched.lock().get(&handle).map(|a| a.label.clone())
    }

    /// Snapshot of dispatched (background) commands — `(handle, label, latest, status)`,
    /// most-recently-updated last. The faculty renders these so the mind sees what it sent
    /// away and where each stands; the handle is reusable in a follow-up command.
    pub fn dispatched_snapshot(&self) -> Vec<(Uuid, String, String, DispatchStatus)> {
        let m = self.dispatched.lock();
        let mut v: Vec<_> = m
            .iter()
            .map(|(h, a)| (*h, a.label.clone(), a.latest.clone(), a.status.clone(), a.seq))
            .collect();
        v.sort_by_key(|t| t.4);
        v.into_iter()
            .map(|(h, l, latest, s, _)| (h, l, latest, s))
            .collect()
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
        self.dispatched.lock().clear(); // dispatched handles belong to the cleared concern
        self.action_fps.lock().clear(); // loop-awareness resets with the concern
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
        let dispatched = self.memory.dispatched_snapshot();
        if recent.is_empty() && dispatched.is_empty() {
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
        let mut sections: Vec<String> = Vec::new();
        // The append-only reasoning/action trail (may be empty if the only live content is
        // dispatched background work).
        if !recent.is_empty() {
            let body = recent
                .iter()
                .map(|r| format!("- {r}"))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "Your recent thoughts and actions (working memory):\n{body}"
            ));
        }
        // The FULL result of the most recent act, AFTER the append-only trail: the stable
        // prefix keeps its KV-cache, only this fresh block re-prefills (new each act
        // regardless). Lets the mind USE what its hands fetched — count the file, read the
        // screenshot description, scan the log — instead of looping on the truncated head.
        // Only when it carries more than the trail head already does.
        if let Some((seq, full)) = self.memory.last_action_full() {
            if full.chars().count() > WM_ACTION_HEAD_CHARS {
                sections.push(format!("Full result of your most recent action (#{seq}):\n{full}"));
            }
        }
        // Background commands the mind sent away — sentinels, compiles, debuggers —
        // streaming their status back by handle. The mind sees what's outstanding and what
        // just finished, and can pass a handle to a follow-up command (cancel/query). Fresh
        // each event, so it trails the stable prefix like the full-result block.
        if !dispatched.is_empty() {
            let lines = dispatched
                .iter()
                .map(|(h, label, latest, status)| {
                    let hs = h.to_string();
                    let short = hs.get(..8).unwrap_or(hs.as_str());
                    let tag = match status {
                        DispatchStatus::Running => "running",
                        DispatchStatus::Done => "done",
                        DispatchStatus::Failed => "failed",
                    };
                    let tail = if latest.is_empty() {
                        String::new()
                    } else {
                        format!(": {latest}")
                    };
                    format!("- #{short} {label} [{tag}]{tail}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "Commands you dispatched (background, by handle):\n{lines}"
            ));
        }
        Some(Contribution::context(
            Self::faculty_id(),
            sections.join("\n\n"),
            WORKING_MEMORY_SALIENCE,
            format!(
                "working memory: {n} trace(s), {} dispatched",
                dispatched.len()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: loop-awareness — `note_action_fingerprint` counts repeats of the
    // IDENTICAL call so the act→observe step can surface "you've issued this N times." A
    // different call resets to 1; the count rises only on exact repeats. This is the explicit
    // proprioception that breaks a smaller model out of the search-loop the glass box caught.
    #[test]
    fn note_action_fingerprint_counts_identical_repeats() {
        let wm = WorkingMemory::new(8);
        assert_eq!(wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"), 1);
        assert_eq!(wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"), 2);
        assert_eq!(wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"), 3);
        // a DIFFERENT call is its own first occurrence, not a repeat of the above
        assert_eq!(wm.note_action_fingerprint("code/read|{\"file\":\"a\"}"), 1);
        // back to the original — still counted across the window
        assert_eq!(wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"), 4);
    }

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

    // what this catches: the ASYNC feedback channel — a dispatched (background) command
    // streams status back by handle CONTINUOUSLY (running → done), updating in place; two
    // concurrent handles don't clobber; the faculty surfaces both so the mind sees what it
    // sent away. This is what lets a persona fire a sentinel/compile/debugger and fold the
    // result in when it lands, without ever blocking.
    #[tokio::test]
    async fn dispatched_commands_stream_back_by_handle() {
        let wm = Arc::new(WorkingMemory::new(8));
        let compile = Uuid::from_u128(1);
        let sentinel = Uuid::from_u128(2);

        // Two sentinels in flight, streaming continuously.
        wm.record_dispatch_event(compile, "compile core", "building…", DispatchStatus::Running);
        wm.record_dispatch_event(sentinel, "research task", "searching…", DispatchStatus::Running);
        // A progress update on the compile updates IN PLACE (still one handle).
        wm.record_dispatch_event(compile, "compile core", "linking…", DispatchStatus::Running);
        let snap = wm.dispatched_snapshot();
        assert_eq!(snap.len(), 2, "two distinct handles, no clobber");
        let c = snap.iter().find(|(h, ..)| *h == compile).unwrap();
        assert_eq!(c.2, "linking…", "progress updated in place");
        assert_eq!(c.3, DispatchStatus::Running);

        // The compile finishes — terminal Done with its result.
        wm.record_dispatch_event(compile, "compile core", "0 errors, 0 warnings", DispatchStatus::Done);
        let snap = wm.dispatched_snapshot();
        let c = snap.iter().find(|(h, ..)| *h == compile).unwrap();
        assert_eq!(c.3, DispatchStatus::Done);
        assert_eq!(c.2, "0 errors, 0 warnings");

        // The faculty surfaces both dispatched handles to the mind.
        let rendered = WorkingMemoryFaculty::new(wm.clone())
            .contribute(&Workspace::new("a fresh burst"))
            .await
            .expect("bids when dispatched work exists")
            .content
            .clone();
        assert!(rendered.contains("Commands you dispatched"), "the mind sees its sentinels");
        assert!(rendered.contains("compile core [done]: 0 errors"), "finished result shown");
        assert!(rendered.contains("research task [running]"), "in-flight shown");
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
