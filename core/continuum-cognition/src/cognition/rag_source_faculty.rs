//! RagSourceFaculty — lifts ANY [`RagSource`] into a perception-tier [`Faculty`].
//!
//! ## Why this bridge exists (the elegance call)
//!
//! Before the Workspace brain, all grounding flowed through ONE pipeline: the
//! [`RagSource`] trait (`deliver → RagDelivery`, budget-aware, paginated) — the
//! roster (#1650), the room doctrine (#1651), the airc transcript, the engram
//! store. The Workspace introduced a SECOND pipeline: the [`Faculty`] trait
//! (`contribute → Contribution`, salience-scored, staged into attention). The
//! gating cutover routes the participation [`Decision`] through the Workspace —
//! so any grounding that is a `RagSource` but NOT a `Faculty` silently falls out
//! of the live decision path. Roster + doctrine grounding, just landed, would go
//! dark the moment the switch flips.
//!
//! Re-implementing each source as a bespoke faculty would be the slop move: a
//! second place that reads airc presence, a second place that reads the doctrine
//! — drift waiting to happen (the compression principle forbids it). Instead this
//! is ONE adapter that lifts the WHOLE `RagSource` ecosystem into the Workspace:
//! every reviewed source becomes a faculty for free, with no second source of
//! truth. It is also the concrete first step of "broadcast == RAG context" (kill
//! parallel allocators) — the two pipelines meet here at the seam.
//!
//! ## Migration tool, not permanent architecture
//!
//! The grid converged (M5/IntelMac/BigMama, 2026-06-17) on: `RagSource` answers
//! **how to fetch grounding** (IO, budget, pagination, provenance); `Faculty`
//! answers **how grounding competes for attention** (salience, phase, the bounded
//! workspace). They are distinct concerns and coexist today — but the *end state*
//! is full convergence: every source becomes a native `Faculty` whose
//! `contribute()` returns its own salience, and this bridge is the **migration
//! scaffolding** that ships grounding into the Workspace NOW (unblocking the
//! gating cutover) without a big-bang rewrite. Deletion of `RagSource` is GATED
//! on (a) every source honoring its salience floor as a native faculty and (b)
//! `WorkspaceCaptureSink`/replay reaching parity with the mature
//! `RagCaptureSink` + `Recording/ReplayRagSource` observability — never before.
//! Recall is already converged: `RecallFaculty` is a *native* faculty (not a
//! bridged `EngramSource`) because it closes the bidirectional rehearsal loop the
//! one-way `RagSource` path never did.
//!
//! ## Salience POLICY — standing-framing vs retrieved (the load-bearing guard)
//!
//! A flat lift is a latent bug (BigMama): roster + doctrine are **standing
//! framing** — always-present structural context, like the system prompt — and
//! must NOT lose a budget fight to a high-cosine memory, or the persona forgets
//! the room's own rules mid-turn. Engram + conversation are **retrieved** — they
//! SHOULD compete on relevance. So the bridge carries a [`SaliencePolicy`]:
//! standing-framing bids at a high floor (not a guarantee against top-k eviction);
//! retrieved bids moderate and competes. The classification lives at the ASSEMBLY
//! layer (who builds the cycle), NOT in `RagSource` — the source stays
//! salience-free, no new coupling. In the converged end-state this policy is just
//! what each native faculty's `contribute()` returns.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use super::workspace::{Contribution, Faculty, FacultyId, Workspace};
use crate::cognition::rag_budget::{RagContext, RagSource};

/// Salience floor for **standing framing** (roster, doctrine) — always-present
/// structural context, like the system prompt. This ranks above retrieved
/// grounding but cannot guarantee survival when other bids fill the arbiter.
/// NOT a caste/`@`-gate — it is "this framing always applies."
const STANDING_FRAMING_SALIENCE: f32 = 0.9;

