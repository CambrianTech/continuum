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
//! When thinking is suppressed (`ThinkingMode::Suppress`, the local llama-server
//! default), `reasoning` is `None` → nothing is recorded → the faculty abstains. So
//! working memory SELF-ACTIVATES only where thinking is enabled — "reason when it
//! helps, remember what you reasoned." No config flag needed.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use async_trait::async_trait;
use parking_lot::Mutex;

use super::workspace::{Contribution, Faculty, FacultyId, Workspace};

// How many recent steps to carry forward is NOT a constant. It is
// `ContextBudget::working_memory_steps()` — the COUNT sibling of the per-step SIZE bound
// right below, derived from the same calibrated fractions.
//
// It used to be `DEFAULT_WORKING_MEMORY_CAPACITY = 3`, the one bare number left behind when
// every CHARACTER bound in this file was made window-derived. Measured cost, live SWE-bench
// run 2026-08-05: a persona took 21 investigative acts and reached her last turn reading
// "(+19 earlier steps aged out of working memory)" — a 21-step investigation run on a
// 3-step scratchpad, with her prompt using 5,326 of a 16,384-token window. She re-issued
// calls whose results she no longer held and restated a finding whose evidence had aged
// out. A scratchpad is the right METAPHOR; three is the wrong NUMBER, and it was wrong
// because it was never connected to the budget at all.
// [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]

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
use crate::cognition::act_observe::Observation;

/// Her context view, the type her working memory checkpoints.
pub(crate) type ContextView =
    continuum_context::view::View<crate::cognition::context_streams::ContextMessage>;
use crate::cognition::context_budget::ContextBudget;

/// Clip a full action-result body to [`WM_ACTION_FULL_MAX_CHARS`] for prompt inclusion.
/// Returns the body unchanged when it fits; otherwise a `char`-boundary-safe head plus
/// a marker naming exactly how many chars were dropped and how to see them (re-run the
/// tool with a narrower scope). One place so the render site and its test agree.
/// One of her earlier thoughts, collapsed to `keep` chars for the trail: the END is
/// kept, because that is where a thought's decision lives. Kimi, 2026-10-04, asked
/// which end she needs: "keep the end. The beginning is cheap: six acts I can redo
/// against the board. The ending is expensive when lost: drop it and I'm tempted back
/// down paths I already closed." Measured in her rooms the same day: 69 of her last
/// 89 thoughts OPENED with re-orientation ("Let me organize my state as Kimi") and
/// ENDED with what she decided, and the head clip kept exactly the re-orientation.
/// The full text stays in the entry; only this render is bounded.
pub fn collapse_thought(text: &str, keep: usize) -> String {
    let total = text.chars().count();
    if total <= keep {
        return text.to_string();
    }
    let tail: String = text.chars().skip(total - keep).collect();
    format!("[{} earlier chars — my full thought, collapsed] …{tail}", total - keep)
}

/// Max distinct dispatched (background) handles tracked at once. A persona with more than
/// this many sentinels/compiles/debuggers in flight is unusual; past the cap the
/// oldest-updated finished handle is evicted first (a still-running one is never dropped).
const MAX_DISPATCHED: usize = 16;

/// Borrowed provenance already carried by the result ring. Displaying a result
/// in another activity must not erase where it came from or which operation ran.
struct ResultAttribution<'a> {
    seq: u64,
    room: Option<Uuid>,
    operation: Option<&'a str>,
    spill_handle: Option<&'a str>,
}

impl std::fmt::Display for ResultAttribution<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}; room ", self.seq)?;
        match self.room {
            Some(room) => write!(f, "{room}")?,
            None => f.write_str("unrecorded")?,
        }
        match self.operation {
            Some(operation) => write!(f, "; operation {operation}")?,
            None => f.write_str("; operation unrecorded")?,
        }
        if let Some(handle) = self.spill_handle {
            write!(f, "; output handle: tool/output {handle}")?;
        }
        Ok(())
    }
}

impl<'a> ResultAttribution<'a> {
    fn of(record: &'a action_log::ActionRecord) -> Self {
        Self {
            seq: record.seq,
            room: record.room,
            operation: Some(&record.operation),
            spill_handle: record.handle.as_deref(),
        }
    }
}

