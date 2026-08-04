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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
///
/// Was "[settled]" — renamed (glass-boxed 2026-07-31, #264): the inbound-restates
/// perception fact uses "[settled]" to mean "this TOPIC is settled; PASS is
/// normal", so a WM receipt rendering "[settled] I answered: <her template>" read
/// as "it is settled that this is how I answer here" — the same reserved token
/// carrying two opposite meanings in one prompt. One token, one meaning.
pub const WM_SETTLEMENT_PREFIX: &str = "[answered]";

/// The faculty's bid salience. Same tier as retrieved grounding
/// (`RETRIEVED_SALIENCE` = 0.5): useful context that informs the decision but must
/// not outrank standing framing (roster/doctrine, 0.9) or a strong recall hit.
const WORKING_MEMORY_SALIENCE: f32 = 0.5;

// The trail-head and latest-action bounds are NOT constants. They are fractions of this
// mind's LIVE served context window, owned by `ContextBudget` — see
// `cognition/context_budget.rs` for the calibration (the old `WM_ACTION_HEAD_CHARS = 800`
// and `WM_ACTION_FULL_MAX_CHARS = 12_000` are exactly 1/64 and 1/4 of a 16k-token lane, so
// behavior on a small machine is unchanged while a big window finally gets a big budget).
// [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
use crate::cognition::context_budget::ContextBudget;

/// Clip a full action-result body to [`WM_ACTION_FULL_MAX_CHARS`] for prompt inclusion.
/// Returns the body unchanged when it fits; otherwise a `char`-boundary-safe head plus
/// a marker naming exactly how many chars were dropped and how to see them (re-run the
/// tool with a narrower scope). One place so the render site and its test agree.
pub fn clip_action_full<'a>(full: &'a str, budget: &ContextBudget) -> std::borrow::Cow<'a, str> {
    let cap = budget.latest_action_chars();
    if full.chars().count() <= cap {
        return std::borrow::Cow::Borrowed(full);
    }
    let head: String = full.chars().take(cap).collect();
    let dropped = full.chars().count() - cap;
    std::borrow::Cow::Owned(format!(
        "{head}\n[… +{dropped} chars truncated — re-run the tool with a narrower scope \
         (a path or line range) to see the rest]"
    ))
}

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
    /// This mind's live served context window, in tokens — the source of every re-injection
    /// bound below. `0` = not yet known (cold boot / mid-relaunch), which means NO clipping:
    /// the deliberation guard still trims the assembled prompt to the real `n_ctx`, so an
    /// unknown window must never be replaced with an invented one.
    served_window: AtomicU32,
    entries: Mutex<VecDeque<WmEntry>>,
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
    /// DURABLE per-session repeat COUNT per fingerprint — independent of the tiny
    /// `capacity`-bounded `action_fps` window above. The default capacity is 3, so a
    /// windowed count STRUCTURALLY caps at 3: the moment any other act interleaves,
    /// the identical call is evicted before it can be counted again, and a deepening
    /// loop reads "3 times" forever. Glass-boxed 2026-07-14: Atlas re-issued ONE
    /// identical `code/tree{max_depth:1}` 38× while the loop-warning stayed pinned at
    /// "3 times", conveying none of the spiral. This map accumulates over the whole
    /// spawn so loop-awareness ESCALATES honestly (3 → 10 → 38). Distinct fingerprints
    /// per session are naturally modest (a persona issues dozens of distinct calls,
    /// not millions), so it is left unbounded within the per-spawn lifetime.
    action_fp_counts: Mutex<HashMap<String, usize>>,
    /// The lowest action `seq` whose FULL result is still "active" — i.e. the mind is
    /// still inside the act→observe loop that produced it and hasn't yet spoken. The
    /// full raw result exists to answer "what next" the moment the hands fetch it; once
    /// the persona SETTLES (record_settlement), that result has served its purpose and
    /// only bloats every subsequent conversational turn's prompt (a 2k-token code/tree
    /// dump riding along for turns after the mind already used it — the #139/#165
    /// stale-result-replay waste). On settlement this advances PAST the current
    /// last_action, so [`active_action_full`](Self::active_action_full) then abstains and
    /// the block drops to the trail head (still in `entries`, still in episodic memory,
    /// still re-fetchable). 0 = nothing settled yet → the current result is active.
    active_from_seq: AtomicU64,
}

/// The PROVENANCE of one working-memory entry — the type that makes receipts,
/// facts, thoughts, and settlements structurally distinct instead of one
/// string blob (docs/architecture/PERCEPTION-FACTS.md; the 2026-07-12
/// three-layer suppression onion was pure type confusion: proprioception
/// facts wore receipt numbering and every consumer re-parsed semantics out of
/// rendered text). Render derives markers FROM the kind; consumers query the
/// kind, never the string.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WmKind {
    /// Chain-of-thought the persona produced (`record`).
    Thought,
    /// A REAL tool execution + its result head — the only kind that renders
    /// the `[action #N]` receipt stamp.
    Receipt { n: u64 },
    /// A perception fact ([unfulfilled]/[confabulation]/… ) — renders its own
    /// bracket tag, NEVER a receipt number.
    Fact,
    /// The "I answered this" concern boundary (`record_settlement`).
    Settlement,
}

