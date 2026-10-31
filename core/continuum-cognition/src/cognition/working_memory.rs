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
//! - Her view's sources (`context_streams`: thoughts, actions) read it each act, so the
//!   deliberator perceives her recent reasoning and her own acts as items of her view.
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use parking_lot::Mutex;


// How many recent steps to carry forward is NOT a constant. It is
// `ContextBudget::WORKING_MEMORY_STEPS` — the COUNT sibling of the per-step SIZE bound
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
    pub fn of(record: &'a action_log::ActionRecord) -> Self {
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
    format!("[result {}{} — collapsed]", ResultAttribution::of(record), workspace_label(record.scope.as_deref()))
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


/// A keyed working-memory fact with a lifetime in work turns.
#[derive(Debug, Clone)]
struct TurnPinnedFact {
    key: String,
    text: String,
    turns_left: u8,
    detail: Option<String>,
}

/// A bounded, rolling buffer of the persona's recent reasoning. Cheap to clone the `Arc`;
/// written by the deliberation faculty and read by her view's sources (`context_streams`).
/// `parking_lot::Mutex` — no poisoning, and every access is a quick sync snapshot/push with no
/// `.await` held across the lock.
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
    /// Keys of pins whose condition ended (unpinned, or their turns ran out), newest last,
    /// until re-pinned: her view retires each one's notice instead of showing an ended
    /// condition until its next turnover. Bounded by the pin keys in use (a handful).
    ended_pins: Mutex<Vec<String>>,
    /// The card the restored checkpoint said her hands were rooted at, until her first
    /// held-card turn reads the board and either confirms the hold or tells her it is
    /// gone (`[claim]`). Set by `restore`, taken once by the turn; `None` otherwise.
    restored_acting_card: Mutex<Option<uuid::Uuid>>,
    /// This mind's live served context window, in tokens — the source of every re-injection
    /// bound below. `0` = not yet known (cold boot / mid-relaunch), which means NO clipping:
    /// the deliberation guard still trims the assembled prompt to the real `n_ctx`, so an
    /// unknown window must never be replaced with an invented one.
    entries: Mutex<VecDeque<WmEntry>>,
    /// The next entry occurrence (`WmEntry::occurrence`): one sequence for every entry, kept in
    /// the checkpoint so a restore and the next append never share one.
    next_entry_seq: AtomicU64,
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
    /// "what I sent away, and where it's at" each heartbeat. A finished handle stays until a submitted render receipt shows its end reached her.
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
    task_results: Mutex<task_results::TaskResults>,
    incarnation: Uuid,
    /// Her learned tool head: every call she makes is noted here, and the deliberation faculty
    /// reads and adopts the head at each epoch turnover ([`super::tool_head`]).
    tool_head: Arc<super::tool_head::ToolHead>,
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
    /// Retained results `(seq, room, result, operation, handle)`, read only by checkpoint
    /// adoption's validation (`persona_workspace::checkpoint_adoption`). It goes when adoption's
    /// writes move onto their disk class handle (ONE-BUDGET row 5, card 2b326931; then adoption reads
    /// `actions`): the one shape is `actions`.
    #[serde(default)]
    pub recent_results: Vec<(u64, Option<Uuid>, String, String, Option<String>)>,
    /// Her state at the seam — the whole held card, her ledger, what was staged, what tore
    /// the life (card 49b5e806, Kimi 2026-09-26). Stamped by `save_volatile`, which knows
    /// whose memory this is and why it is being written; rendered FIRST on wake. `None` on
    /// snapshots from before this field and for a citizen at home with nothing to hand.
    #[serde(default)]
    pub handoff: Option<crate::cognition::handoff::Handoff>,
    /// Her actions, whole ([`action_log`]): what a restore reads, the one shape (no projection
    /// for another build: this is the first version, Joel 2026-10-31).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) actions: Vec<action_log::ActionRecord>,
    /// Her context view ([`WorkingMemory`]'s `view`), derived: absent, or no longer decodable
    /// (`derived_view`), her first act after the restore starts a new one.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "derived_view::or_new_epoch")]
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
    /// Consumed terminal references and outstanding wake obligations, checkpointed together.
    pub(crate) task_results: task_results::TaskResults,
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

    /// Price the whole of every result an earlier life evicted before `stored_chars` was kept:
    /// its body left RAM for its handle by the old window share (Aiko on the M5, 2026-10-29),
    /// so its whole had no price and showed at pointer level. `whole_of` measures the saved
    /// output a handle names (`None`: not saved or unreadable, which stays a pointer, honestly).
    /// Once per restore, before the mind acts, so no read happens under an inference grant.
    pub fn price_evicted_wholes(&mut self, whole_of: impl Fn(&str) -> Option<usize>) {
        if self.actions.is_empty() {
            self.actions = self.action_records();
        }
        for record in self.actions.iter_mut().filter(|r| r.result.is_none() && r.stored_chars.is_none()) {
            if let Some(handle) = record.handle.as_deref() {
                record.stored_chars = whole_of(handle);
            }
        }
    }

    /// Her actions as records, as the checkpoint carries them.
    pub(crate) fn action_records(&self) -> Vec<action_log::ActionRecord> {
        self.actions.clone()
    }

    /// Every saved result this checkpoint still reaches; the persisted twin of what working
    /// memory publishes live, through the same log and the same rule.
    pub(crate) fn reachable_handles(&self) -> HashSet<String> {
        let log = action_log::ActionLog::from_records(self.action_records());
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
    /// This entry's occurrence, stamped once by the one owner of entry identity
    /// (`WorkingMemory::push_entry`, from `next_entry_seq`): two entries that say the same
    /// thing are two occurrences, so a settlement that repeats an earlier one is its own item
    /// in her view (passed, failed, passed reads as passed). Never a content hash. Never in a
    /// checkpoint: an occurrence names an item within one life's view, a restart begins a new
    /// epoch that re-renders every item, and restore numbers each entry it brings back from
    /// the same counter, so the checkpoint's shape does not change.
    #[serde(skip)]
    pub occurrence: u64,
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


mod derived_view;
mod action_log;
mod continuity;
mod task_results;
pub use task_results::{TaskReceiptConflict, TaskReceiptError};
mod ledger;
mod perception;
mod residency;

pub use action_log::ActOutputRefusal;
pub(crate) use ledger::operation_level;
pub(crate) use residency::{ResidentResults, ShedCandidate};

impl Drop for WorkingMemory {
    /// A mind that is gone points at nothing: its saved results may be evicted again.
    fn drop(&mut self) {
        crate::cognition::tool_executor::spill::withdraw_live_handles(self.instance);
    }
}

#[cfg(test)]
mod tests;