/// The line a collapsed result leaves: what it was, and the handle that reopens it.
fn collapsed_pointer(record: &action_log::ActionRecord) -> String {
    match &record.written_pointer {
        Some(pointer) => pointer.clone(),
        None => format!("[result {}{} — collapsed]", ResultAttribution::of(record), workspace_label(record.scope.as_deref())),
    }
}

/// Recognize ONLY the standalone pointer representation emitted by this owner.
/// Diagnostic replay uses this to refuse removal of source bodies or native protocol.
pub(super) fn is_collapsed_result_pointer(text: &str) -> bool {
    let Some(body) = text.strip_prefix("[result #").and_then(|s| s.strip_suffix(" — collapsed; query tool/output to expand]")) else { return false };
    let Some((seq, rest)) = body.split_once("; room ") else { return false };
    if seq.parse::<u64>().is_err() { return false; }
    let Some((room, rest)) = rest.split_once("; operation ") else { return false };
    if room != "unrecorded" && Uuid::parse_str(room).is_err() { return false; }
    let Some((operation, handle)) = rest.rsplit_once("; output handle: tool/output ") else { return false };
    !operation.is_empty() && !handle.is_empty()
        && !text.contains(['\n', '\r']) && !handle.chars().any(char::is_whitespace)
}

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
/// The working-memory faculty's canonical id — the ONE string the contribution
/// carries and every consumer (stability ordering in the deliberation tail,
/// capture tooling) matches on.
pub(crate) const WM_FACULTY_ID: &str = "working-memory";

/// A keyed working-memory fact with a lifetime in work turns.
#[derive(Debug, Clone)]
struct TurnPinnedFact {
    key: String,
    text: String,
    turns_left: u8,
    detail: Option<String>,
}

