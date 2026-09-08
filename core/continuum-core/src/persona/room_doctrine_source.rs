//! RoomDoctrineSource — reads the airc room operating doctrine and
//! packages it as a `[Room operating doctrine]` grounding block.
//!
//! ### Why this source exists (slice 2)
//!
//! Slice 1 ([`RoomRosterSource`](super::room_roster_source)) grounds a
//! persona in WHO else is present. This grounds it in WHAT KIND of room
//! it is — the activity's operating contract. Rooms are the universal
//! activity primitive (chat, dev-coordination, game, academy, help,
//! settings); a coordination room is not a free-for-all chat. Ivar's
//! *other* failure was over-participating in a coordination room because
//! it had no signal about the room's nature.
//!
//! airc already owns this: `Airc::room_doctrine` returns the latest
//! `RoomDoctrinePublished` for the current room — markdown the airc-core
//! docs explicitly say agents should "render verbatim / inject as a
//! system message into agent context." This source is that injection,
//! routed through the same RAG-grounding + capture/replay path as the
//! roster. Thin continuum: we read airc's doctrine, we don't invent a
//! room-nature concept. See
//! [[docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md]] §5 slice 2.
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: a failed/absent
//!   doctrine read returns an empty delivery — cognition stays up; a
//!   room with no published doctrine simply renders no block.
//! - Persona-scoped at construction (defense in depth, same as the
//!   roster + engram sources).
//! - Atomic unit = the doctrine body; no pagination (one current
//!   contract per room).

use std::sync::Arc;

use airc_core::doctrine::RoomDoctrinePublished;
use airc_core::PeerId;
use airc_lib::AircError;
use async_trait::async_trait;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — the service-loop projection routes this delivery
/// into system-prompt grounding (a `[Room operating doctrine]` block).
const SOURCE_ID: &str = "room-doctrine";

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;

/// Abstract reader over the airc room operating doctrine. Production
/// rides on `airc_lib::Airc::room_doctrine`; tests stub it without a
/// daemon. Mirrors the `AircRosterReader` rail.
#[async_trait]
pub trait AircDoctrineReader: Send + Sync {
    /// The latest published operating doctrine for a NAMED room, or `None` if
    /// none has been published there.
    ///
    /// ROOM-PARAMETRIC (#443, measured live 2026-08-17). This took no `room` and
    /// read whatever the scope's default subscription happened to be, so the
    /// source could only be BOUND at bootstrap and had to abstain on any other
    /// turn room — measured 39 abstains in 90 minutes with bound=academy,
    /// turn=bench-room. A citizen answering a turn in a per-run bench room got
    /// NO operating doctrine: she was handed the room's work with none of the
    /// room's rules.
    ///
    /// `airc_lib::room_doctrine_in` already existed for exactly this, and its
    /// own doc names the defect verbatim — *"A citizen who belongs to several
    /// rooms answers a turn in the room it arrived in; reading doctrine from her
    /// default instead grounds that answer in another room's rules."* The
    /// capability was built and never wired. This is the wire.
    ///
    /// `None` keeps the pre-#443 behaviour exactly (the scope's current room),
    /// which is what an UNSTAMPED context (background consolidation) still wants.
    async fn room_doctrine(
        &self,
        room: Option<uuid::Uuid>,
    ) -> Result<Option<RoomDoctrinePublished>, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule
/// OK — the trait is ours.
#[async_trait]
impl AircDoctrineReader for airc_lib::Airc {
    async fn room_doctrine(
        &self,
        room: Option<uuid::Uuid>,
    ) -> Result<Option<RoomDoctrinePublished>, AircError> {
        airc_lib::Airc::room_doctrine_in(self, room.map(airc_core::RoomId::from_uuid)).await
    }
}

/// WHY a delivery turned out the way it did — the thing an empty `RagDelivery`
/// cannot say for itself (#3873).
///
/// A doctrine delivery is empty for five materially different reasons, and until
/// this enum existed all five rendered as the same nothing: a room with no
/// published doctrine was indistinguishable, at every seam we had, from a reader
/// whose cache never wired up. #3873 could not be triaged past that ambiguity —
/// "no doctrine has ever been published here" and "the source is broken" were the
/// two live hypotheses and no instrument separated them.
/// [[absence-rendered-as-positive-fact]]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Nothing has been observed yet — the pre-first-call sentinel. Distinct from
    /// every real outcome so the FIRST call always reports one.
    Unobserved,
    /// A doctrine block was delivered.
    Delivered,
    /// The context belongs to another persona (defense in depth; should not occur
    /// in production, and its appearance in the ledger is itself the finding).
    CrossPersona,
    /// A synthetic nil-room context — deliberately gets nothing, and deliberately
    /// does NOT fall back to the bound room (the exam-bleed pin).
    NilRoom,
    /// The reader answered successfully and returned nothing **within the window
    /// it reads**. This is NOT "the room has no doctrine", and naming it that was
    /// the first version of this enum -- caught by @Astra on review of #3903.
    ///
    /// THE BOUND, verified in `airc.rs::room_doctrine_in` rather than assumed:
    /// it calls `page_recent_filtered(filter, 200)` and `continue`s past events
    /// that fail to decode. So `Ok(None)` means *no decodable
    /// `DoctrinePublished` event in the latest 200 events of this room*. In a
    /// busy room that is minutes. A doctrine published an hour ago is invisible
    /// to it, and an undecodable one is silently skipped.
    ///
    /// Calling that `NoDoctrinePublished` renders COULD_NOT_LOOK as
    /// MEASURED_ABSENT -- the exact defect this enum exists to prevent,
    /// committed inside the enum. Establishing real absence needs a complete
    /// indexed projection, which this reader is not.
    /// [[absence-rendered-as-positive-fact]]
    NotObservedInReadWindow,
    /// The reader failed. Cognition stays up, but this is NOT the normal case and
    /// must never again look like one.
    ReadFailed,
    /// A doctrine exists but the budget could not carry the marker plus real
    /// content.
    BudgetTooSmall,
}