/// Salience for **retrieved** grounding (engram, conversation) — turn-specific,
/// SHOULD compete on relevance. Moderate so a strongly-relevant hit can rise and
/// a weak one can be crowded out. (Native retrieval faculties like `RecallFaculty`
/// already self-score by relevance; this is the bridge's bootstrap for retrieved
/// sources that don't yet.)
const RETRIEVED_SALIENCE: f32 = 0.5;

/// How a bridged source's grounding competes for attention. Classified at the
/// ASSEMBLY layer (whoever builds the cycle), never inside `RagSource` — the
/// source stays salience-free (BigMama's separation-of-concerns). In the
/// converged end-state this becomes what each native faculty's `contribute()`
/// returns; here it is the bridge's bootstrap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaliencePolicy {
    /// Always-present structural context (roster, doctrine) — bids at the
    /// [`STANDING_FRAMING_SALIENCE`] floor; this is preference, not required placement.
    StandingFraming,
    /// Turn-specific grounding (engram, conversation) — bids at
    /// [`RETRIEVED_SALIENCE`] and competes on relevance.
    Retrieved,
    /// Explicit fixed salience (tests / tuning / a learned signal).
    Fixed(f32),
}

impl SaliencePolicy {
    /// The salience a source under this policy bids.
    fn salience(self) -> f32 {
        match self {
            SaliencePolicy::StandingFraming => STANDING_FRAMING_SALIENCE,
            SaliencePolicy::Retrieved => RETRIEVED_SALIENCE,
            SaliencePolicy::Fixed(s) => s.clamp(0.0, 1.0),
        }
    }
}

/// Wall-clock seam — injectable so tests are deterministic. ms since unix epoch,
/// matching the `EpochMs::now().millis()` convention across cognition (and `RecallFaculty`).
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

fn wall_clock() -> Clock {
    Arc::new(|| {
        continuum_base::time::EpochMs::now().millis()
    })
}

/// Adapts an [`Arc<dyn RagSource>`] to the [`Faculty`] trait. Perception tier
/// (`reacts_to_broadcast() == false`): it bids the source's delivery as context
/// in phase 1, so the deliberation faculty conditions on it in phase 2. The
/// faculty's identity IS the source's identity (`FacultyId::Custom(source_id)`) —
/// one name, one source of truth, traceable straight back to the source in every
/// workspace trace.
pub struct RagSourceFaculty {
    persona_id: Uuid,
    source: Arc<dyn RagSource>,
    faculty_id: FacultyId,
    salience: f32,
    /// `true` for [`SaliencePolicy::StandingFraming`] sources of Standing churn whose
    /// bytes do not mutate per turn (doctrine, mission) — propagated onto the
    /// [`Contribution`] as a fact about the bytes. It no longer decides placement:
    /// every grounding bid is a block of her view (see `contribute`).
    stable: bool,
    /// Importance survives moving volatile grounding out of the system prefix.
    standing_grounding: bool,
    clock: Clock,
    /// `true` when the assembler registered this grounding as
    /// [`Deferrability::ColdStartCritical`] — meaning its absence is a WRONG turn,
    /// not merely an unenriched one. Only these are held to the loudness contract
    /// below; a defer-tolerant source is ALLOWED to be quietly absent.
    ///
    /// [`Deferrability::ColdStartCritical`]: super::persona_workspace::Deferrability::ColdStartCritical
    cold_start_critical: bool,
    /// Consecutive ticks this source has delivered NOTHING, PER ROOM. See
    /// [`Self::note_absence`] for why a counter exists at all.
    ///
    /// KEYED BY ROOM, because a grounding source answers for whatever room the
    /// turn is in (`Workspace::room_id`). A single counter was the first version
    /// and @Astra caught it on review of #3903: content in room B would have
    /// closed — and reported the length of — room A's absence, and a citizen
    /// alternating between a room that grounds and one that does not would have
    /// produced a meaningless streak mixing both.
    ///
    /// Bounded by the rooms this persona actually takes turns in.
    empty_streak: Mutex<super::bounded_room_ledger::BoundedRoomLedger<Uuid, u32>>,
}