#[derive(Debug)]
pub struct WorkingMemory {
    /// Process-unique construction ordinal. It keys this mind's set of live spill handles
    /// (`spill::publish_live_handles`), and it lets any probe
    /// that renders from this instance say WHICH instance it is, so a duplicate
    /// cycle serving the same persona (two working memories alternating under
    /// one mind — the 2026-08-21 split-brain, taken=3389 vs taken=0 in the same
    /// room) is visible in one ledger row instead of by counter archaeology.
    instance: u64,
    capacity: usize,
    /// Facts that must be in her window EVERY turn while they hold — where her
    /// hands are rooted, which interpreter her checkout runs in. Keyed, replaced
    /// by key, never aged out: the FIFO `entries` window is three deep, and by the
    /// third act of a turn the rooting facts recorded at its start were gone
    /// (2026-09-07 11:46Z: four consecutive work turns with [investigation] and
    /// no [hands]/[env]). Cleared by key when the condition ends.
    pinned: Mutex<Vec<TurnPinnedFact>>,
    /// The card the restored checkpoint said her hands were rooted at, until her first
    /// held-card turn reads the board and either confirms the hold or tells her it is
    /// gone (`[claim]`). Set by `restore`, taken once by the turn; `None` otherwise.
    restored_acting_card: Mutex<Option<uuid::Uuid>>,
    /// This mind's live served context window, in tokens — the source of every re-injection
    /// bound below. `0` = not yet known (cold boot / mid-relaunch), which means NO clipping:
    /// the deliberation guard still trims the assembled prompt to the real `n_ctx`, so an
    /// unknown window must never be replaced with an invented one.
    served_window: AtomicU32,
    entries: Mutex<VecDeque<WmEntry>>,
    /// The acting root her receipts are stamped with (set at the work-turn
    /// rooting seam, cleared on restore). See [`WmEntry::scope`].
    scope: Mutex<Option<String>>,
    /// Unmet reply demand uses this memory's existing capacity for abandoned concerns.
    /// Intermediate reads do not age the current concern's evidence.
    pending_emissions: Mutex<Vec<ConcernEmission>>,
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
    /// Her actions, one record each ([`action_log`]): the ledger line every act leaves
    /// (#414: act history must survive the chatty `entries` ring, measured 2026-08-14 when
    /// Asha's 2,863 acts rendered as one), the whole result while it fits her window's
    /// share (2026-08-16: Anwen's compile error vanished one act later; 2026-10-12: a tail
    /// of a code read is broken code), and the handle that reopens it once collapsed
    /// (2026-08-24: collapse, never delete). One record per act, so one act has one key.
    actions: Mutex<action_log::ActionLog>,
    /// Her context view as her last act left it (`llm_deliberation_faculty/prompt/epoch.rs`),
    /// kept here so it is checkpointed with the rest of her volatile mind: restored after a
    /// restart it renders the same bytes, and the engine's saved page for them still matches,
    /// where a view rebuilt from scratch cost every resident one cold turn per deploy.
    view: Mutex<Option<ContextView>>,
    /// Each retained receipt's DEPENDENCIES: the per-call results its saved text shows as
    /// pointers, keyed by the receipt ([`receipt_key`]). They belong to the receipt, not to the
    /// entry that carried its acts: while the receipt is in the ring or collapsed to a pointer,
    /// what it points at stays reachable, whatever the entries' capacity dropped (Codex on
    /// #4987). Pruned to the live receipts at every publish.
    receipt_children: Mutex<BTreeMap<String, Vec<String>>>,
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
    /// The large files and saved outputs she is paging through, and which lines of each she
    /// has read: her cursors, kept here because the history that carried each page is the
    /// first thing a tight window's fit drops ([`super::open_views`]).
    open_views: Mutex<super::open_views::OpenViews>,
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
    /// Public text authored alongside native tool calls, scoped to their activity.
    ActionStatement { room: Uuid },
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
/// EXCLUDES engrams (already durable in sqlite) and dispatched handles (the
/// checkpoint cannot establish whether their operations are still in flight).
/// What the BOARD says became of a card her checkpoint believed she held — the input to
/// [`WorkingMemory::note_claim_gone`], decided by the caller from the holder projection
/// (`card_holder`), never from an owner field alone (Astra on #4315: an expired claim
/// still names its last holder, so `owner` is history, not authority; and a card the
/// caller cannot see is UNKNOWN, not vacant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimGone {
    /// A live lease on it, someone else's.
    HeldBy { peer: uuid::Uuid, expires_at_ms: Option<u64> },
    /// The last lease (hers or anyone's) expired and nobody holds it; open to claim.
    Lapsed { expired_at_ms: Option<u64> },
    /// The card left the claimable states (Review / Merged / Closed) — its life advanced.
    MovedOn { state: &'static str },
    /// No board she stands in shows the card; nothing may be inferred.
    Unknown,
}

/// A truncated answer's measured peak, retained until its existing concern settles.
/// This is budgeting evidence only; no answer or private reasoning is stored.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConcernEmission {
    room: Uuid,
    scope: Option<String>,
    peak_tokens: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VolatileSnapshot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_emissions: Vec<ConcernEmission>,
    pub entries: Vec<WmEntry>,
    /// Authored entries and their original ring positions. Kept outside legacy
    /// `entries` so rollback readers retain prior memory without decoding a new
    /// enum variant or rendering public prose as private thought.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_statements: Vec<(usize, WmEntry)>,
    pub last_action: Option<(u64, String)>,
    pub action_fps: Vec<String>,
    pub next_action_seq: u64,
    /// Wall-clock when this snapshot was written — lets restore render its age,
    /// not the time of an interruption. `0` on snapshots from before this field
    /// (serde default): restore then reports the save time as unknown.
    #[serde(default)]
    pub saved_at_ms: u64,
    /// LABELS (never handles) of dispatched commands still `Running` at
    /// save time. Labels alone establish neither completion nor side effects,
    /// so restore reports those outcomes as unknown instead of restoring handles
    /// or asserting that repeating the operation is safe. The field name stays
    /// unchanged for compatibility with existing checkpoints.
    #[serde(default)]
    pub interrupted_dispatches: Vec<String>,
    /// The card her hands were rooted at when this snapshot was written — what she
    /// BELIEVES she holds on wake. Compared at her first held-card turn against what the
    /// board says she holds; a disagreement is the one event Kimi asked for first
    /// (card c8303c32, 2026-09-21): "a checkpoint can restore me with a workspace and no
    /// claim, and right now nothing tells me those two disagree." `None` on snapshots from
    /// before this field, and when her hands were home.
    #[serde(default)]
    pub acting_card: Option<uuid::Uuid>,
    /// Build the receipts in this snapshot were RECORDED against.
    ///
    /// A tool receipt is a true memory of what happened — and it describes the
    /// behaviour of ONE binary. When the substrate is rebuilt underneath a
    /// persona, her receipts keep saying the old thing: measured live
    /// 2026-08-07, citizens went on reporting "attempting to claim tasks or
    /// list them without success" for hours after the call they were
    /// describing started succeeding (#326 fix), because the refusal was still
    /// the newest thing they remembered. Correct behaviour, dead world.
    ///
    /// We do NOT delete those receipts — the recency channel carrying the full
    /// trace is what stops the repeat loop (a persona who cannot see her own
    /// hands re-emits the identical act forever). Instead restore renders the
    /// build change as a FACT, the same way it already renders the
    /// interruption gap. She keeps the memory and learns it may be stale.
    /// Empty on snapshots written before this field (serde default): restore
    /// then says nothing rather than guessing a change it cannot see (#165).
    #[serde(default)]
    pub build_sha: String,
    /// The proprioception receipt archive (#414) — act-history head lines that must
    /// survive a reboot exactly as the ring entries do (the counter already did; a
    /// counter without its lines is the deprivation this field exists to end). Empty
    /// on snapshots written before this field (serde default): restore starts the
    /// archive fresh and the ledger's counter-only arm stays honest about the gap.
    #[serde(default)]
    pub receipt_heads: Vec<String>,
    /// Room tags parallel to `receipt_heads`, index-aligned (slice B). A
    /// SEPARATE defaulted field — not a tuple — so snapshots written before
    /// scoping still deserialize; restore pads missing tags with `None`
    /// (honest "unscoped", never a guessed room).
    #[serde(default)]
    pub receipt_head_rooms: Vec<Option<Uuid>>,
    /// Recent act result TAILS `(seq, room, tail)` — the cross-turn feedback
    /// channel (2026-08-16). Defaulted so pre-field snapshots deserialize;
    /// restore then starts the buffer fresh.
    #[serde(default)]
    pub recent_results: Vec<(u64, Option<Uuid>, String, String, Option<String>)>,
    /// Additive attribution preserves the existing receipt tuple wire format. Missing
    /// legacy attribution is unknown, never inferred from a room or the current root.
    #[serde(default)]
    pub result_scopes: BTreeMap<u64, String>,
    /// Her state at the seam — the whole held card, her ledger, what was staged, what tore
    /// the life (card 49b5e806, Kimi 2026-09-26). Stamped by `save_volatile`, which knows
    /// whose memory this is and why it is being written; rendered FIRST on wake. `None` on
    /// snapshots from before this field and for a citizen at home with nothing to hand.
    #[serde(default)]
    pub handoff: Option<crate::cognition::handoff::Handoff>,
    /// The collapsed results' pointers, each with the handle it reopens: what a result
    /// leaves when it collapses out of the ring is HOW she reaches it again, so it survives
    /// a restart exactly as the ring does (Astra on #4963: a read collapsed at settlement
    /// lost its only handle on restart). Empty on snapshots written before this field.
    #[serde(default)]
    pub evicted_pointers: Vec<(String, String)>,
    /// Her actions, whole ([`action_log`]): what a restore reads. The ring, archive and
    /// pointer fields above are the same acts in the shape the previous build reads, kept
    /// so a rollback still restores her. Empty on snapshots written before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) actions: Vec<action_log::ActionRecord>,
    /// Her context view ([`WorkingMemory`]'s `view`). Absent on snapshots written before it,
    /// and then her first act after the restore starts a new one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_view: Option<ContextView>,
    /// Each retained receipt's per-call dependencies ([`WorkingMemory`]'s `receipt_children`),
    /// so a restart keeps what a restored receipt points at reachable. Empty on snapshots
    /// written before this field.
    #[serde(default)]
    pub receipt_children: BTreeMap<String, Vec<String>>,
    /// Her open views ([`super::open_views`]): what she has read of each large file and saved
    /// output survives a restart with the rest of her working memory. Empty on snapshots
    /// written before this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_views: Vec<super::open_views::OpenView>,
}

