//! RoomRosterSource — reads real airc room presence (`active_agents`)
//! and packages "who else is in this room right now" as RagItems for
//! the budget allocator.
//!
//! ### Why this source exists
//!
//! The persona cognition loop binds `engram + airc` sources and grounds
//! the turn in recalled memory + recent transcript. But it had **no
//! grounding in who the other participants are** — the airc transcript
//! delivers messages tagged with other citizens' names, while the
//! system prompt only says "you are <name>, never narrate others"
//! without ever naming who "others" are. Under that gap a small model
//! sees `BigMama:`, `Joel:`, `IntelMac:` in the history and role-plays
//! the whole room (the Ivar confabulation bug).
//!
//! This source closes the gap by reading the **truthful** presence list
//! from airc — not a static seed, not a guess — and delivering one item
//! per present citizen. The projection layer routes these into
//! **system-prompt grounding** (a `[Present in this room]` block), NOT
//! the conversation history (which would re-create the very "is this a
//! message?" confusion). See
//! [[docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md]] §5 slice 1.
//!
//! ### Architecture
//!
//! Mirrors [`AircRagSource`] exactly: abstracts an `AircRosterReader`
//! trait so unit tests don't need a live airc daemon. The real impl
//! rides on `airc_lib::Airc::active_agents` + `peer_alias` +
//! `peer_id` (self, to exclude the persona from its own roster). This
//! is the adapter-first rail: ship the trait + the real impl + a stub.
//!
//! ### Security grounding (per the airc-native doc §4)
//!
//! Each `AgentLiveness` carries a `runtime` label
//! (`"claude"`/`"codex"`/`"persona"`/`"interactive"`) — outsider agents
//! self-identify. The source carries that origin into item metadata so
//! downstream slices can mark grid-local citizens vs outsider agents and
//! let a persona weigh instructions by origin. Slice 1 only surfaces it;
//! enforcement is slice 3.
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: a failed presence read
//!   returns an empty delivery + `tracing::warn` — cognition stays up
//!   even when the airc subsystem is degraded.
//! - Persona-scoped at construction: a cross-persona ctx returns empty
//!   (defense in depth, same shape as `AircRagSource`).
//! - Roster is small + always-current; no pagination (atomic unit = one
//!   present citizen, like the `ToolSource` precedent in the trait doc).

use std::sync::Arc;
use std::time::Duration;

use airc_core::PeerId;
use airc_lib::{AircError, RoomMember};
use async_trait::async_trait;
use continuum_positron::RosterSlotView;

use crate::ipc::positron_source::roster_slot_from_member;
use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — used by budget presets, telemetry, and the
/// service-loop projection that routes this delivery into system-prompt
/// grounding rather than conversation history.
const SOURCE_ID: &str = "room-roster";

/// How far back a heartbeat counts as "present". Matches the airc
/// agent-liveness convention of a short recency window — a peer that
/// hasn't beaten within this window is treated as gone.
pub(crate) const PRESENCE_WINDOW: Duration = Duration::from_secs(120);

/// How many recent transcript events airc scans to build the roster.
/// The presence reduction keeps one entry per peer, so this only needs
/// to span the heartbeat cadence across all present peers — not the
/// full scrollback. Passed straight to `Airc::room_roster`.
pub(crate) const ROSTER_SCAN: usize = 200;

/// Token estimate — the ONE canonical chars/4 estimator (`cognition::token_budget`),
/// shared by every RAG source so the replay ledger's numbers match. (Was a private
/// copy — converged.)
use crate::cognition::token_budget::estimate_prompt_tokens as estimate_tokens;