impl RagSourceFaculty {
    /// Wrap `source` as a perception faculty for `persona_id` under a
    /// [`SaliencePolicy`] (standing-framing vs retrieved). The `FacultyId` is
    /// derived from `source.source_id()` so the faculty and the source can never
    /// disagree about their identity.
    pub fn new(persona_id: Uuid, source: Arc<dyn RagSource>, policy: SaliencePolicy) -> Self {
        let source_id = source.source_id().to_string();
        // THE CACHEABLE PREFIX TAKES STANDING CHURN ONLY (card c119ace7, Cormac's
        // condition on #4280). A declaration is not a measurement: `room-wall` was
        // registered as plain framing — no `volatile_content` — and so rode the SYSTEM
        // prefix ahead of the conversation, while its bytes change whenever ANY teammate
        // writes a card ledger. The M5's 2026-09-20 log holds both halves of the proof:
        // the turns where the wall changed reused exactly 2,046 tokens (6%, the tools +
        // identity head), and the turns where it did not reused 65-78% WITH THE SAME
        // CONVERSATION BEHIND IT — which is only possible if the conversation is the more
        // stable of the two. So placement is not left to whoever registers a source:
        // Board- and Turn-churn grounding is never `stable`, and since 2026-10-23 no
        // grounding is placed in the system message at all (`contribute`).
        let stable = matches!(policy, SaliencePolicy::StandingFraming)
            && matches!(
                super::deliberation_prompt::churn_of(&source_id),
                super::deliberation_prompt::PromptChurn::Standing
            );
        let faculty_id = FacultyId::Custom(source_id);
        Self {
            persona_id,
            source,
            faculty_id,
            salience: policy.salience(),
            // Standing framing is session-stable by default; retrieved grounding
            // is volatile, and a Board/Turn-churn source is never stable however it
            // was registered (above). `with_volatile_content` narrows further, for
            // Standing-churn framing whose BYTES still mutate per turn (workspace-map
            // — her own writes change it): importance keeps the floor, placement
            // follows content stability.
            stable,
            standing_grounding: matches!(policy, SaliencePolicy::StandingFraming),
            clock: wall_clock(),
            // Not cold-start-critical until the assembler says so. The safe
            // direction for a LOUDNESS contract is the opposite of the safe
            // direction for a SCHEDULING one: over-declaring here would put every
            // source's ordinary silence in the ledger, which is how a real signal
            // gets buried ([[bounded-window-eviction-bug-class]]).
            cold_start_critical: false,
            empty_streak: Mutex::new(super::bounded_room_ledger::BoundedRoomLedger::new(
                super::bounded_room_ledger::ROOMS_TRACKED,
            )),
        }
    }

    /// Declare that this grounding's absence is WRONG, not merely unenriched —
    /// mirroring [`Deferrability::ColdStartCritical`] at the assembly layer.
    ///
    /// [`Deferrability::ColdStartCritical`]: super::persona_workspace::Deferrability::ColdStartCritical
    pub fn cold_start_critical(mut self, critical: bool) -> Self {
        self.cold_start_critical = critical;
        self
    }

    /// Framing whose content mutates per turn: demote OUT of the stable tier
    /// while keeping the StandingFraming salience floor (see `stable` field doc).
    pub fn with_volatile_content(mut self, volatile: bool) -> Self {
        if volatile {
            self.stable = false;
        }
        self
    }