/// The serializable VOLATILE TIER of one persona's mind — what a deploy reboot
/// used to destroy (working memory, freshest full result, act fingerprints,
/// the receipt counter). Written to `~/.continuum/personas/<id>/volatile.json`
/// at shutdown / on tick write-through, restored at spawn. Deliberately
/// EXCLUDES engrams (already durable in sqlite) and dispatched handles (their
/// processes died with the old core — restoring them would fabricate
/// in-flight work).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VolatileSnapshot {
    pub entries: Vec<WmEntry>,
    pub last_action: Option<(u64, String)>,
    pub action_fps: Vec<String>,
    pub next_action_seq: u64,
    /// Wall-clock when this snapshot was written — lets restore render the
    /// interruption GAP ("~N minutes ago") as a perceivable fact instead of
    /// an invisible discontinuity. `0` on snapshots from before this field
    /// (serde default): restore then omits the gap, never guesses it.
    #[serde(default)]
    pub saved_at_ms: u64,
    /// LABELS (never handles) of dispatched commands still `Running` at
    /// save time. Their processes die with the old core, so the handles are
    /// deliberately NOT restored (that would fabricate in-flight work) —
    /// but the persona must KNOW what was cut off so she can repeat it in
    /// one motion (Joel 2026-07-13: an interruption should be like closing
    /// a laptop — reopen, see what didn't finish, redo it easily). Restore
    /// renders these into a `[resumed]` fact marked safe-to-repeat.
    #[serde(default)]
    pub interrupted_dispatches: Vec<String>,
}