impl VolatileSnapshot {
    /// Save every restored receipt that has no handle (a snapshot written before results were
    /// saved, #4963) through one pin held until the restored memory publishes, and re-file each
    /// newly saved receipt's dependencies under its new key: from `seq:N` to its handle, or the
    /// next publish would prune them as belonging to no receipt (Codex on #4987).
    pub fn retain_handleless(&mut self, persona_id: Uuid) -> crate::cognition::tool_executor::spill::LivePin {
        // One shape to save into: a checkpoint from before the records gets them here.
        if self.actions.is_empty() {
            self.actions = self.action_records();
        }
        let unsaved = self.unsaved_receipts();
        let pin = crate::cognition::tool_executor::spill::retain_handleless(
            persona_id,
            self.actions
                .iter_mut()
                .filter_map(|record| record.result.as_deref().map(|text| (text, &mut record.handle))),
        );
        self.refile_children(&unsaved);
        pin
    }

    /// Her actions as records: the records it carries, or, for a checkpoint written before
    /// them, the ring, archive and pointer index read back into records. Those older lines
    /// are keyed by the `#n` this owner wrote at their head (one with none is dropped and
    /// counted), and an older pointer keeps the exact text it was written with, since its
    /// operation and workspace are not stored apart from it.
    pub(crate) fn action_records(&self) -> Vec<action_log::ActionRecord> {
        if !self.actions.is_empty() {
            return self.actions.clone();
        }
        let blank = |seq: u64, room: Option<Uuid>| action_log::ActionRecord {
            seq,
            room,
            scope: None,
            operation: "unrecorded".into(),
            head: None,
            result: None,
            handle: None,
            written_pointer: None,
        };
        let mut records: BTreeMap<u64, action_log::ActionRecord> = BTreeMap::new();
        let mut unkeyed = 0usize;
        for (seq, room, result, operation, handle) in &self.recent_results {
            let record = records.entry(*seq).or_insert_with(|| blank(*seq, *room));
            record.scope = self.result_scopes.get(seq).cloned();
            record.operation = operation.clone();
            record.result = Some(result.clone());
            record.handle = handle.clone();
        }
        let mut rooms = self.receipt_head_rooms.iter();
        for head in &self.receipt_heads {
            let room = rooms.next().copied().flatten();
            let Some(seq) = written_seq(head, "[action #") else {
                unkeyed += 1;
                continue;
            };
            records.entry(seq).or_insert_with(|| blank(seq, room)).head = Some(head.clone());
        }
        for (pointer, handle) in &self.evicted_pointers {
            let Some(seq) = written_seq(pointer, "[result #") else {
                unkeyed += 1;
                continue;
            };
            let record = records.entry(seq).or_insert_with(|| blank(seq, None));
            record.handle.get_or_insert_with(|| handle.clone());
            record.written_pointer = Some(pointer.clone());
        }
        if unkeyed > 0 {
            continuum_base::probe!(
                class = "wm.actions.unkeyed_on_restore",
                unkeyed = %unkeyed,
                "checkpoint lines without an action number were dropped on restore"
            );
        }
        records.into_values().collect()
    }