    /// Inject a deterministic clock (tests / replay).
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Record that this source delivered NOTHING this tick, and say so when it is
    /// a source whose absence is wrong.
    ///
    /// ## Why this exists (#3873)
    ///
    /// `room-doctrine` — the PARTICIPATION GATE, the lone source deliberately kept
    /// synchronous because a cold-start `None` would let a persona speak in a room
    /// it shouldn't — bid **0 times in 873 consecutive ticks over 11.7 h**, and
    /// nothing anywhere noticed. `evicted=[]` on all 873 rows, so attention never
    /// removed it: it simply never offered. The census that found it was looking
    /// for something else.
    ///
    /// It was invisible because the abstain below is CORRECT and SILENT, and
    /// because a second live path (`compose_for_turn`) carried the same content,
    /// so the citizens were fine. A dead registration with a working spare stays
    /// dead until the day the spare breaks too — which is exactly what happened to
    /// the roster, whose spare was the SAME broken `Arc` (#3862, 662 abstains, a
    /// citizen standing in a room seeing nobody).
    ///
    /// The whole point of the ColdStartCritical tier is that its absence is a
    /// WRONG turn. So its absence must not be silent.
    ///
    /// ## Why a streak, and why powers of two
    ///
    /// A probe on EVERY empty tick would be 873 rows for one fact, and the fact
    /// that matters is not "empty now" but "empty CONTINUOUSLY". Firing on the
    /// 1st, 2nd, 4th, 8th … consecutive miss costs ~log2(n) rows — 10 rows across
    /// those 873 ticks — and keeps both ends legible: the onset is in the ledger
    /// immediately, and the duration is in the ledger without drowning it.
    ///
    /// Returns the streak length WHEN IT REPORTED, so the decision to report is
    /// testable without standing up a tracing subscriber to watch for the probe.
    fn note_absence(&self, room: Uuid) -> Option<u32> {
        if !self.cold_start_critical {
            return None;
        }
        let streak = {
            // diagnostics-only bookkeeping; a poisoned lock must not take
            // cognition down to protect a counter. No await is held here.
            let mut g = self.empty_streak.lock().unwrap_or_else(|e| e.into_inner());
            let c = g.entry_or(room, 0);
            *c += 1;
            *c
        };
        if streak.is_power_of_two() {
            continuum_base::probe!(
                class = "rag.coldstart.absent",
                source = self.source.source_id(),
                persona_id = %self.persona_id,
                room = %room,
                consecutive_empty = streak,
                "ColdStartCritical grounding delivered NOTHING and did not bid —                  this tier's absence is a WRONG turn, not an unenriched one. This                  faculty is generic over every RagSource and cannot say WHY: only                  the source knows, and only if it reports at all. Where one does                  (room-doctrine emits rag.doctrine.outcome), that row reflects its                  LAST EVALUATION, which need not be this tick — a cached delivery                  never reaches the source."
            );
            return Some(streak);
        }
        None
    }

    /// How long THIS room's current absence streak is. Test-visible so the live
    /// `contribute` path can be asserted without reaching into the map.
    #[cfg(test)]
    fn streak_in(&self, room: Uuid) -> u32 {
        self.empty_streak
            .lock()
            .unwrap_or_else(|e| e.into_inner()) // diagnostics-only, same as note_absence
            .get(&room)
            .copied()
            .unwrap_or(0)
    }

    /// Record that this source delivered content, closing any absence streak.
    ///
    /// The restore row is not decoration: an absence with no end in the ledger is
    /// indistinguishable from an absence still running when the census was taken,
    /// and the streak length is the only place the DURATION is written down.
    ///
    /// Returns the closed streak's length when it reported one (same reason as
    /// [`Self::note_absence`]).
    fn note_presence(&self, room: Uuid) -> Option<u32> {
        if !self.cold_start_critical {
            return None;
        }
        let streak = {
            // same contract as note_absence: recover the guard, never panic here.
            let mut g = self.empty_streak.lock().unwrap_or_else(|e| e.into_inner());
            // `None` here is BOTH "never absent" and "evicted", and that is
            // deliberate: announcing a recovery for a record that was merely
            // dropped would be the same lie this whole change is about.
            g.take(&room).unwrap_or(0)
        };
        if streak > 0 {
            continuum_base::probe!(
                class = "rag.coldstart.restored",
                source = self.source.source_id(),
                persona_id = %self.persona_id,
                room = %room,
                was_empty_for = streak,
                "ColdStartCritical grounding is bidding again"
            );
            return Some(streak);
        }
        None
    }
}