impl Outcome {
    fn code(self) -> u8 {
        self as u8
    }
    fn from_code(c: u8) -> Self {
        match c {
            1 => Self::Delivered,
            2 => Self::CrossPersona,
            3 => Self::NilRoom,
            4 => Self::NotObservedInReadWindow,
            5 => Self::ReadFailed,
            6 => Self::BudgetTooSmall,
            // 0 and anything unrecognised: nothing observed. `from_code` only ever
            // reads back what `code` wrote, so the fallthrough is unreachable in
            // practice -- it is written this way because a panic here would take
            // down cognition to report a logging detail.
            _ => Self::Unobserved,
        }
    }
}

/// RoomDoctrineSource — persona-bound, reads the room doctrine from any
/// `AircDoctrineReader`.
pub struct RoomDoctrineSource {
    persona_id: uuid::Uuid,
    /// The room whose doctrine this source grounds — see the room gate in
    /// `deliver` and [`for_room`](Self::for_room). `None` = unscoped
    /// (legacy/test construction): pre-gate behavior.
    room_id: Option<uuid::Uuid>,
    reader: Arc<dyn AircDoctrineReader>,
    /// The last [`Outcome`] observed PER ROOM, with the wall-clock ms at which it
    /// was observed. Transitions are what reach the probe ledger -- see
    /// [`Self::report`].
    ///
    /// KEYED BY ROOM because this source is TURN-PARAMETRIC (#443): it answers
    /// for whatever room the turn is in. A single cell was the first version and
    /// it was wrong three ways, all caught by @Astra on review of #3903 — the
    /// same outcome in room B was suppressed by room A's, alternating rooms
    /// emitted a transition every tick with no state having changed, and a
    /// recovery in B would have been reported as room A's absence ending. The
    /// key is `Option<Uuid>` because an UNSTAMPED context (background
    /// consolidation) is its own scope, not a room.
    ///
    /// Bounded by the persona's subscribed rooms, which is a handful; a room she
    /// never takes a turn in never gets an entry.
    observed: std::sync::Mutex<std::collections::HashMap<Option<uuid::Uuid>, (Outcome, u64)>>,
}