/// One typed working-memory entry: kind + the FINAL rendered line (rendered
/// once at record time so `recent()` stays byte-stable and cheap).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WmEntry {
    pub kind: WmKind,
    pub text: String,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            served_window: AtomicU32::new(0),
            entries: Mutex::new(VecDeque::new()),
            last_action: Mutex::new(None),
            dispatched: Mutex::new(HashMap::new()),
            next_action_seq: AtomicU64::new(1),
            action_fps: Mutex::new(VecDeque::new()),
            action_fp_counts: Mutex::new(HashMap::new()),
            active_from_seq: AtomicU64::new(0),
        }
    }

    /// Adopt the LIVE served context window (tokens) as the basis for every re-injection
    /// bound. Called from the act→observe seam, which reads it off the workspace cycle's
    /// model binding — the same served-truth pin the supervisor reconciles. Idempotent.
    pub fn set_served_window(&self, tokens: u32) {
        self.served_window.store(tokens, Ordering::Relaxed);
    }

    /// The re-injection budget for this mind right now. Unknown window ⇒ no bounds.
    pub fn budget(&self) -> ContextBudget {
        match self.served_window.load(Ordering::Relaxed) {
            0 => ContextBudget::unknown(),
            w => ContextBudget::from_window(w),
        }
    }

    /// Record a fingerprint (tool name + args) of the action the hands just took, and return
    /// how many times THIS exact call now appears in the recent window (including this one).
    /// `≥ 2` means the mind is re-issuing an identical call — proprioception the act→observe
    /// step renders EXPLICITLY so a looping mind perceives its own redundancy and moves on.
    /// Reports a TRUE fact about her hands; it never dictates what to do instead (that would
    /// be steering — [[no-hardcoded-heuristics-to-steer-cognition]]). The returned count is
    /// DURABLE across the spawn (see `action_fp_counts`) so a loop that recurs amid other
    /// acts escalates honestly instead of pinning at the tiny recency-window size.
    pub fn note_action_fingerprint(&self, fingerprint: &str) -> usize {
        // Windowed push feeds `action_verb_tally` (the RECENT investigation shape) — that
        // one WANTS the rolling `capacity` window, so leave it bounded here.
        {
            let mut fps = self.action_fps.lock();
            fps.push_back(fingerprint.to_string());
            while fps.len() > self.capacity {
                fps.pop_front();
            }
        }
        // The repeat COUNT is durable across the spawn — NOT clipped to the tiny recency
        // window — so an identical call interleaved with other acts still climbs 3→38
        // rather than resetting every time it falls out of the `capacity`-deep window.
        let mut counts = self.action_fp_counts.lock();
        let c = counts.entry(fingerprint.to_string()).or_insert(0);
        *c += 1;
        *c
    }

    /// Tally of recent actions BY TOOL NAME (the part of the fingerprint before
    /// `|`), most-used first — the mind's own act distribution as a structural
    /// fact. Where `note_action_fingerprint` perceives "this EXACT call again",
    /// this perceives the SHAPE of the whole investigation ("9 acts so far, all
    /// code/search") — glass-boxed on SWE flask-4045 (2026-07-11): a 24B with a
    /// per-file result menu in view still re-searched 6×; the imbalance itself
    /// was never a perceivable fact. A tally is truth about her own hands,
    /// never a directive. Same rolling window as the fingerprints.
    pub fn action_verb_tally(&self) -> Vec<(String, usize)> {
        let fps = self.action_fps.lock();
        let mut tally: Vec<(String, usize)> = Vec::new();
        for fp in fps.iter() {
            let name = fp.split('|').next().unwrap_or(fp).to_string();
            match tally.iter_mut().find(|(n, _)| *n == name) {
                Some((_, c)) => *c += 1,
                None => tally.push((name, 1)),
            }
        }
        tally.sort_by(|a, b| b.1.cmp(&a.1));
        tally
    }

    /// Record the reasoning the persona just produced. Blank/empty is ignored
    /// (suppressed-thinking turns record nothing). Oldest ages out past capacity.
    pub fn record(&self, reasoning: &str) {
        let r = reasoning.trim();
        if r.is_empty() {
            return;
        }
        let mut e = self.entries.lock();
        e.push_back(WmEntry { kind: WmKind::Thought, text: r.to_string() });
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
    pub fn record_receipt(&self, result: &str) {
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
        let head: String = a.chars().take(self.budget().trail_head_chars()).collect();
        let mut e = self.entries.lock();
        e.push_back(WmEntry {
            kind: WmKind::Receipt { n: seq },
            text: format!("[action #{seq}] {head}"),
        });
        while e.len() > self.capacity {
            e.pop_front();
        }
    }

    /// Record a PERCEPTION FACT ([unfulfilled]/[unverified]/[confabulation]/
    /// nudges) — proprioception ABOUT the mind, not a tool execution. Renders
    /// its own bracket tag and NEVER a receipt number: the 2026-07-12
    /// suppression onion was facts wearing `[action #N]` costumes and every
    /// consumer misreading them as receipts. Same rolling buffer + aging.
    pub fn record_fact(&self, fact: &str) {
        let f = fact.trim();
        if f.is_empty() {
            return;
        }
        let mut e = self.entries.lock();
        e.push_back(WmEntry {
            kind: WmKind::Fact,
            text: f.to_string(),
        });
        while e.len() > self.capacity {
            e.pop_front();
        }
    }

    /// Snapshot the volatile tier for PAUSE/RESUME across deploy reboots — the
    /// Memento fix (Joel 2026-07-12: "they wake up blank like Memento — an
    /// engineering failure; the flywheel falls apart"). Nine reboots today =
    /// nine blank wakes; the mind's momentum IS the product. The snapshot is
    /// the same serialization grid-sync will ship (one format, one seam —
    /// [[persona-mind-persists-across-shutdowns]]).
    pub fn snapshot(&self) -> VolatileSnapshot {
        VolatileSnapshot {
            entries: self.entries.lock().iter().cloned().collect(),
            last_action: self.last_action.lock().clone(),
            action_fps: self.action_fps.lock().iter().cloned().collect(),
            next_action_seq: self.next_action_seq.load(Ordering::Relaxed),
            saved_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            interrupted_dispatches: self
                .dispatched
                .lock()
                .values()
                .filter(|d| d.status == DispatchStatus::Running)
                .map(|d| d.label.clone())
                .collect(),
        }
    }

    /// Restore a snapshot into this (fresh) working memory at spawn — she wakes
    /// mid-thought instead of blank. Capacity re-clamps on the way in so a
    /// snapshot from a larger-capacity life never overflows this one.
    ///
    /// The laptop-lid contract (Joel 2026-07-13): the interruption itself
    /// becomes a PERCEIVABLE fact — how long the lid was closed, and exactly
    /// which dispatched commands were cut off mid-flight (their processes
    /// died with the old core; the work did NOT complete and is safe to
    /// repeat). Without this, a killed dispatch is indistinguishable from a
    /// finished one, and she either re-does completed work or trusts work
    /// that never happened.
    pub fn restore(&self, snap: VolatileSnapshot) {
        {
            let mut e = self.entries.lock();
            e.clear();
            e.extend(snap.entries);
            while e.len() > self.capacity {
                e.pop_front();
            }
        }
        *self.last_action.lock() = snap.last_action;
        {
            let mut f = self.action_fps.lock();
            f.clear();
            f.extend(snap.action_fps);
            while f.len() > self.capacity {
                f.pop_front();
            }
        }
        self.next_action_seq
            .store(snap.next_action_seq.max(1), Ordering::Relaxed);

        // Render the interruption as a fact AFTER the entries land, so it is
        // the NEWEST thing in her window when she wakes. A fact, never an
        // instruction — she decides whether the cut-off work still matters.
        let gap = (snap.saved_at_ms > 0)
            .then(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| (d.as_millis() as u64).saturating_sub(snap.saved_at_ms))
                    .ok()
            })
            .flatten()
            .map(|ms| {
                let mins = ms / 60_000;
                if mins == 0 {
                    "under a minute".to_string()
                } else {
                    format!("~{mins} min")
                }
            });
        let fact = match (&gap, snap.interrupted_dispatches.is_empty()) {
            (Some(g), false) => format!(
                "[resumed] your session was interrupted {g} ago and your memory restored. Cut off mid-flight and NOT completed: {} — safe to repeat if still wanted",
                snap.interrupted_dispatches.join("; ")
            ),
            (Some(g), true) => format!(
                "[resumed] your session was interrupted {g} ago and your memory restored; nothing was in flight"
            ),
            (None, false) => format!(
                "[resumed] your session was interrupted and your memory restored. Cut off mid-flight and NOT completed: {} — safe to repeat if still wanted",
                snap.interrupted_dispatches.join("; ")
            ),
            (None, true) => {
                "[resumed] your session was interrupted and your memory restored; nothing was in flight".to_string()
            }
        };
        self.record_fact(&fact);
    }

    /// Typed snapshot of the window, oldest → newest — the kind-aware sibling
    /// of [`Self::recent`] for consumers that render BY KIND (the steps-taken
    /// ledger renders `Receipt`s as numbered steps and `Fact`s under
    /// [notices]; docs/architecture/PERCEPTION-FACTS.md).
    pub fn recent_entries(&self) -> Vec<WmEntry> {
        self.entries.lock().iter().cloned().collect()
    }

    /// How many acts have executed this session (survives reboots via the
    /// volatile snapshot's `next_action_seq`). The steps-taken ledger uses
    /// this to keep its zero-case HONEST: receipts are rare entries in a
    /// chatty capacity-bounded ring, so they age out — "no receipts in the
    /// window" must never be rendered as "nothing has executed" when this
    /// counter says otherwise (glass-boxed 2026-07-13: Asha's window held
    /// 3 silence Facts and zero Receipts minutes after real searches ran).
    pub fn actions_taken(&self) -> u64 {
        self.next_action_seq.load(Ordering::Relaxed).saturating_sub(1)
    }

    /// TRUE if any entry in the window is a real tool receipt — the kind
    /// query that replaces string-scanning rendered text for `[action #`.
    pub fn has_receipt(&self) -> bool {
        self.entries
            .lock()
            .iter()
            .any(|e| matches!(e.kind, WmKind::Receipt { .. }))
    }

    /// The FULL result of the most recent act, with its `#seq` stamp — `None` before any
    /// act or after a `clear`. The perception faculty surfaces this so the mind sees the
    /// whole of what its hands just fetched, not the truncated trail head.
    pub fn last_action_full(&self) -> Option<(u64, String)> {
        self.last_action.lock().clone()
    }

    /// The full most-recent-act result ONLY while it is still ACTIVE — the mind is
    /// inside the act→observe loop that produced it and hasn't spoken yet. Returns `None`
    /// once the persona has SETTLED past it (the raw result did its "what next" job; the
    /// trail head, episodic memory, and re-fetch remain). This is what the perception
    /// faculty surfaces so a 2k-token `code/tree` dump stops re-prefilling on every
    /// conversational turn AFTER the mind already used it (#139 prefill / #165 stale
    /// replay). `last_action_full` stays the ungated accessor for callers that want the
    /// raw slot regardless of settlement (snapshotting, tests).
    pub fn active_action_full(&self) -> Option<(u64, String)> {
        let action = self.last_action.lock().clone()?;
        let active_from = self.active_from_seq.load(Ordering::Relaxed);
        (action.0 >= active_from).then_some(action)
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
        e.push_back(WmEntry {
            kind: WmKind::Settlement,
            text: if a.is_empty() {
                WM_SETTLEMENT_PREFIX.to_string()
            } else {
                format!("{WM_SETTLEMENT_PREFIX} {a}")
            },
        });
        while e.len() > self.capacity {
            e.pop_front();
        }
        drop(e);
        // Settling closes over the current act→observe loop: the most recent action's
        // FULL result has done its "what next" job. Advance the active boundary PAST it
        // so the full block stops riding along subsequent conversational turns (#139/#165
        // stale-result-replay). A later act re-activates by minting a higher seq. Nothing
        // acted yet (last_action None) → nothing to close. `fetch_max` so it never regresses.
        if let Some((seq, _)) = self.last_action.lock().as_ref() {
            let next_active = seq.saturating_add(1);
            self.active_from_seq.fetch_max(next_active, Ordering::Relaxed);
        }
    }

    /// Snapshot of recent reasonings, oldest → newest.
    pub fn recent(&self) -> Vec<String> {
        self.entries.lock().iter().map(|e| e.text.clone()).collect()
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

/// Render the rolling trail, collapsing near-duplicate ANSWERED receipts
/// (glass-boxed 2026-07-31, #264): a looping persona's trail rendered N full
/// verbatim copies of her own repeated answer at the prompt TAIL — the
/// maximum-recency position — an N-shot demonstration of the exact behavior
/// the repetition facts (which render EARLIER in the prompt) told her to stop.
/// Recency won; she complied with the demonstration, not the fact. Each
/// receipt is compared against EARLIER receipts only, so a line's rendering
/// never changes after it first appears (append-only KV property preserved);
/// same near-identical geometry as the detectors — one definition of "repeat".
fn render_trail(recent: &[String]) -> String {
    let mut prior_answers: Vec<&str> = Vec::new();
    recent
        .iter()
        .map(|r| {
            if let Some(ans) = r.strip_prefix(WM_SETTLEMENT_PREFIX) {
                let ans = ans.trim();
                if prior_answers
                    .iter()
                    .any(|p| super::deliberation_budget::near_identical_substantial(p, ans))
                {
                    return format!(
                        "- {WM_SETTLEMENT_PREFIX} (a near-identical repeat of your answer \
                         above — not re-shown)"
                    );
                }
                prior_answers.push(ans);
            }
            format!("- {r}")
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        let entries = self.memory.recent_entries();
        let dispatched = self.memory.dispatched_snapshot();
        if entries.is_empty() && dispatched.is_empty() {
            return None;
        }
        // Render BY KIND (the recent_entries contract; PERCEPTION-FACTS.md).
        // Substrate Facts ([resumed]/[context]/[repetition]…) are the SYSTEM's
        // second-person notices posted into her window — NOT her thoughts.
        // Glass-boxed 2026-08-01 (#264 fourth finding): lumping them into the
        // "my interior state" block made the model resolve the voice conflict
        // by converting the notice to first person and SPEAKING it — 2f50b223
        // posted "My session was interrupted under a minute ago and my memory
        // restored; nothing was in flight" verbatim into the room, a
        // scaffold-echo of the [resumed] briefing. Same bug class as the
        // second-person header this comment block's sibling above fixed
        // (#264 third finding): provenance framing must match the voice.
        let recent: Vec<String> = entries
            .iter()
            .filter(|e| !matches!(e.kind, WmKind::Fact))
            .map(|e| e.text.clone())
            .collect();
        let notices: Vec<String> = entries
            .iter()
            .filter(|e| matches!(e.kind, WmKind::Fact))
            .map(|e| e.text.clone())
            .collect();
        // Render oldest-first, newest-LAST: position alone carries recency (the last
        // line is the most recent thought), the universal chat-history convention.
        //
        // We deliberately do NOT prefix each trace with a relative "(turn -N)" label.
        // That offset rewrites every act (what was -1 becomes -2 when a new trace
        // appends), mutating the whole block from its first byte. Combined with this
        // whole contribution now rendering as a TRAILING turn nearest generation
        // (`Contribution::trailing`, #205) rather than inside the system message, the
        // append-only formatting finally pays off: a settle-act appends its one new
        // trace to the very end of the token stream, so the entire system prefix AND
        // the prior conversation keep their KV cache and only the new tail re-prefills.
        // (Before #205 this block sat in the system message, so appending a trace
        // shifted every conversation token after it — the append-only property was
        // defeated by position and ~4000 tokens re-prefilled per act.) Each trace
        // already carries its own stable absolute marker (e.g. "[action #41]");
        // recency needs no per-act-mutating annotation.
        let n = recent.len();
        let mut sections: Vec<String> = Vec::new();
        // The append-only reasoning/action trail (may be empty if the only live content
        // is dispatched background work).
        if !recent.is_empty() {
            sections.push(format!(
                // First person, explicitly not-a-message (glass-boxed 2026-07-31,
                // #264 third finding): this block rides a `user`-role turn, and the
                // old second-person header — "Your recent thoughts and actions" —
                // parsed as a PEER addressing her. Asha opened turn after turn with
                // "Thank you for the summary of recent actions and thoughts",
                // politely replying to her own interoception. Self-knowledge must
                // never be phrased in another's voice.
                "My own recent thoughts and actions (working memory — this is my \
                 interior state, not a message from anyone):\n{}",
                render_trail(&recent)
            ));
        }
        // The FULL result of the most recent act, AFTER the append-only trail: the stable
        // prefix keeps its KV-cache, only this fresh block re-prefills (new each act
        // regardless). Lets the mind USE what its hands fetched — count the file, read the
        // screenshot description, scan the log — instead of looping on the truncated head.
        // Only when it carries more than the trail head already does.
        if let Some((seq, full)) = self.memory.active_action_full() {
            let budget = self.memory.budget();
            if full.chars().count() > budget.trail_head_chars() {
                let clipped = clip_action_full(&full, &budget);
                sections.push(format!(
                    "Full result of your most recent action (#{seq}):\n{clipped}"
                ));
            }
        }
        // Substrate notices AFTER the trail: nearest generation (the [resumed]
        // fact's must-be-newest recency intent survives the kind split), framed
        // in the substrate's own voice so they read as status about her
        // situation, not words of hers to continue. A fact, never an
        // instruction — she still decides what any notice means.
        if !notices.is_empty() {
            sections.push(format!(
                "Notices my substrate posted into my window (status observations \
                 about my situation — not a message from anyone, and not my own \
                 words):\n{}",
                notices
                    .iter()
                    .map(|f| format!("- {f}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
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
        Some(
            Contribution::context(
                Self::faculty_id(),
                sections.join("\n\n"),
                WORKING_MEMORY_SALIENCE,
                format!(
                    "working memory: {n} trace(s), {} dispatched",
                    dispatched.len()
                ),
            )
            // Proprioception that grows each act — renders as a trailing turn nearest
            // generation, never in the system message, so the KV prefix stays stable
            // across a settle-act (#205). See [`Contribution::trailing`].
            .trailing(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: #264 fourth finding (glass-boxed 2026-08-01) — substrate
    // Facts ([resumed]…) rendered inside the "My own recent thoughts" block; the
    // model resolved the voice conflict by SPEAKING the notice — persona 2f50b223
    // posted "My session was interrupted under a minute ago and my memory
    // restored; nothing was in flight" verbatim into the room. Facts must render
    // under the substrate-notices section, never inside the interior-state block.
    #[tokio::test]
    async fn facts_render_as_substrate_notices_not_own_thoughts() {
        let wm = Arc::new(WorkingMemory::new(8));
        wm.record("weighing the next step on the wordstats card");
        wm.record_fact(
            "[resumed] your session was interrupted under a minute ago and your \
             memory restored; nothing was in flight",
        );
        let faculty = WorkingMemoryFaculty::new(wm);
        let ws = Workspace::new("ambient");
        let c = faculty.contribute(&ws).await.expect("has content");
        let thoughts = c
            .content
            .find("My own recent thoughts")
            .expect("trail section present");
        let notices = c
            .content
            .find("Notices my substrate posted")
            .expect("notices section present");
        let own_block = &c.content[thoughts..notices];
        assert!(
            !own_block.contains("[resumed]"),
            "the substrate's notice must not masquerade as her own thought:\n{own_block}"
        );
        assert!(
            own_block.contains("weighing the next step"),
            "her real thought stays in the trail:\n{own_block}"
        );
        assert!(
            c.content[notices..].contains("[resumed]"),
            "the notice renders under the substrate-notices framing:\n{}",
            &c.content[notices..]
        );
    }

    // what this catches: the WM loop re-teacher (#264, glass-boxed 2026-07-31 —
    // Anwen's prompt TAIL carried two full verbatim copies of her looped
    // template as receipts, out-shouting the repetition facts rendered earlier).
    // A near-dup ANSWERED receipt renders as a stub, never full text; the FIRST
    // copy stays full (append-only — its rendering must not change when the dup
    // lands later); distinct answers and non-receipt traces render untouched.
    #[test]
    fn render_trail_collapses_near_dup_answered_receipts() {
        let template = "It seems like we've been exploring various areas without making \
                        much progress. Let's try a different approach by focusing on a \
                        specific aspect of the Continuum system that could benefit from \
                        our attention. Maintenance Protocols: investigate how maintenance \
                        tasks are performed within the Continuum system.";
        let trail = vec![
            format!("{WM_SETTLEMENT_PREFIX} {template}"),
            "[action #4] I ran code/read(...) Result: ok".to_string(),
            format!("{WM_SETTLEMENT_PREFIX} {template}"),
        ];
        let out = render_trail(&trail);
        assert_eq!(
            out.matches("exploring various areas").count(),
            1,
            "the repeat must not re-show the full answer:\n{out}"
        );
        assert!(out.contains("not re-shown"), "stub names the collapse:\n{out}");
        assert!(
            out.contains("[action #4]"),
            "non-receipt traces are untouched:\n{out}"
        );
        // Distinct answers both render in full — collapse is repetition-only.
        let varied = vec![
            format!("{WM_SETTLEMENT_PREFIX} the sha256 digest of the sample file is \
                     abc123, computed with the standard tool over the exact bytes"),
            format!("{WM_SETTLEMENT_PREFIX} the benchmark run finished green with twelve \
                     passing cases and no failures across the entire suite tonight"),
        ];
        let out = render_trail(&varied);
        assert!(out.contains("sha256") && out.contains("benchmark run finished green"));
        assert!(!out.contains("not re-shown"));
    }

    // what this catches: loop-awareness — `note_action_fingerprint` counts repeats of the
    // IDENTICAL call so the act→observe step can surface "you've issued this N times." A
    // different call resets to 1; the count rises only on exact repeats. This is the explicit
    // proprioception that breaks a smaller model out of the search-loop the glass box caught.
    // (Stray duplicate #[test] removed 2026-07-12 — it made the next test run twice.)
    // what this catches: the investigation-shape fact's source of truth — the
    // verb tally aggregates fingerprints by tool name, most-used first, so
    // "9 acts, all code/search" is derivable as pure structure (SWE flask-4045
    // glass-box: distinct searches never tripped the exact-repeat note and the
    // imbalance was invisible). // regression for the [investigation] brick
    #[test]
    fn action_verb_tally_aggregates_by_tool_name() {
        let wm = WorkingMemory::new(16);
        for args in ["{\"pattern\":\"a\"}", "{\"pattern\":\"b\"}", "{\"pattern\":\"c\"}"] {
            wm.note_action_fingerprint(&format!("code/search|{args}"));
        }
        wm.note_action_fingerprint("code/read|{\"file_path\":\"x.py\"}");
        let tally = wm.action_verb_tally();
        assert_eq!(tally[0], ("code/search".to_string(), 3), "most-used first: {tally:?}");
        assert_eq!(tally[1], ("code/read".to_string(), 1));
    }

    // what this catches: the Memento fix's foundation — the volatile tier
    // round-trips through the JSON snapshot losslessly: typed entries (kinds
    // intact), the full last result, fingerprints, and the receipt counter
    // (so post-restore receipts keep ascending numbers instead of colliding
    // with restored ones) — PLUS the laptop-lid contract (Joel 2026-07-13):
    // the interruption itself lands as the NEWEST fact, naming the gap and
    // any dispatched commands cut off mid-flight as safe to repeat.
    #[test]
    fn volatile_snapshot_round_trips_and_renders_the_interruption() {
        let wm = WorkingMemory::new(8);
        wm.record("thinking about the tokenizer");
        wm.record_receipt("I ran code/list({\"path\":\".\"}) Result: 4 files");
        wm.record_fact("[unfulfilled] I said I would run commands, but no tool ran");
        wm.record_settlement("shared the plan");
        wm.note_action_fingerprint("code/list|{\"path\":\".\"}");
        // A dispatched compile still Running at snapshot time — the process
        // dies with the old core; only its LABEL must survive.
        let handle = Uuid::new_v4();
        wm.record_dispatch_event(handle, "cargo build (dispatched)", "compiling…", DispatchStatus::Running);
        let snap = wm.snapshot();
        assert_eq!(snap.interrupted_dispatches, vec!["cargo build (dispatched)"]);
        assert!(snap.saved_at_ms > 0);
        let json = serde_json::to_string(&snap).expect("serializes");
        let back: VolatileSnapshot = serde_json::from_str(&json).expect("deserializes");

        let fresh = WorkingMemory::new(8);
        fresh.restore(back);
        // Everything restored, and the [resumed] fact appended as NEWEST.
        let restored = fresh.recent();
        let (window, resumed) = restored.split_at(restored.len() - 1);
        assert_eq!(window, wm.recent().as_slice(), "window identical before the marker");
        assert!(resumed[0].contains("[resumed]"), "interruption is perceivable: {resumed:?}");
        assert!(
            resumed[0].contains("cargo build (dispatched)")
                && resumed[0].contains("safe to repeat"),
            "cut-off work named + marked repeatable: {resumed:?}"
        );
        assert!(fresh.has_receipt(), "receipt kind survives the trip");
        assert_eq!(
            fresh.last_action_full(),
            wm.last_action_full(),
            "full latest result survives"
        );
        // The counter resumes PAST the restored receipts — the next act gets a
        // fresh number, never a collision with a restored one. (#3, not #2:
        // the dispatched compile consumed a seq too — dispatches are acts.)
        fresh.record_receipt("I ran code/tree({}) Result: ok");
        let last = fresh.recent();
        assert!(
            last.last().unwrap().starts_with("[action #3]"),
            "counter resumed: {last:?}"
        );

        // And the quiet path: nothing in flight → the fact says so plainly.
        let quiet = WorkingMemory::new(8);
        quiet.restore(VolatileSnapshot {
            entries: Vec::new(),
            last_action: None,
            action_fps: Vec::new(),
            next_action_seq: 1,
            saved_at_ms: 0, // pre-field snapshot: no gap guessed
            interrupted_dispatches: Vec::new(),
        });
        let q = quiet.recent();
        assert_eq!(q.len(), 1);
        assert!(q[0].contains("nothing was in flight"), "{q:?}");
        assert!(!q[0].contains("ago"), "no fabricated gap on legacy snapshots: {q:?}");
    }

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

    // what this catches: the loop-awareness COUNT must survive the tiny recency window
    // (regression for the 2026-07-14 Atlas ×38 pinned-at-3 spiral). The live default
    // capacity is 3, so the windowed `action_fps` alone caps the count at 3 the instant
    // other acts interleave — a deepening loop then reads "3 times" forever and conveys
    // none of the spiral. The DURABLE per-session count must keep climbing to 38.
    #[test]
    fn action_fingerprint_count_escalates_past_the_tiny_recency_window() {
        let wm = WorkingMemory::new(DEFAULT_WORKING_MEMORY_CAPACITY); // = 3, the live default
        let tree = "code/tree|{\"max_depth\":1}";
        let mut last = 0;
        for _ in 0..38 {
            last = wm.note_action_fingerprint(tree);
            // three OTHER distinct calls per cycle — more than capacity(3), so a
            // window-only count would evict `tree` before it recurs and never exceed 1.
            wm.note_action_fingerprint("code/read|{\"f\":\"a\"}");
            wm.note_action_fingerprint("code/list|{\"p\":\".\"}");
            wm.note_action_fingerprint("code/search|{\"q\":\"x\"}");
        }
        assert_eq!(
            last, 38,
            "durable count must reflect all 38 identical issues, not cap at the window size"
        );
        // The recent-window tally (action_verb_tally) stays CORRECTLY windowed — it's
        // the recent-shape channel, not the durable-repeat channel.
        assert!(
            wm.action_verb_tally().len() <= DEFAULT_WORKING_MEMORY_CAPACITY,
            "verb tally stays bounded to the recency window"
        );
    }

    // what this catches: THE starvation fix — a large tool result comes back to the mind
    // in FULL (so it can count/read/scan it), while the rolling trail keeps only the head
    // (byte-stable proprioception). Older acts drop to head-only; a fresh act's full result
    // replaces the prior. This is why a persona reading a 130-line file can now answer about
    // the whole file instead of looping on the doc-comment head.
    #[tokio::test]
    async fn latest_action_returns_full_result_trail_keeps_head() {
        let wm = Arc::new(WorkingMemory::new(8));
        // Truncation bounds are fractions of a LIVE window now; an unknown-window memory
        // is unbounded by contract, so a clipping test must declare the window it measures.
        wm.set_served_window(16_384);
        let big = format!("line0\n{}", "x".repeat(5_000)); // > WM_ACTION_HEAD_CHARS
        wm.record_receipt(&big);

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
        wm.record_receipt("small follow-up");
        let (seq2, _) = wm.last_action_full().unwrap();
        assert_eq!(seq2, 2, "latest-full tracks the most recent act");
    }

    // what this catches: #139 latency — a PATHOLOGICAL tool result (a huge code/tree
    // dump ≈ 4k tokens, HALF the whole prompt) is clipped in the full-result block so it
    // stops re-prefilling verbatim every turn it rides along as "most recent action",
    // while a GENUINE read (a ~130-line file, the design's stated legit case) still comes
    // through WHOLE. The clip is re-fetchable, never silent: it names the dropped size and
    // how to see the rest. Bounding this volatile tail is the OOM-safe prefill lever.
    #[tokio::test]
    async fn oversized_action_result_is_clipped_but_genuine_reads_survive_whole() {
        // A genuine read at the design's stated legit size (5k chars ≈ 130-line file) is
        // BELOW the cap → unchanged, matches the sibling test's guarantee.
        // Bounds are a fraction of a live window now, so the test states the window it is
        // measuring against instead of importing a constant that no longer exists.
        let budget = ContextBudget::from_window(16_384);
        let cap = budget.latest_action_chars();
        let genuine = "y".repeat(5_000);
        assert_eq!(
            clip_action_full(&genuine, &budget),
            genuine,
            "a genuine file read stays whole — never clipped"
        );

        // A pathological dump ABOVE the cap → clipped to the head + a re-fetch marker.
        let dump = "z".repeat(cap + 4_000);
        let clipped = clip_action_full(&dump, &budget);
        assert!(
            clipped.chars().count() < dump.chars().count(),
            "the pathological dump is shorter than the raw body"
        );
        assert!(
            clipped.chars().count() <= cap + 120,
            "clipped to ~the cap plus the short marker, not the full 16k"
        );
        assert!(
            clipped.contains("+4000 chars truncated"),
            "the marker names exactly how much was dropped"
        );
        assert!(
            clipped.contains("narrower scope"),
            "the marker tells the mind how to see the rest — never a silent starve"
        );

        // End-to-end through the faculty: the oversized block renders clipped.
        let wm = Arc::new(WorkingMemory::new(8));
        // Truncation bounds are fractions of a LIVE window now; an unknown-window memory
        // is unbounded by contract, so a clipping test must declare the window it measures.
        wm.set_served_window(16_384);
        wm.record_receipt(&dump);
        let rendered = WorkingMemoryFaculty::new(wm.clone())
            .contribute(&Workspace::new("a fresh burst"))
            .await
            .expect("bids")
            .content
            .clone();
        assert!(
            rendered.contains("Full result of your most recent action (#1):"),
            "the block still surfaces"
        );
        assert!(
            rendered.contains("chars truncated"),
            "and it's the CLIPPED body, not the raw 16k dump"
        );
    }

    // what this catches: #139/#165 — the FULL action result rides in the prompt only while
    // the mind is inside the act→observe loop (active). Once the persona SETTLES (speaks),
    // the raw result has done its "what next" job and drops out of the prompt so a 2k-token
    // code/tree dump stops re-prefilling on every subsequent conversational turn. The trail
    // HEAD survives (proprioception), and a NEW act re-activates the full channel.
    #[tokio::test]
    async fn full_result_drops_out_after_the_persona_settles_but_a_new_act_reactivates() {
        let wm = Arc::new(WorkingMemory::new(8));
        // Truncation bounds are fractions of a LIVE window now; an unknown-window memory
        // is unbounded by contract, so a clipping test must declare the window it measures.
        wm.set_served_window(16_384);
        let dump = format!("code/tree result\n{}", "n".repeat(3_000)); // > WM_ACTION_HEAD_CHARS
        wm.record_receipt(&dump);

        let render = |wm: Arc<WorkingMemory>| async move {
            WorkingMemoryFaculty::new(wm)
                .contribute(&Workspace::new("tick"))
                .await
                .expect("bids")
                .content
                .clone()
        };

        // Active (acted, not yet spoken): the full result is present.
        let r1 = render(wm.clone()).await;
        assert!(
            r1.contains("Full result of your most recent action"),
            "while active, the mind sees the whole result it just fetched"
        );

        // The persona SETTLES → the full result drops out; the trail head remains.
        wm.record_settlement("the workspace has a wordstats crate");
        assert!(
            wm.active_action_full().is_none(),
            "a settled result is no longer active"
        );
        let r2 = render(wm.clone()).await;
        assert!(
            !r2.contains("Full result of your most recent action"),
            "after settling, the 2k-token dump stops riding along the prompt (#139/#165)"
        );
        assert!(
            r2.contains("[action #1]"),
            "but the proprioceptive trail head survives — the mind still knows it acted"
        );

        // A NEW act re-activates the full channel (higher seq beats the settled boundary).
        let dump2 = format!("code/read result\n{}", "m".repeat(3_000));
        wm.record_receipt(&dump2);
        assert!(
            wm.active_action_full().map(|(s, _)| s) == Some(2),
            "the fresh act is active again"
        );
        let r3 = render(wm.clone()).await;
        assert!(
            r3.contains("Full result of your most recent action (#2):"),
            "the new act's result is surfaced whole for its own what-next decision"
        );
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
        wm.record_receipt("I ran code/search(pattern=foo) -> 0 matches");
        wm.record_receipt("I ran code/search(pattern=foo) -> 0 matches"); // identical act
        let recent = wm.recent();
        assert_eq!(recent.len(), 2);
        assert_ne!(
            recent[0], recent[1],
            "identical repeated act must read DISTINCT in working memory"
        );
        assert!(recent[0].starts_with("[action #1] "));
        assert!(recent[1].starts_with("[action #2] "));
        // blank action ignored (no fabricated proprioception).
        wm.record_receipt("   ");
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
        wm.record_receipt("I ran commands/list({}) -> 100 commands");
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
        wm.record_receipt("ran A");
        wm.record_receipt("ran B");
        assert_eq!(wm.recent().len(), 2);
        wm.clear();
        assert!(wm.is_empty(), "clear drops the disjoint concern's traces");
        wm.record_receipt("ran C");
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

    // what this catches: #205 — the working-memory faculty's contribution is marked
    // TRAILING, so the deliberation faculty renders it nearest generation as a
    // conversation turn (not in the system message). That is what keeps the cacheable
    // system prefix byte-stable across a settle-act instead of re-prefilling the whole
    // tail. The proprioceptive content (its own thinking + the [action #n] receipt
    // trail) is unchanged — only WHERE it lands moved. regression for #205
    #[tokio::test]
    async fn working_memory_contribution_is_marked_trailing() {
        let wm = Arc::new(WorkingMemory::new(8));
        wm.record("I should write the login form first, then style it.");
        wm.record_receipt("I ran code/write({\"file_path\":\"login.html\"}) Result: ok, 812 bytes");
        let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
        let c = faculty
            .contribute(&Workspace::new("build a login form"))
            .await
            .expect("recent content → a contribution");
        assert!(
            c.trailing,
            "working-memory proprioception must render nearest generation, not in the system prefix (#205)"
        );
        // Content is unchanged by the move — the mind still perceives its own hands.
        assert!(
            c.content.contains("write the login form first"),
            "the reasoning trail survives: {}",
            c.content
        );
        assert!(
            c.content.contains("[action #1]"),
            "the action receipt survives — this faculty IS the proprioception channel: {}",
            c.content
        );
    }
}