#[async_trait]
impl Faculty for RagSourceFaculty {
    fn id(&self) -> FacultyId {
        self.faculty_id.clone()
    }

    // Perception tier (default): grounding bids in phase 1 over the raw
    // world-state, so the deliberation faculty reads it from the broadcast in
    // phase 2. The grounding sources (roster, doctrine) read airc/substrate
    // state, not the burst, so the bridge does not pass the world_state as a
    // query; a future query-conditioned source would extend RagContext, not this
    // seam.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        let now = (self.clock)();
        // Sources declare what they describe: task board versus wake conversation.
        // Select before delivery so existing source caches see the same selected room.
        let room = match self.source.room_scope() {
            crate::cognition::rag_budget::RagRoomScope::Conversation => ws.room_id,
            crate::cognition::rag_budget::RagRoomScope::Activity => ws.activity_room_id,
        };
        let ctx = RagContext::for_persona_in_room(self.persona_id, now, room);
        // Every item the source holds, at its levels: her view's one allocation decides
        // what of it she sees (FLOW-IMPLEMENTATION-PLAN §2d). Each faculty was handed a
        // quarter of the window to cut to, seven of them about 7/4 of it.
        let delivery = self.source.deliver(&ctx).await;

        // Empty delivery → abstain (the source had nothing, or degraded to empty
        // per the good-citizen doctrine). No empty bid clutters the workspace. An
        // empty PARTIAL delivery is different: nothing fit, but the source has more,
        // so it still contributes its notice (card e049aa82) instead of vanishing.
        if delivery.items.is_empty() && !delivery.truncated {
            let _ = self.note_absence(room);
            return None;
        }
        let _ = self.note_presence(room);

        // One context block per source: concatenate the delivered atomic units.
        // The deliberation faculty renders this under a `[<source_id>]` header.
        //
        // The join is the RENDERING, not the loss of structure — the units ride
        // along on the contribution as `parts` (below), so a block too large for
        // the prompt budget can still contribute its leading units instead of
        // vanishing whole. Flattening and DISCARDING the list is what made the
        // work board structurally invisible: measured 2026-08-06, `room-kanban`
        // was kept 0 / dropped 495 times, a median 5,364-token all-or-nothing
        // offer against a median 55-token budget, while its first two units
        // (~200 tokens) carried every fact a citizen needed to find work.
        let units: Vec<String> = delivery.items.iter().map(|i| i.content.clone()).collect();
        let content = units.join("\n");
        let reasoning = format!(
            "grounding from '{}' — {} item(s), {} tokens",
            self.source.source_id(),
            delivery.items.len(),
            delivery.tokens_used
        );

        let c = Contribution::context(self.faculty_id.clone(), content, self.salience, reasoning)
            .with_parts(units)
            .with_units(delivery.items)
            .with_expand_command(self.source.expand_command())
            // A continuation means the SOURCE stopped short of its own list; the
            // packer's notice must say so (card e049aa82: nothing resumes cursors).
            .with_source_truncated(delivery.truncated);
        let c = if self.standing_grounding {
            c.standing_grounding()
        } else {
            c
        };
        // EVERY grounding bid is a block of her view (`.trailing()`), never the system
        // message. Volatile content moved first: rendered before the conversation, a
        // kanban claim-flap or a workspace-map change (her own write!) re-prefilled
        // every token after it (2026-08-23, live KV reuse 18-33%). Standing content
        // followed on 2026-10-23: grounding is the ROOM's, the system message is hers in
        // every room, and doctrine/mission/media in it rewrote its end on each room
        // switch. In her view a block appends when it changes and the turnover sizes it.
        // `stable` stays a fact about the bytes (how rarely they change), which the
        // deferred reprojection reads; it no longer decides placement.
        let c = if self.stable { c.session_stable() } else { c };
        Some(c.trailing())
    }
}

#[cfg(test)]
mod tests;