impl RoomDoctrineSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircDoctrineReader>) -> Self {
        Self {
            persona_id,
            room_id: None,
            reader,
            observed: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Emit a probe when, and only when, THIS ROOM's outcome changes.
    ///
    /// Per-tick reporting would be 873 rows to say one thing, and the fact worth
    /// having is not "empty now" but "empty for a DIFFERENT reason than before".
    /// A transition ledger is bounded by the number of real state changes -- for a
    /// room whose doctrine the reader has never seen in its window, exactly ONE
    /// row, written on the first tick, saying so in those words.
    ///
    /// ## What this does NOT join with (corrected, #3903 review)
    ///
    /// An earlier version of this doc claimed the source outcome and the
    /// [`RagSourceFaculty`] absence streak "join on source + tick". They do not,
    /// and I had not checked it. Two reasons, both verified:
    ///
    /// 1. There is no shared tick or cycle id on either probe.
    /// 2. `CachedRagSource::deliver` returns `cached.delivery.clone()` WITHOUT
    ///    calling the inner source (`cached_source.rs`, the `answers` fast path),
    ///    so on a cache hit this function never runs at all.
    ///
    /// So the two probes count different populations: the faculty's streak counts
    /// TICKS WITHOUT A BID; this outcome is the last ACTUAL READ, which on a
    /// cached path may be several ticks old. The probe therefore carries
    /// `observed_age_ms` — the reader can see how stale the observation is instead
    /// of assuming it is this tick's.
    ///
    /// Returns `true` when it emitted, so the transition rule is testable without
    /// standing up a tracing subscriber to watch for the probe.
    ///
    /// [`RagSourceFaculty`]: crate::cognition::rag_source_faculty::RagSourceFaculty
    /// `now_ms` is THE TURN'S clock (`ctx.now_ms`), never `SystemTime::now()` —
    /// `SubstrateContext` documents that so observations stamp consistently and
    /// replay stays deterministic.
    fn report(&self, outcome: Outcome, room: Option<uuid::Uuid>, now_ms: u64) -> bool {
        let now = now_ms;
        // lock poisoning here would mean a panic inside this tiny map update; the
        // observation ledger is diagnostics, and taking cognition down to protect
        // its bookkeeping is the wrong trade.
        let mut guard = self.observed.lock().unwrap_or_else(|e| e.into_inner());
        let prev = guard.insert(room, (outcome, now));
        let (prev_outcome, prev_at) = prev.unwrap_or((Outcome::Unobserved, now));
        if prev_outcome == outcome {
            return false;
        }
        drop(guard);
        crate::probe!(
            class = "rag.doctrine.outcome",
            source = SOURCE_ID,
            persona_id = %self.persona_id,
            bound_room = ?self.room_id,
            effective_room = ?room,
            from = ?prev_outcome,
            to = ?outcome,
            observed_age_ms = now.saturating_sub(prev_at),
            "room-doctrine outcome changed for THIS room — an empty delivery now              says WHICH empty it is (unobserved-in-window vs read-failed vs gated              out), and how old the previous observation was"
        );
        true
    }

    /// The last outcome observed FOR ONE ROOM. Test-visible so a regression can
    /// assert WHICH absence occurred and that room A's state never answers for
    /// room B — the whole point of both the enum and the key.
    #[cfg(test)]
    fn observed_in(&self, room: Option<uuid::Uuid>) -> Outcome {
        self.observed
            .lock()
            .unwrap_or_else(|e| e.into_inner()) // diagnostics-only ledger, same as report()
            .get(&room)
            .map(|(o, _)| *o)
            .unwrap_or(Outcome::Unobserved)
    }

    /// Bind this source to the room its reader answers for, so a context-stamped
    /// turn in ANY other context (another room, the eval fork's nil room) gets an
    /// empty delivery instead of this room's doctrine (the exam-bleed fix).
    /// [[identity-context-session-three-axes]]
    pub fn for_room(mut self, room_id: uuid::Uuid) -> Self {
        self.room_id = Some(room_id);
        self
    }

    /// Fit the doctrine body to `budget` tokens. A doctrine is a single
    /// contract; if it doesn't fit we deliver a truncated prefix (with a
    /// marker) rather than dropping it entirely — partial guidance still
    /// grounds the persona in the room's nature.
    ///
    /// Returns `None` (no block) when the budget is too small to carry
    /// the marker PLUS some real content. The alternative — emitting a
    /// `[Room operating doctrine]` block whose entire content is the
    /// truncation marker — is strictly worse than no block: it spends
    /// tokens to say nothing, and (with a naive char budget) overspends
    /// the allocation. Both the no-content case and overspend are
    /// guarded here. `estimate_tokens(s) = s.chars()/4 + 1`, so a string
    /// of `4*budget - 4` chars estimates to exactly `budget` tokens —
    /// that's the ceiling we fit the (truncated body + marker) under.
    fn fit_body(body: &str, budget: u32) -> Option<String> {
        if budget == 0 {
            return None;
        }
        if estimate_tokens(body) <= budget {
            return Some(body.to_string());
        }
        const MARKER: &str = "\n…[doctrine truncated]";
        let marker_chars = MARKER.chars().count();
        // Max chars whose estimate stays within `budget` tokens.
        let max_chars = (budget as usize).saturating_mul(4).saturating_sub(4);
        if max_chars <= marker_chars {
            // No room for the marker plus any content → emit no block.
            return None;
        }
        let char_budget = max_chars - marker_chars;
        let mut out: String = body.chars().take(char_budget).collect();
        out.push_str(MARKER);
        Some(out)
    }
}

#[async_trait]
impl RagSource for RoomDoctrineSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        // the room's doctrine is a single standing statement, delivered whole.
        None
    }

    /// Floorless by design (unchanged): doctrine is lightweight standing
    /// framing that takes leftover room rather than reserving any.
    fn floor_tokens(&self) -> u32 {
        0
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        let empty = |res| RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: Vec::new(),
            tokens_used: 0,
            continuation: None,
            resolution_used: res,
        };

        // Persona-scoped (defense in depth, same shape as the roster).
        if ctx.persona_id != self.persona_id {
            let _ = self.report(Outcome::CrossPersona, None, ctx.now_ms);
            return empty(ResolutionPreference::Placeholder);
        }
        // Room resolution — TURN-PARAMETRIC (#443), the same shape RoomBoardSource
        // already uses and for the same reason: the rules she needs are the rules
        // OF THE ROOM SHE IS STANDING IN. The stamped turn room wins; the bound
        // room is only the fallback for UNSTAMPED contexts (background
        // consolidation, legacy construction — pre-gate behaviour, unchanged).
        // A synthetic nil room still gets NOTHING and does NOT fall back (the
        // exam-bleed pin).
        let effective_room = match ctx.airc_room.as_ref().map(|r| r.as_uuid()) {
            Some(t) if t.is_nil() => {
                crate::probe!(
                    class = "rag.room_gate.abstain",
                    source = SOURCE_ID,
                    bound_room = ?self.room_id,
                    turn_room = %t,
                    persona_id = %ctx.persona_id,
                    "synthetic nil-room context — no doctrine, and no fallback to the bound room"
                );
                let _ = self.report(Outcome::NilRoom, Some(t), ctx.now_ms);
                return empty(ResolutionPreference::Placeholder);
            }
            Some(t) => Some(t),
            None => self.room_id,
        };

        let card = match self.reader.room_doctrine(effective_room).await {
            Ok(Some(card)) => card,
            // Nothing in the reader's WINDOW → no block. Not "no doctrine
            // exists": the reader pages the latest 200 events and skips decode
            // failures, so this is an unobserved, not an absence. #3873 spent
            // its life as "the gate has never held" because a working source
            // reading an empty window looked exactly like a broken one — and
            // this variant must not re-tell that story in the other direction.
            Ok(None) => {
                let _ = self.report(Outcome::NotObservedInReadWindow, effective_room, ctx.now_ms);
                return empty(resolution);
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "room_doctrine: read failed — empty delivery, cognition stays up"
                );
                let _ = self.report(Outcome::ReadFailed, effective_room, ctx.now_ms);
                return empty(ResolutionPreference::Placeholder);
            }
        };

        let Some(content) = Self::fit_body(&card.body, budget) else {
            let _ = self.report(Outcome::BudgetTooSmall, effective_room, ctx.now_ms);
            return empty(resolution);
        };
        let tokens = estimate_tokens(&content);

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            version = %card.version,
            tokens,
            "room_doctrine: deliver"
        );
        let _ = self.report(Outcome::Delivered, effective_room, ctx.now_ms);

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items: vec![RagItem {
                content,
                tokens,
                metadata: serde_json::json!({
                    "version": card.version,
                    "published_by": card.published_by.as_uuid().to_string(),
                    "published_at_ms": card.published_at_ms,
                }),
            }],
            tokens_used: tokens,
            // One current contract per room; no pagination.
            continuation: None,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        _cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        // Doctrine is a single current snapshot; any cursor is stale.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn card(body: &str) -> RoomDoctrinePublished {
        RoomDoctrinePublished {
            room_id: airc_core::RoomId::new(),
            body: body.to_string(),
            version: "v1abc".to_string(),
            published_by: PeerId::new(),
            published_at_ms: 1_000_000,
        }
    }

    struct StubReader {
        doctrine: Option<RoomDoctrinePublished>,
        fail: Mutex<bool>,
        /// The room the source last ASKED for — #443's regression pins it.
        asked_room: Mutex<Option<Option<uuid::Uuid>>>,
    }

    impl StubReader {
        fn new(doctrine: Option<RoomDoctrinePublished>) -> Self {
            Self {
                doctrine,
                fail: Mutex::new(false),
                asked_room: Mutex::new(None),
            }
        }
        fn asked_room(&self) -> Option<Option<uuid::Uuid>> {
            *self.asked_room.lock().unwrap()
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl AircDoctrineReader for StubReader {
        async fn room_doctrine(
            &self,
            room: Option<uuid::Uuid>,
        ) -> Result<Option<RoomDoctrinePublished>, AircError> {
            *self.asked_room.lock().unwrap() = Some(room);
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.doctrine.clone())
        }
    }

    mod which_empty_is_it {
        use super::*;

        fn source(reader: Arc<StubReader>, room: uuid::Uuid) -> RoomDoctrineSource {
            RoomDoctrineSource::new(persona(), reader).for_room(room)
        }

        fn ctx_in(room: uuid::Uuid) -> RagContext {
            RagContext::for_persona_in_room(persona(), 1_000_000, room)
        }

        // what this catches: regression for #3873. `room-doctrine` delivered
        // nothing on 873 consecutive ticks and the two live hypotheses — "no
        // doctrine was ever published here" (source fine) and "the cache never
        // wired up" (source broken) — were INDISTINGUISHABLE at every seam we
        // had, because both rendered as the same empty delivery. They are now
        // different rows. [[absence-rendered-as-positive-fact]]
        #[tokio::test]
        async fn an_unpublished_room_and_a_broken_reader_are_different_absences() {
            let room = uuid::Uuid::new_v4();
            let reader = Arc::new(StubReader::new(None));
            let src = source(reader.clone(), room);

            let d = src.deliver(&ctx_in(room), 4_000, ResolutionPreference::Raw).await;
            assert!(d.items.is_empty(), "no doctrine published — no block");
            assert_eq!(
                src.observed_in(Some(room)),
                Outcome::NotObservedInReadWindow,
                "an empty WINDOW must say it is a window, not an absence"
            );

            reader.set_fail(true);
            let d = src.deliver(&ctx_in(room), 4_000, ResolutionPreference::Raw).await;
            assert!(d.items.is_empty(), "a failed read still keeps cognition up");
            assert_eq!(
                src.observed_in(Some(room)),
                Outcome::ReadFailed,
                "and a BROKEN reader must not look like an unobserved window"
            );

            assert_ne!(
                Outcome::NotObservedInReadWindow,
                Outcome::ReadFailed,
                "non-degeneracy: the two states this test distinguishes must not                  be the same value, or it would pass without discriminating"
            );
        }

        // what this catches: the transition rule. Reporting every tick would be
        // 873 rows to say one thing, and the row that matters would be evicted
        // from whatever window someone is reading
        // ([[bounded-window-eviction-bug-class]]). One row per real state change:
        // the first tick reports, an unchanged second tick does not, and a CHANGE
        // reports again.
        #[tokio::test]
        async fn only_a_change_of_outcome_reaches_the_ledger() {
            let room = uuid::Uuid::new_v4();
            let src = source(Arc::new(StubReader::new(None)), room);

            assert!(
                src.report(Outcome::NotObservedInReadWindow, Some(room), 1_000),
                "the FIRST observation is always a transition (from Unobserved)"
            );
            assert!(
                !src.report(Outcome::NotObservedInReadWindow, Some(room), 1_000),
                "a repeat of the same state must not cost a row"
            );
            assert!(
                src.report(Outcome::ReadFailed, Some(room), 1_200),
                "a real change must always be written down"
            );
            assert!(
                src.report(Outcome::Delivered, Some(room), 1_300),
                "including recovery — the end of an absence is a fact too"
            );
        }

        // what this catches: regression for @Astra's #3903 review finding 2 — the
        // first version kept ONE outcome cell for a source that is
        // TURN-PARAMETRIC across rooms (#443). Room A's state then answered for
        // room B three ways: B's first observation was suppressed when it matched
        // A's, alternating rooms emitted a transition every tick with no state
        // having changed, and a recovery in B would have been reported as A's
        // absence ending.
        //
        // NON-DEGENERACY, stated because a fixture with fewer degrees of freedom
        // than the bug cannot fail: this needs TWO rooms with DIFFERENT outcomes,
        // and it asserts the rooms differ and the outcomes differ before relying
        // on them. [[fixture-with-fewer-degrees-of-freedom-than-the-bug]]
        #[test]
        fn one_room_s_outcome_never_answers_for_another() {
            let a = uuid::Uuid::new_v4();
            let b = uuid::Uuid::new_v4();
            assert_ne!(a, b, "non-degeneracy: two DIFFERENT rooms");
            let src = source(Arc::new(StubReader::new(None)), a);

            assert!(src.report(Outcome::NotObservedInReadWindow, Some(a), 1_000));
            // Room B has never been observed, so its FIRST observation is a
            // transition even though it carries the same outcome as A's.
            assert!(
                src.report(Outcome::NotObservedInReadWindow, Some(b), 1_010),
                "B's first observation must not be suppressed by A's state"
            );
            // Neither room has changed, so going back to A reports nothing.
            assert!(
                !src.report(Outcome::NotObservedInReadWindow, Some(a), 1_020),
                "alternating rooms must not manufacture transitions"
            );
            // A real change in B, and A is untouched by it.
            assert!(src.report(Outcome::Delivered, Some(b), 1_030));
            assert_eq!(src.observed_in(Some(a)), Outcome::NotObservedInReadWindow);
            assert_eq!(src.observed_in(Some(b)), Outcome::Delivered);
            assert_ne!(
                src.observed_in(Some(a)),
                src.observed_in(Some(b)),
                "non-degeneracy: the two rooms must hold DIFFERENT outcomes, or                  this test would pass with the key removed"
            );
            // An UNSTAMPED context (background consolidation) is its own scope,
            // not either room.
            assert_eq!(src.observed_in(None), Outcome::Unobserved);
        }

        // what this catches: the code<->enum round-trip the stored representation
        // relies
        // on. A silent collision (two outcomes sharing a code) would merge two
        // distinct absences back into one, undoing this whole change while every
        // other test still passed.
        #[test]
        fn every_outcome_survives_the_atomic_round_trip_distinctly() {
            let all = [
                Outcome::Unobserved,
                Outcome::Delivered,
                Outcome::CrossPersona,
                Outcome::NilRoom,
                Outcome::NotObservedInReadWindow,
                Outcome::ReadFailed,
                Outcome::BudgetTooSmall,
            ];
            for o in all {
                assert_eq!(Outcome::from_code(o.code()), o, "round-trip for {o:?}");
            }
            let mut codes: Vec<u8> = all.iter().map(|o| o.code()).collect();
            codes.sort_unstable();
            let before = codes.len();
            codes.dedup();
            assert_eq!(before, codes.len(), "two outcomes must never share a code");
        }
    }

    // what this catches: #443 — a citizen taking a turn in a per-run BENCH room
    // was handed the ACADEMY's rules, or (before the room-parametric reader) none
    // at all. Measured live 2026-08-17: 39 `rag.room_gate.abstain` rows in 90
    // minutes with bound=academy, turn=bench-room, so every bench turn ran with no
    // operating doctrine. The reader is now asked for the room she is STANDING in.
    #[tokio::test]
    async fn the_turn_room_wins_over_the_bound_room() {
        let bound = uuid::Uuid::new_v4();
        let turn = uuid::Uuid::new_v4();
        let reader = Arc::new(StubReader::new(Some(card("be excellent"))));
        let src = RoomDoctrineSource::new(persona(), reader.clone()).for_room(bound);

        let ctx = RagContext::for_persona_in_room(persona(), 1_000_000, turn);
        let out = src.deliver(&ctx, 4096, ResolutionPreference::Raw).await;

        assert_eq!(
            reader.asked_room(),
            Some(Some(turn)),
            "the reader must be asked for the TURN room, not the bound room"
        );
        assert!(
            !out.items.is_empty(),
            "a stamped turn in another room must still receive doctrine — abstaining \
             here is exactly the #443 defect (she got the work, not the rules)"
        );
    }

    // what this catches: the exam-bleed pin must survive the #443 change — a
    // synthetic nil room gets NOTHING and must NOT silently fall back to the bound
    // room, or an eval fork would read the live room's doctrine.
    #[tokio::test]
    async fn a_nil_room_gets_nothing_and_never_falls_back() {
        let bound = uuid::Uuid::new_v4();
        let reader = Arc::new(StubReader::new(Some(card("be excellent"))));
        let src = RoomDoctrineSource::new(persona(), reader.clone()).for_room(bound);

        let ctx = RagContext::for_persona_in_room(persona(), 1_000_000, uuid::Uuid::nil());
        let out = src.deliver(&ctx, 4096, ResolutionPreference::Raw).await;

        assert!(out.items.is_empty(), "a nil-room context must receive no doctrine");
        assert_eq!(
            reader.asked_room(),
            None,
            "the reader must not be consulted at all for a nil room"
        );
    }

    // what this catches: a published doctrine surfaces as a delivery the
    // service loop can route into the [Room operating doctrine] grounding
    // block — the fix for a persona ignoring the room's nature.
    #[tokio::test]
    async fn published_doctrine_surfaces() {
        let reader = Arc::new(StubReader::new(Some(card(
            "This is a coordination room. Respond sparingly; do not chat.",
        ))));
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.items[0].content.contains("Respond sparingly"));
        assert_eq!(delivery.items[0].metadata["version"], "v1abc");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: a room with NO published doctrine renders no
    // block (backwards-compatible; most rooms have none yet).
    #[tokio::test]
    async fn no_doctrine_delivers_nothing() {
        let reader = Arc::new(StubReader::new(None));
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    // what this catches: a read failure degrades to empty, never panics —
    // cognition stays up if the doctrine subsystem is degraded.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let reader = Arc::new(StubReader::new(Some(card("body"))));
        reader.set_fail(true);
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(Some(card("body"))));
        let source = RoomDoctrineSource::new(persona(), reader);
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(
                &RagContext::for_persona(alien, 1_000_000),
                1_000,
                ResolutionPreference::Raw,
            )
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: an over-budget doctrine is truncated and NEVER
    // overspends the allocation — across the full budget regime, not just
    // a comfortable one. The earlier version only checked budget=20 and
    // missed that tiny budgets (1..=6) overspent with a content-free
    // marker-only block (adversarial review of PR #1651). Invariant now:
    // for ANY budget, either no block is delivered, or the delivered
    // block fits the budget AND carries real content (not just the
    // truncation marker).
    #[tokio::test]
    async fn oversized_doctrine_never_overspends_across_budget_regime() {
        let big = "x".repeat(10_000);
        for budget in [1u32, 2, 3, 5, 6, 7, 8, 20, 100, 500] {
            let reader = Arc::new(StubReader::new(Some(card(&big))));
            let source = RoomDoctrineSource::new(persona(), reader);
            let delivery = source
                .deliver(&ctx(), budget, ResolutionPreference::Raw)
                .await;
            assert!(
                delivery.tokens_used <= budget,
                "budget {budget}: overspent ({} > {budget})",
                delivery.tokens_used
            );
            if let Some(item) = delivery.items.first() {
                // A delivered block must carry real doctrine content, not
                // just the truncation marker — else it spends tokens to
                // say nothing.
                let only_marker =
                    item.content.trim_start().starts_with('…') || !item.content.contains('x');
                assert!(
                    !only_marker,
                    "budget {budget}: delivered a content-free block: {:?}",
                    item.content
                );
            }
        }
    }

    // what this catches: a budget too small to carry the marker plus real
    // content yields NO block rather than a token-wasting marker-only one.
    #[tokio::test]
    async fn tiny_budget_emits_no_block() {
        let big = "x".repeat(10_000);
        let reader = Arc::new(StubReader::new(Some(card(&big))));
        let source = RoomDoctrineSource::new(persona(), reader);
        let delivery = source.deliver(&ctx(), 3, ResolutionPreference::Raw).await;
        assert!(delivery.items.is_empty(), "tiny budget must emit no block");
        assert_eq!(delivery.tokens_used, 0);
    }
}