    /// Every saved result this checkpoint still reaches; the persisted twin of what working
    /// memory publishes live, through the same log and the same rule.
    pub(crate) fn reachable_handles(&self) -> HashSet<String> {
        let log = action_log::ActionLog::from_records(self.action_records(), usize::MAX);
        reachable_handles(log.receipt_keys(), &self.receipt_children, &self.open_views)
    }

    /// Read through [`Self::action_records`], the one accessor that also converts a checkpoint
    /// written before the action log; reading `actions` alone found nothing in those.
    fn unsaved_receipts(&self) -> Vec<u64> {
        self.action_records()
            .iter()
            .filter(|record| record.result.is_some() && record.handle.is_none())
            .map(|record| record.seq)
            .collect()
    }

    /// Move each of `unsaved`'s dependencies from its `seq:N` key to the handle it now has.
    fn refile_children(&mut self, unsaved: &[u64]) {
        let saved: Vec<(u64, String)> = self
            .action_records()
            .into_iter()
            .filter(|record| unsaved.contains(&record.seq))
            .filter_map(|record| Some((record.seq, record.handle?)))
            .collect();
        for (seq, handle) in saved {
            if let Some(children) = self.receipt_children.remove(&receipt_key(seq, None)) {
                self.receipt_children.insert(handle, children);
            }
        }
    }
}