/// Abstract reader over airc room membership. Production impl rides on
/// `airc_lib::Airc::room_roster`; tests use a stub that returns canned
/// `RoomMember`s without needing a daemon.
///
/// Slice "consume room_roster" (airc#1232): this used to be a 2-method
/// `active_agents` + `peer_alias_map` reader, with continuum re-parsing
/// `IdentityPublished` events to join names onto presence — a wire-format
/// coupling the adversarial review of continuum#1650 flagged. airc now
/// owns that join: `room_roster` returns presence + names in ONE batched
/// scan. So this is a single-method reader and the continuum-side
/// parsing is gone — thin continuum, airc owns presence+identity.
#[async_trait]
pub trait AircRosterReader: Send + Sync {
    /// This persona's own airc peer id — used to exclude self from the
    /// roster so the grounding block reads "who is NOT me". (`room_roster`
    /// includes self by design; the caller drops its own peer_id.)
    fn self_peer_id(&self) -> PeerId;

    /// Everyone present in this persona's room — presence joined with
    /// published display names in ONE airc-side batched scan, within
    /// `within`, over the most recent `window` transcript events.
    /// Newest-wins per peer; peers that signalled `Leaving` are excluded
    /// by airc; `display_name` is `None` for a present-but-unnamed peer.
    /// ROOM-PARAMETRIC (#443, measured live 2026-08-17). This took no `room`, so
    /// the source could only be BOUND at bootstrap and had to abstain on any
    /// other turn room — measured 42 abstains in 90 minutes with bound=academy,
    /// turn=bench-room. A citizen answering a turn in a per-run bench room saw
    /// NO ONE: no teammates, no peers to address, in the room she was actually
    /// standing in. `airc_lib::room_roster_in` already existed; it was never
    /// wired. `None` keeps the pre-#443 behaviour (the scope's current room).
    async fn room_roster(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<RoomMember>, AircError>;

    /// The CARDS-flavored roster (#262): presence + each peer's FULL
    /// published identity card (name/pronouns/role/bio/integrations).
    /// Default adapts the thin [`room_roster`](Self::room_roster) with
    /// `identity: None` — a reader that can't reach the identity store
    /// still yields an honest presence-only roster; `airc_lib::Airc`
    /// overrides with the real card join.
    async fn room_roster_cards(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
        Ok(self
            .room_roster(within, window, room)
            .await?
            .into_iter()
            .map(|m| airc_lib::RoomMemberCard {
                peer_id: m.peer_id,
                runtime: m.runtime,
                availability: m.availability,
                last_seen_ms: m.last_seen_ms,
                identity: None,
            })
            .collect())
    }
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule
/// OK — the trait is ours (defined in this crate).
#[async_trait]
impl AircRosterReader for airc_lib::Airc {
    fn self_peer_id(&self) -> PeerId {
        airc_lib::Airc::peer_id(self)
    }

    async fn room_roster(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<RoomMember>, AircError> {
        airc_lib::Airc::room_roster_in(self, room.map(airc_core::RoomId::from_uuid), within, window)
            .await
    }

    async fn room_roster_cards(
        &self,
        within: Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
        airc_lib::Airc::room_roster_cards_in(
            self,
            room.map(airc_core::RoomId::from_uuid),
            within,
            window,
        )
        .await
    }
}

/// RoomRosterSource — persona-bound, reads presence from any
/// `AircRosterReader`.
pub struct RoomRosterSource {
    persona_id: uuid::Uuid,
    /// The room whose presence this source grounds — see the room gate in
    /// `deliver` and [`for_room`](Self::for_room). `None` = unscoped
    /// (legacy/test construction): pre-gate behavior.
    room_id: Option<uuid::Uuid>,
    reader: Arc<dyn AircRosterReader>,
}

impl RoomRosterSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircRosterReader>) -> Self {
        Self {
            persona_id,
            room_id: None,
            reader,
        }
    }

    /// Bind this source to the room its reader answers for, so a context-stamped
    /// turn in ANY other context (another room, the eval fork's nil room) gets an
    /// empty delivery instead of this room's roster — a turn must never be told
    /// who is present in a room it isn't in (the exam-bleed fix).
    /// [[identity-context-session-three-axes]]
    pub fn for_room(mut self, room_id: uuid::Uuid) -> Self {
        self.room_id = Some(room_id);
        self
    }

    /// Format one present citizen as a roster grounding line from the neutral
    /// [`RosterSlotView`] the shared projection produced. The line is
    /// human-readable on its own; the structured parts also ride in metadata
    /// so prompt-assembly and sentinel verifiers can render / trace without
    /// re-parsing the string.
    ///
    /// Shape: `<name> [<runtime>]` plus ` — <availability>` when the peer
    /// reported one. Examples: `Aria [persona]`, `win-claude [claude] — busy`
    /// (availability is airc's neutral label, carried through the slot).
    fn format_line(name: &str, runtime: &str, availability: Option<&str>) -> String {
        match availability {
            Some(avail) => format!("{name} [{runtime}] — {avail}"),
            None => format!("{name} [{runtime}]"),
        }
    }

    /// One present citizen → a grounding [`RagItem`], built from the SAME
    /// neutral [`RosterSlotView`] the WS widget roster is built from (the
    /// convergence #8/#13). Display name, runtime origin, availability and
    /// recency are read off the slot — never re-derived here — so the persona
    /// grounding and the widget roster can never disagree about who is present
    /// or drop different fields.
    fn make_item(slot: &RosterSlotView) -> RagItem {
        let content = Self::format_line(
            &slot.display_name,
            &slot.provenance.runtime,
            slot.availability.as_deref(),
        );
        let tokens = estimate_tokens(&content);
        RagItem {
            content,
            tokens,
            // The service-loop projection reads metadata["display_name"] to
            // populate other_persona_names (single-party history-drop): keep
            // the bare name here, matching the transcript sender name.
            metadata: serde_json::json!({
                "peer_id": slot.member_id.to_string(),
                "display_name": slot.display_name,
                "runtime": slot.provenance.runtime,
                "availability": slot.availability,
                "last_seen_ms": slot.last_seen_ms,
            }),
        }
    }

    /// The room-authority fact for a room with NO human present — the
    /// flywheel's self-authorization ground (Joel 2026-08-02: personas do real
    /// work, then park on "Would you like me to proceed?" addressed to a room
    /// where no one will ever answer). This is a PERCEPTION fact, not an
    /// output gate ([[no-hardcoded-heuristics-to-steer-cognition]]): it tells
    /// the persona the true authority structure of the room and lets her own
    /// deliberation draw the conclusion. Emitted only when the roster carries
    /// no human; a room WITH a human present says nothing (the roster lines
    /// already name them, and deference to a present human is correct).
    fn no_human_authority_item() -> RagItem {
        let content = "[room] No human is present in this room. Questions addressed \
                       to the room will not be answered by an operator — the work \
                       board is the authority here. Choose work from the board and \
                       proceed; report results, not requests for permission."
            .to_string();
        let tokens = estimate_tokens(&content);
        RagItem {
            content,
            tokens,
            metadata: serde_json::json!({ "fact": "no_human_present" }),
        }
    }

    /// Is this roster runtime a HUMAN-facing interactive client? The SAME
    /// classification `ipc/positron_presence.rs` applies (`"interactive"` →
    /// Human, everything else → agent/persona kinds) — one rule, referenced in
    /// both places; if the wire grows more human runtimes, both cite this fn.
    fn is_human_runtime(runtime: &str) -> bool {
        runtime == "interactive"
    }
}

#[async_trait]
impl RagSource for RoomRosterSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn expand_command(&self) -> Option<&'static str> {
        // the roster IS the whole membership; there is no longer form to fetch.
        None
    }

    /// Floorless by design (unchanged): the roster is a handful of presence
    /// lines and must never reserve budget away from the heavyweights.
    fn floor_tokens(&self) -> u32 {
        0
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing (defense in
        // depth, same shape as AircRagSource).
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: SOURCE_ID.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }
        // Room resolution — TURN-PARAMETRIC (#443), the same shape RoomBoardSource
        // uses. The peers she needs to see are the peers OF THE ROOM SHE IS
        // STANDING IN; the stamped turn room wins, the bound room is only the
        // fallback for UNSTAMPED contexts. A synthetic nil room gets NOTHING and
        // does NOT fall back (the exam-bleed pin).
        let effective_room = match ctx.airc_room.as_ref().map(|r| r.as_uuid()) {
            Some(t) if t.is_nil() => {
                crate::probe!(
                    class = "rag.room_gate.abstain",
                    source = SOURCE_ID,
                    bound_room = ?self.room_id,
                    turn_room = %t,
                    persona_id = %ctx.persona_id,
                    "synthetic nil-room context — no roster, and no fallback to the bound room"
                );
                return RagDelivery {
                    source_id: SOURCE_ID.to_string(),
                    items: Vec::new(),
                    tokens_used: 0,
                    continuation: None,
                    resolution_used: ResolutionPreference::Placeholder,
                };
            }
            Some(t) => Some(t),
            None => self.room_id,
        };

        // ONE airc call returns presence joined with display names
        // (airc#1232). A failure is non-fatal — empty delivery, cognition
        // stays up (good-citizen doctrine).
        let members = match self
            .reader
            .room_roster(PRESENCE_WINDOW, ROSTER_SCAN, effective_room)
            .await
        {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "room_roster: room_roster failed — empty delivery, cognition stays up"
                );
                return RagDelivery {
                    source_id: SOURCE_ID.to_string(),
                    items: Vec::new(),
                    tokens_used: 0,
                    continuation: None,
                    resolution_used: ResolutionPreference::Placeholder,
                };
            }
        };

        let self_peer = self.reader.self_peer_id();

        // Authority structure (the flywheel's self-authorization ground): scan
        // the FULL roster (self included — a persona is never a human, so this
        // is equivalent, but scanning all members keeps the fact independent
        // of the self-exclusion policy below) for any human-facing client.
        let human_present = members.iter().any(|m| Self::is_human_runtime(&m.runtime));

        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        if !human_present {
            let fact = Self::no_human_authority_item();
            if fact.tokens <= budget {
                tokens_used += fact.tokens;
                items.push(fact);
            }
        }
        for member in members {
            // Exclude self — the roster grounds the persona in who is NOT
            // itself. (room_roster includes self by design.) Self-exclusion is
            // THIS source's own policy, applied before the shared projection —
            // never baked into it (the widget roster keeps self).
            if member.peer_id == self_peer {
                continue;
            }
            // The ONE `RoomMember` → neutral slot projection, the same the WS
            // widget roster uses. Name fallback, runtime origin, availability
            // label and recency all resolve there, once.
            let slot = roster_slot_from_member(&member);
            let item = Self::make_item(&slot);
            if tokens_used.saturating_add(item.tokens) > budget {
                // Budget exhausted. Roster is unordered presence; we stop
                // rather than paginate (atomic unit = one present citizen, no
                // continuation). A truncated roster is still truthful for the
                // citizens it names.
                break;
            }
            tokens_used += item.tokens;
            items.push(item);
        }

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            present = items.len(),
            tokens_used,
            "room_roster: deliver"
        );

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            // Presence is always-current and small; no pagination.
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
        // Roster does not paginate — it's a small, always-current
        // snapshot. Any cursor is stale by construction.
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

    /// Test double — canned room members, optional failure. (airc joins
    /// presence + names into `RoomMember` now; the stub returns them
    /// directly — no separate alias map to maintain.)
    struct StubReader {
        self_peer: PeerId,
        members: Vec<RoomMember>,
        fail: Mutex<bool>,
        /// The room the source last ASKED for. #443's regression pins this:
        /// asking for the wrong room is invisible when the stub ignores it.
        asked_room: Mutex<Option<Option<uuid::Uuid>>>,
    }

    impl StubReader {
        fn new(self_peer: PeerId, members: Vec<RoomMember>) -> Self {
            Self {
                self_peer,
                members,
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
    impl AircRosterReader for StubReader {
        fn self_peer_id(&self) -> PeerId {
            self.self_peer
        }
        async fn room_roster(
            &self,
            _within: Duration,
            _window: usize,
            room: Option<uuid::Uuid>,
        ) -> Result<Vec<RoomMember>, AircError> {
            *self.asked_room.lock().unwrap() = Some(room);
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.members.clone())
        }
    }

    /// Build a `RoomMember` — `name: Some` mirrors a peer that published
    /// an identity card; `None` mirrors present-but-unnamed.
    fn member(peer: PeerId, runtime: &str, name: Option<&str>) -> RoomMember {
        RoomMember {
            peer_id: peer,
            display_name: name.map(|s| s.to_string()),
            runtime: runtime.to_string(),
            availability: None,
            last_seen_ms: 1_000_000,
        }
    }

    // what this catches (the flywheel self-authorization ground, Joel
    // 2026-08-02 "personas do real work then park on 'Would you like me to
    // proceed?'"): an agent/persona-only roster delivers the no-human
    // authority FACT first — the true authority structure of the room — and a
    // roster WITH a human ("interactive") delivers NO such fact (deference to
    // a present human is correct, and the roster line already names them).
    #[tokio::test]
    async fn no_human_roster_carries_the_authority_fact_and_human_presence_silences_it() {
        let me = PeerId::new();
        let agent = PeerId::new();
        let human = PeerId::new();

        // Agents/personas only → the fact rides first.
        let reader = Arc::new(StubReader::new(
            me,
            vec![member(agent, "persona", Some("Anwen"))],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.items[0].metadata["fact"], "no_human_present");
        assert!(
            delivery.items[0]
                .content
                .contains("work board is the authority"),
            "the fact teaches the authority structure, not an instruction to be quiet"
        );

        // A human client present → no fact; only the roster lines.
        let reader = Arc::new(StubReader::new(
            me,
            vec![
                member(agent, "persona", Some("Anwen")),
                member(human, "interactive", Some("Operator")),
            ],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 2, "two peers, zero facts");
        assert!(
            delivery
                .items
                .iter()
                .all(|i| i.metadata.get("fact").is_none()),
            "a room with a human present carries no authority fact"
        );
    }

    // what this catches: the confabulation root cause — a present peer
    // must surface in the roster with a real alias + its origin runtime,
    // so the persona is grounded in who else is here.
    #[tokio::test]
    async fn present_peer_surfaces_with_alias_and_origin() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(
            me,
            vec![member(other, "claude", Some("win-claude"))],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        // Agent-only roster → the no-human authority fact rides FIRST, then
        // the peer line (the flywheel self-authorization ground).
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.items[0].metadata["fact"], "no_human_present");
        assert!(delivery.items[1].content.contains("win-claude"));
        assert!(delivery.items[1].content.contains("claude"));
        assert_eq!(delivery.items[1].metadata["runtime"], "claude");
        // The service-loop projection reads metadata["display_name"] to
        // populate other_persona_names (single-party history-drop). Lock
        // that contract: the bare name must be present and match the
        // transcript sender name, not the formatted line.
        assert_eq!(delivery.items[1].metadata["display_name"], "win-claude");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: THE convergence guarantee — the persona grounding now
    // carries availability + recency (the liveness the WS widget rail used to
    // DROP through `AircPresenceSlot`), because both rails build from the one
    // shared `roster_slot_from_member`. A regression that stopped threading
    // these through the shared projection would silently blind the persona to
    // who is busy/away and how recently they were seen. Availability is airc's
    // neutral snake_case label (`busy`), not Debug's `Busy`.
    #[tokio::test]
    async fn availability_and_recency_carry_through_shared_projection() {
        use airc_lib::AgentAvailabilityState;
        let me = PeerId::new();
        let other = PeerId::new();
        let busy = RoomMember {
            peer_id: other,
            display_name: Some("win-claude".to_string()),
            runtime: "claude".to_string(),
            availability: Some(AgentAvailabilityState::Busy),
            last_seen_ms: 1_700_000_000_000,
        };
        let reader = Arc::new(StubReader::new(me, vec![busy]));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.items[1].content, "win-claude [claude] — busy");
        assert_eq!(delivery.items[1].metadata["availability"], "busy");
        assert_eq!(
            delivery.items[1].metadata["last_seen_ms"],
            1_700_000_000_000_u64
        );
    }

    // what this catches: the persona must NOT appear in its own roster —
    // the block is "who is NOT me". A self-entry would re-introduce the
    // self/other confusion the source exists to remove.
    #[tokio::test]
    async fn self_peer_excluded_from_roster() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(
            me,
            vec![member(me, "persona", None), member(other, "persona", None)],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(
            delivery.items.len(),
            2,
            "the fact + only the other peer, not self"
        );
        assert_eq!(
            delivery.items[1].metadata["peer_id"],
            other.as_uuid().to_string()
        );
    }

    // what this catches: a peer with no alias is still surfaced, never
    // silently dropped — an unnamed citizen is worse than a labelled one for
    // grounding. After the convergence it uses the SAME provisional label the
    // WS widget projection uses (`peer-XXXXXXXX`, via the shared
    // roster_slot_from_member) — one fallback-label decision, not a second form.
    #[tokio::test]
    async fn peer_without_alias_uses_shared_provisional_label() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![member(other, "codex", None)]));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(
            delivery.items.len(),
            2,
            "the no-human fact + the unnamed peer"
        );
        let simple = other.as_uuid().simple().to_string();
        let expected_label = format!("peer-{}", &simple[..8]);
        assert_eq!(
            delivery.items[1].content,
            format!("{expected_label} [codex]"),
            "unnamed peer uses the shared provisional label + its runtime origin"
        );
    }

    // what this catches: an EMPTY room (nobody else present) still delivers the
    // no-human authority fact — being alone is the STRONGEST self-authorization
    // case (there is nobody to ask) — and nothing else, no panic.
    #[tokio::test]
    async fn empty_room_delivers_only_the_authority_fact() {
        let me = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![]));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1);
        assert_eq!(delivery.items[0].metadata["fact"], "no_human_present");
        assert_eq!(delivery.tokens_used, delivery.items[0].tokens);
    }

    // what this catches: airc degraded → empty delivery, cognition stays
    // up (good-citizen doctrine). A roster read failure must never take
    // down the turn.
    #[tokio::test]
    async fn reader_error_returns_empty_no_panic() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![member(other, "persona", None)]));
        reader.set_fail(true);
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    // what this catches: cross-persona ctx gets nothing (defense in
    // depth) — a persona must never read another's room grounding.
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![member(other, "persona", None)]));
        let source = RoomRosterSource::new(persona(), reader);
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

    // what this catches: budget truncates the roster instead of
    // over-spending — a present citizen's line is an atomic unit, so we
    // stop cleanly rather than emit a partial line.
    #[tokio::test]
    async fn budget_truncates_without_overspend() {
        let me = PeerId::new();
        let reader = Arc::new(StubReader::new(
            me,
            vec![
                member(PeerId::new(), "persona", None),
                member(PeerId::new(), "persona", None),
                member(PeerId::new(), "persona", None),
            ],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        // Each line ~ a handful of tokens; a tiny budget fits at most one.
        let delivery = source.deliver(&ctx(), 4, ResolutionPreference::Raw).await;
        assert!(delivery.items.len() < 3, "budget must truncate the roster");
        assert!(delivery.tokens_used <= 4, "never overspend the budget");
    }
}