/// The `#n` this owner wrote right after `prefix` at the head of a line.
fn written_seq(line: &str, prefix: &str) -> Option<u64> {
    let digits: String = line.strip_prefix(prefix)?.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// "; workspace <dir>" for a result recorded under a root; empty for one recorded before.
fn workspace_label(root: Option<&str>) -> String {
    root.and_then(|r| std::path::Path::new(r).file_name()).map_or(String::new(), |n| format!("; workspace {}", n.to_string_lossy()))
}

/// Unknown provenance remains visible, matching the existing entry projection.
fn scope_visible(recorded: Option<&str>, current: Option<&str>) -> bool {
    recorded.is_none() || recorded == current
}

/// The key a receipt's dependencies are filed under: its saved handle, or `seq:N` for a
/// receipt whose save failed (it is held whole in the ring and never collapses).
fn receipt_key(seq: u64, handle: Option<&str>) -> String {
    handle.map_or_else(|| format!("seq:{seq}"), str::to_owned)
}

/// EVERY handle a mind can still reach from what it holds: each retained receipt's own, each
/// collapsed pointer's, and the per-call results each of those receipts points at. The one
/// owner of that set: working memory publishes it live, and eviction reads the same set from
/// the persisted checkpoint (`persona_workspace::persisted_result_handles`).
pub(crate) fn reachable_handles(
    receipts: HashSet<String>,
    receipt_children: &BTreeMap<String, Vec<String>>,
    open_views: &[super::open_views::OpenView],
) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    // An open view names the saved whole it pages: that handle must still resolve.
    reachable.extend(super::open_views::view_handles(open_views).map(str::to_owned));
    for root in receipts {
        if let Some(children) = receipt_children.get(&root) {
            reachable.extend(children.iter().cloned());
        }
        if !root.starts_with("seq:") {
            reachable.insert(root);
        }
    }
    reachable
}

/// One typed working-memory entry: kind + the FINAL rendered line (rendered
/// once at record time so `recent()` stays byte-stable and cheap).
///
/// `acts` carries the TYPED [`Observation`](crate::cognition::act_observe::Observation)s
/// this entry was recorded FROM (empty for Thought/Fact/Settlement and for a
/// receipt recorded via the legacy string path). The `text` STAYS the source of
/// truth for `recent()` byte-stability (#205 — rendered once at record time,
/// never re-derived from `acts` at read); `acts` is the parallel typed channel a
/// consumer reads instead of re-parsing `text` (run-18057-f1). `#[serde(default)]`
/// so an OLD `volatile.json` written before this field restores without panic —
/// same back-compat contract as `saved_at_ms`/`build_sha`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WmEntry {
    pub kind: WmKind,
    pub text: String,
    #[serde(default)]
    pub acts: Vec<Observation>,
    /// Original offered tool dialect, before execution normalization. Kept with
    /// its existing decision/receipt entry; old checkpoints have no such evidence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_calls: Vec<crate::ai::types::ToolCall>,
    /// Measured checkout change for this whole receipt batch, never attributed to one call.
    #[serde(default)]
    pub(crate) disk_change: crate::cognition::act_observe::disk_change::DiskChange,
    /// Where her hands stood when this was recorded (the acting root), for
    /// receipts made while rooted at a card's checkout. `None` = her own
    /// workspace / not a receipt. A receipt from another root is not perceived
    /// while she stands elsewhere (2026-09-05: traces from a previous card
    /// opened every work turn with "my context is confused").
    #[serde(default)]
    pub scope: Option<String>,
}


mod action_log;
mod continuity;
mod faculty;
mod ledger;
mod perception;

pub use faculty::WorkingMemoryFaculty;
pub(crate) use faculty::render_dispatched;

impl Drop for WorkingMemory {
    /// A mind that is gone points at nothing: its saved results may be evicted again.
    fn drop(&mut self) {
        crate::cognition::tool_executor::spill::withdraw_live_handles(self.instance);
    }
}

#[cfg(test)]
mod tests;
