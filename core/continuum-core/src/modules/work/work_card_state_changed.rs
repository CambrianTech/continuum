use super::*;

/// The bus event published the moment a card transitions — by [`bridge_wire_work_event`]
/// from the transition's own wire echo, so EVERY writer fires it: a citizen's
/// `work/state`, the operator's `airc work state`, a remote peer. Subscribers
/// glob-match this to REACT (e.g. the SWE grade-on-done handler, board freshness,
/// auto-close) — the system is event-based, never polling
/// ([[the-whole-system-is-event-based-not-polling]]). Payload: `{card_id, state}`.
pub const WORK_CARD_STATE_CHANGED: &str = "work.card.state_changed";

/// Process-global handle to the bus + registry so [`bridge_wire_work_event`] (called
/// from persona inbound streams, which hold no `ModuleContext`) can publish the
/// transition event. Set once by `WorkModule::initialize`; the bus and registry ARE
/// process-global (one core), same granularity as the gossip ledger and the admission
/// gates — so a process-global is the honest shape, not a field threaded through every
/// command constructor.
pub(super) static WORK_EVENT_BUS: OnceLock<(Arc<MessageBus>, Arc<ModuleRegistry>)> = OnceLock::new();

/// Decode a wire transcript event into the `work.card.state_changed` payload — or
/// `None` if it isn't a card-state transition. Pure over the event so the contract
/// is unit-testable against the real producer (`airc_work::encode_work_event`).
///
/// Payload contract: `{card_id, state}` with `card_id` the FULL hyphenated UUID and
/// `state` the snake_case `CardState` serde form ("closed"/"merged"/…) — the exact
/// vocabulary `benchmark_grade::is_terminal` and `parse_state` already speak.
pub(super) fn wire_card_state_payload(event: &airc_core::TranscriptEvent) -> Option<Value> {
    if !airc_work::transcript_is_work_event(event) {
        return None;
    }
    let item = airc_work::decode_transcript_work_event(event).ok()?;
    let airc_work::WorkEvent::CardStateChanged(changed) = item.event else {
        return None;
    };
    let state = serde_json::to_value(changed.state).ok()?;
    Some(serde_json::json!({
        "card_id": changed.card_id.as_uuid().to_string(),
        "state": state,
        // The card's OWN room — boards are per-room, so a subscriber that reads
        // the board or posts a verdict must scope to THIS room, never to whatever
        // current_room() happens to be (the documented #345 wrong-room trap;
        // found live 2026-08-15 when the grader read the grading citizen's
        // academy board and never found the bench card).
        "room_id": event.room_id.as_uuid().to_string(),
    }))
}

/// Once-per-process sighting of a (card_id, state) TRANSITION — the cross-path dedup
/// between the in-process verb emit and the wire-echo bridge.
///
/// # Why two feeders exist at all (2026-08-17, the night two solved cards graded as
/// nothing)
///
/// `work/state` used to rely ENTIRELY on its own transcript echo returning through a
/// persona subscribe stream (`bridge_wire_work_event`, the only caller). Measured that
/// night: Atlas closed astropy-14182 and astropy-14995 — both `Closed` in the store,
/// on the board — and `persona.inbound.raw_event` counted 2 rows in 80 minutes (a
/// comparable window earlier the same day: 83). #434 (post-reboot durable delivery to
/// citizen scopes down) starves the echo, the bridge never fires, the grader never
/// hears, and finished work grades as nothing. A grade tail that depends on wire
/// delivery working is a grade tail with a known-open bug in its spine.
///
/// So the VERB now emits directly (in-process, delivery-proof) and the bridge remains
/// for every writer that is NOT this core's `work/state` (operator CLI, remote peers).
/// When delivery works both paths see the same transition; THIS ring makes it publish
/// once. Keyed by (card, state) rather than wire event id because the verb path has no
/// wire event yet. A LEGITIMATE re-transition to the same state hours later would be
/// deduped only if the ring still held it — 256 transitions of churn ages it out long
/// before that matters.
pub(super) fn first_transition_sighting(card_id: &str, state: &str) -> bool {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    const SEEN_CAP: usize = 256;
    static SEEN: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    let key = format!("{card_id}\u{1}{state}");
    let seen = SEEN.get_or_init(|| Mutex::new(VecDeque::with_capacity(SEEN_CAP)));
    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    if seen.contains(&key) {
        return false;
    }
    if seen.len() >= SEEN_CAP {
        seen.pop_front();
    }
    seen.push_back(key);
    true
}

/// Publish one card-state transition onto the internal bus — the ONE emitter both
/// feeders (the `work/state` verb, the wire bridge) route through. Dedup lives HERE so
/// neither feeder needs to know the other exists.
pub(crate) async fn emit_card_state_changed(payload: Value, via: &'static str) {
    let card_id = payload["card_id"].as_str().unwrap_or("").to_string();
    let state = payload["state"].as_str().unwrap_or("").to_string();
    if !first_transition_sighting(&card_id, &state) {
        return;
    }
    continuum_base::probe!(
        class = "work.card.state_changed.bridged",
        via = via,
        card_id = %card_id,
        state = %state,
        "card-state transition published onto the internal bus"
    );
    if let Some((bus, registry)) = WORK_EVENT_BUS.get() {
        bus.publish(WORK_CARD_STATE_CHANGED, payload, registry)
            .await;
    }
}

/// Once-per-process sighting of a wire event id. Every resident persona's subscribe
/// stream yields the SAME room event once, so the bridge below would otherwise
/// publish N copies for N residents; first sighting wins. Bounded ring — old ids
/// age out, which is safe because duplicate deliveries arrive within the same
/// fan-out instant, not hours apart.
pub(super) fn first_sighting(event_id: Uuid) -> bool {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    const SEEN_CAP: usize = 256;
    static SEEN: OnceLock<Mutex<VecDeque<Uuid>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(VecDeque::with_capacity(SEEN_CAP)));
    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    if seen.contains(&event_id) {
        return false;
    }
    if seen.len() >= SEEN_CAP {
        seen.pop_front();
    }
    seen.push_back(event_id);
    true
}

/// THE one emitter of [`WORK_CARD_STATE_CHANGED`] — bridge a card-state transition
/// heard on the WIRE onto the internal bus. The state change in the airc store is
/// the single source of truth, and its transcript echo is the single event source:
/// a citizen's own `work/state` (her echo arrives on her own subscribe stream), the
/// operator's `airc work state`, and a remote peer's transition all fire subscribers
/// (the grade-on-done handler) identically. Before this bridge, only the continuum
/// `work/state` VERB emitted — a card closed by any other writer changed the board
/// and graded nothing (found live 2026-08-15: Asha's real rle_roundtrip artifact,
/// operator close, zero grade).
pub async fn bridge_wire_work_event(event: &airc_core::TranscriptEvent, observer: Uuid) {
    // Review delivery is durable/idempotent in its existing storage owner. Do
    // not mark it seen before awaiting: cancellation must permit exact replay.
    crate::persona::training_producer::reviewed::bridge_review(event, observer).await;
    let Some(payload) = wire_card_state_payload(event) else {
        return;
    };
    if !first_sighting(event.event_id.as_uuid()) {
        return;
    }
    emit_card_state_changed(payload, "wire-echo").await;
}

/// Default claim lease (ms) — 30 min — when the caller doesn't set one. The claim
/// is heartbeat-extendable; this is just the initial TTL.
///
/// Shared with the persona runtime's heartbeat pump
/// ([`crate::persona::airc_runtime`]), which renews a citizen's live claims on the
/// presence cadence. ONE lease length, one place — a renewal that used a different
/// TTL than the claim would make "how long is my hold good for" answerable two ways.
pub(crate) const DEFAULT_CLAIM_TTL_MS: u64 = 30 * 60 * 1000;

/// Resolve the caller's runtime, retaining the owner of live membership changes.
/// A caller-less command uses the operator self-peer; an authenticated caller
/// must resolve its own runtime and can never fall through to another identity.
pub(crate) fn persona_runtime(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
    // What the CALLER actually invoked, for the refusal text (#358).
    family: &str,
) -> Result<Arc<PersonaAircRuntime>, CommandError> {
    // ONE resolver (`operator_peer::acting_runtime`): a persona acts as herself, an
    // agent session as the agent self-peer, a caller-less session as the operator.
    crate::persona::operator_peer::acting_runtime(registry, ctx, family).map(|(rt, _)| rt)
}

/// Read/write the caller's own airc handle. Membership mutations use
/// [`persona_runtime`] instead so they also notify its live subscriptions.
pub(crate) fn persona_airc(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
    family: &str,
) -> Result<Arc<Airc>, CommandError> {
    Ok(persona_runtime(registry, ctx, family)?.airc().clone())
}

pub(super) fn priority_str(p: Priority) -> &'static str {
    match p {
        Priority::P0 => "p0",
        Priority::P1 => "p1",
        Priority::P2 => "p2",
        Priority::P3 => "p3",
    }
}

/// Resolve a card id THE WAY THE BOARD TEACHES IT. The board projection renders
/// cards with 8-char short ids (`card 08ece9e8 [Open]`); the lifecycle verbs
/// demanded the full 32-char UUID, so a persona quoting the id she was SHOWN
/// was rejected — glass-boxed 2026-07-10 minutes after the verbs opened: Anwen
/// AND Asha both executed real `work/claim({"card_id":"08ece9e8"})` calls and
/// both bounced on "expected length 32". A handle a projection displays must
/// be accepted by the verbs that consume it (positron consistency).
///
/// The prefix/near-miss decision + candidate resolution now live in the shared
/// [`continuum_base::id_resolve`] primitive (this was the proven outlier it was lifted
/// from); here we supply the ONLY card-specific knowledge — the candidate set is
/// the live board's card ids.
pub(super) async fn resolve_card_id(
    airc: &std::sync::Arc<airc_lib::Airc>,
    s: &str,
) -> Result<WorkCardId, CommandError> {
    // Fast path: a clean UUID needs no board read.
    if let continuum_base::id_resolve::IdMatch::Full(id) = continuum_base::id_resolve::normalize(s) {
        return Ok(WorkCardId::from_uuid(id));
    }
    let horizon = board_horizon(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("board read for id resolution: {e}")))?;
    resolve_card_id_in_boards(&horizon, s)
}

/// Resolve against an already-read view, so reading a card need not fold the
/// subscribed boards again after expanding its short id.
pub(super) fn resolve_card_id_in_boards(horizon: &BoardHorizon, s: &str) -> Result<WorkCardId, CommandError> {
    if let continuum_base::id_resolve::IdMatch::Full(id) = continuum_base::id_resolve::normalize(s) {
        return Ok(WorkCardId::from_uuid(id));
    }
    let candidates: Vec<Uuid> = horizon
        .boards
        .iter()
        .flat_map(|(_, board)| {
            board
                .snapshot()
                .cards
                .into_iter()
                .map(|c| c.card_id.as_uuid())
        })
        .collect();
    // An empty candidate set is the horizon's story to tell (outage, no rooms, or
    // empty boards), not the resolver's "no cards exist".
    if candidates.is_empty() {
        return Err(horizon.not_found("card", s));
    }
    continuum_base::id_resolve::resolve(s, &candidates, "card")
        .map(WorkCardId::from_uuid)
        .map_err(CommandError::Invalid)
}

/// Resolve a claim id the same way [`resolve_card_id`] resolves cards (#164:
/// short-form ids must resolve on EVERY id param, not just card_id). The board
/// projection and the [work] grounding facts render claim ids in short form, so
/// the lifecycle verbs (release/heartbeat) must accept the handle the persona
/// was SHOWN. Candidates are the live board's claim ids; the prefix/near-miss
/// decision is the shared `id_resolve` primitive — one resolution behavior for
/// every id kind, no second copy of the rules.
pub(super) async fn resolve_claim_id(
    airc: &std::sync::Arc<airc_lib::Airc>,
    s: &str,
) -> Result<ClaimId, CommandError> {
    if let continuum_base::id_resolve::IdMatch::Full(id) = continuum_base::id_resolve::normalize(s) {
        return Ok(ClaimId::from_uuid(id));
    }
    let horizon = board_horizon(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("board read for claim resolution: {e}")))?;
    let candidates: Vec<Uuid> = horizon
        .boards
        .iter()
        .flat_map(|(_, board)| {
            board
                .snapshot()
                .cards
                .iter()
                .filter_map(|c| c.claim_id.map(|id| id.as_uuid()))
                .collect::<Vec<_>>()
        })
        .collect();
    // The identical outage shape as cards (Fable, #4299 review): an empty claim set
    // during a failed walk is the horizon's story, never "no claims exist".
    if candidates.is_empty() {
        return Err(horizon.not_found("claim", s));
    }
    continuum_base::id_resolve::resolve(s, &candidates, "claim")
        .map(ClaimId::from_uuid)
        .map_err(|e| CommandError::Invalid(claim_gone_hint(e)))
}

/// A claim id that resolves to nothing on a readable board has one dominant cause the
/// resolver cannot name: the lease EXPIRED (or was released) and the claim left the
/// board. `work/heartbeat` — the verb whose purpose is to stop a lease expiring — told
/// Kimi "no claim matches id prefix … available claim ids: …" for her own lapsed claim
/// (2026-09-21, Fable), which reads as a typo, not as an expiry. Say the cause and the
/// way out, on every claim miss.
pub(super) fn claim_gone_hint(resolver_error: String) -> String {
    format!(
        "{resolver_error}. A claim that is gone from the board has EXPIRED or been released — \
         if it was yours, re-claim the card (work/claim) and retry — YOUR WORK IS UNTOUCHED, a lapsed claim takes nothing from your checkout"
    )
}

pub(super) fn parse_state(s: &str) -> Result<CardState, CommandError> {
    match s.to_ascii_lowercase().as_str() {
        "open" => Ok(CardState::Open),
        "claimed" => Ok(CardState::Claimed),
        "in_progress" | "inprogress" | "in-progress" => Ok(CardState::InProgress),
        "blocked" => Ok(CardState::Blocked),
        "review" => Ok(CardState::Review),
        "merged" => Ok(CardState::Merged),
        "closed" | "done" => Ok(CardState::Closed),
        other => Err(CommandError::Invalid(format!(
            "unknown card state '{other}' (open|claimed|in_progress|blocked|review|merged|closed)"
        ))),
    }
}

// ─────────────────────────── work/claim ──────────────────────────
/// The room whose board holds `card_id` — the ONE place that answers *which
/// activity does this card belong to*.
///
/// A card carries no room of its own (airc's `WorkCard` has no such field);
/// boards are PER-ROOM, so a card's activity IS the room whose board it appears
/// on. Two callers need that fact and used to derive it separately — badly: the
/// wrong-room claim retry inlined the scan, and the claim-fired solve dispatch
/// did not derive it at all and went roomless instead (#425, measured: 13,209
/// roomless turns, 35% of one citizen's cognition, invisible to every room).
///
/// Rooms are tried in subscription order and a card exists on exactly one board,
/// so the first hit is the only hit. A caller can only read boards of rooms it is
/// SUBSCRIBED to, so this widens no visibility — it only stops discarding what
/// the caller can already see.
pub(crate) async fn room_holding_card(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
) -> Option<airc_lib::Room> {
    card_in_subscribed_rooms(airc, card_id)
        .await
        .map(|(room, _)| room)
}

/// The card AND the subscribed room whose board holds it — the one walk behind
/// [`room_holding_card`], kept whole for callers that need the card's own fields
/// (its title is what on-claim staging resolves the recipe from).
pub(crate) async fn card_in_subscribed_rooms(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
) -> Option<(airc_lib::Room, airc_lib::WorkCard)> {
    board_horizon_for_card(airc, Some(card_id))
        .await
        .ok()?
        .boards
        .into_iter()
        .find_map(|(room, board)| {
            board
                .snapshot()
                .cards
                .iter()
                .find(|c| c.card_id == card_id)
                .map(|card| (room, card.clone()))
        })
}

/// Every board the caller can SEE: one fold per subscribed room, in subscription
/// order. This is the ONE definition of "a card I can address" for this scope.
///
/// The scope's current room is a cwd-derived default, not a horizon: a citizen
/// resident in the academy AND a run room addresses a card by prefix meaning
/// "a card on any board I am in". Resolving prefixes against the current room
/// alone answered Joaquin's `work/get 0c4317e1` with "available: the 12 bench
/// cards" — the run room's board — while her academy board held the card the
/// whole time (2026-09-05; the fold, the cursor and the page size were all
/// exonerated before the resolver was read). A room whose board fails to read
/// is skipped, never fatal: a stale run room must not hide the academy.
/// The board a READ verb looks at: `room`'s when named (a room name or channel id
/// this persona is subscribed to), else the CURRENT room's. Boards are per room —
/// #academy holds 176 cards, #continuum 231, a fresh activity room none — and a
/// citizen seated in a triage room saw an EMPTY board because `work/list` read the
/// room she was standing in (2026-09-17). The board she triages is the board she
/// names. Write verbs are unaffected: they resolve a card across every subscribed
/// board already.
pub(crate) async fn board_to_read(
    airc: &Arc<Airc>,
    room: Option<&str>,
) -> Result<airc_lib::BoardSnapshot, CommandError> {
    let projection = match room.map(str::trim).filter(|r| !r.is_empty()) {
        Some(name) => {
            let room = airc
                .room_by_name_or_channel(name, "read the work board of")
                .await
                .map_err(|e| CommandError::Invalid(format!("room {name:?}: {e}")))?;
            airc.project_room_work_board(&room, airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                .await
                .map_err(|e| CommandError::Internal(format!("board read ({name}): {e}")))?
        }
        None => airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?,
    };
    Ok(projection.snapshot())
}

/// Every board the caller can see, AND every subscribed room whose board could not
/// be read this walk. A skipped room is not fatal (the doc on [`board_to_read`]:
/// a stale run room must not hide the academy) — but it is not SILENT either.
/// Kimi's `work/get 31c241e2` ran during a daemon outage (2026-09-21, card
/// cb4dea0f): every board read failed, the walk returned an empty set, and the
/// resolver told her "no cards exist … there are none to choose from" — a read
/// failure reported as an absence, and she reasoned from it. The horizon carries
/// the failures so a miss can say which it was.
pub(crate) struct BoardHorizon {
    pub(crate) boards: Vec<(airc_lib::Room, airc_lib::WorkBoardProjection)>,
    /// `(room name, error)` for each subscribed room skipped this walk.
    pub(crate) unreadable: Vec<(String, String)>,
}

impl BoardHorizon {
    pub(super) fn readable_room_names(&self) -> Vec<String> {
        self.boards.iter().map(|(room, _)| room.name.clone()).collect()
    }

    /// The truthful "not found" for this walk — see [`not_found_in`]. `label` is the
    /// id kind the caller resolves (`"card"`, `"claim"`), the same word `id_resolve` gets.
    pub(super) fn not_found(&self, label: &str, requested: &str) -> CommandError {
        not_found_in(
            &self.readable_room_names(),
            &self.unreadable,
            label,
            requested,
        )
    }
}

/// One sentence per way an id can fail to be found on the boards a citizen can see,
/// because they call for different next moves: a read failure means RETRY, an empty
/// subscription means JOIN the card's room, and a readable miss means the card lives
/// in a room she has not joined. Folding all three into "no cards exist" (the
/// resolver's empty-candidate line) sent Kimi looking for a fold bug during an outage.
pub(super) fn not_found_in(
    readable_rooms: &[String],
    unreadable: &[(String, String)],
    label: &str,
    requested: &str,
) -> CommandError {
    if !unreadable.is_empty() {
        let failed = unreadable
            .iter()
            .map(|(room, err)| format!("{room} ({err})"))
            .collect::<Vec<_>>()
            .join("; ");
        let searched = if readable_rooms.is_empty() {
            "none".to_string()
        } else {
            readable_rooms.join(", ")
        };
        return CommandError::Internal(format!(
            "{label} {requested}: board read FAILED in {} of {} subscribed room(s) — {failed}; \
             readable boards searched: {searched}. This is a read failure, not an absence: \
             the {label} may be on an unreadable board — retry before concluding it does not exist",
            unreadable.len(),
            unreadable.len() + readable_rooms.len(),
        ));
    }
    if readable_rooms.is_empty() {
        return CommandError::NotFound(format!(
            "{label} {requested}: you are subscribed to no rooms, so there is no board to \
             search — join the room that holds the {label} and retry"
        ));
    }
    if label == "claim" {
        return CommandError::NotFound(format!(
            "claim {requested}: no claims at all on the boards of the rooms you are in ({}) — \
             a claim that is gone from the board has EXPIRED or been released; if it was \
             yours, re-claim the card (work/claim) and retry — YOUR WORK IS UNTOUCHED, a lapsed claim takes nothing from your checkout",
            readable_rooms.join(", ")
        ));
    }
    CommandError::NotFound(format!(
        "{label} {requested}: not on the board of any room you are in ({}) — it lives in a \
         room you have not joined",
        readable_rooms.join(", ")
    ))
}

pub(crate) async fn board_horizon(airc: &Arc<Airc>) -> Result<BoardHorizon, airc_lib::AircError> {
    board_horizon_for_card(airc, None).await
}

/// A fully identified card needs only its first containing board, as the existing
/// resolver's first match does. Prefixes need the complete horizon to detect ambiguity.
/// Keep one walk and one read-failure policy for both; unrelated later rooms must not
/// delay an already located card (44c5612e: Intel work/get took 121–161 seconds while
/// its containing board read in under one second).
pub(super) async fn board_horizon_for_card(
    airc: &Arc<Airc>,
    card_id: Option<WorkCardId>,
) -> Result<BoardHorizon, airc_lib::AircError> {
    let set = airc.subscription_set().await?;
    let mut horizon = BoardHorizon {
        boards: Vec::new(),
        unreadable: Vec::new(),
    };
    for sub in set.all() {
        let room = sub.as_room();
        match airc.work_board_in(&room).await {
            Ok(board) => {
                let found = card_id.is_some_and(|id| board.card(id).is_some());
                horizon.boards.push((room, board));
                if found {
                    break;
                }
            }
            Err(e) => horizon.unreadable.push((room.name, e.to_string())),
        }
    }
    Ok(horizon)
}

/// Locate `card_id`'s room, switch the caller's current room there, and retry the
/// claim once.
///
/// Returns `None` when no subscribed room holds the card, or when the card's room
/// IS the current one (the room that already refused has nothing to follow) — in
/// both cases the original wrong-room refusal stands, verbatim. `Some(result)`
/// means a room-switched claim was attempted and that result REPLACES the refusal,
/// so a genuine contention in the card's room still reports as contention.
/// Follow a card to the room that holds it: join that room (if it is not the
/// current one) so a by-id mutation can be retried there. `Some(room name)`
/// when the caller should retry; `None` when the card is placeable nowhere or
/// the caller already stands in its room. Accept-or-redirect, never
/// refuse-and-instruct — shared by claim, state and release.
pub(crate) async fn follow_card_room(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    verb: &'static str,
) -> Option<String> {
    // The card's room: the subscribed room whose board holds it. The round tracker's
    // node-wide fallback went with the detached runner (card c4f4b8cd step 2).
    let room = room_holding_card(airc, card_id).await?;
    let (room_name, room_channel) = (room.name, room.channel);
    let current = airc.current_room().await.ok();
    if current.as_ref().is_some_and(|c| c.channel == room_channel) {
        return None;
    }
    if airc.join(&room_name).await.is_err() {
        return None;
    }
    continuum_base::probe!(
        class = "work.followed_card_room",
        verb = verb,
        card_id = %short8(card_id.as_uuid()),
        room = %room_name,
        "a by-id mutation targeted a card outside the current room — switched to \
         the card's room and retried (accept-or-redirect, never refuse-and-instruct)"
    );
    Some(room_name)
}

/// What she now holds, in the room the claim LANDED in — the followed room when the
/// claim followed the card. The renewal loop reads this beside its subscription walk,
/// so a claim on a room this scope does not subscribe to is still renewed (card
/// 2d9df546: Kimi's followed claim got no heartbeat across 28 acts and lapsed under her
/// while `persona.claim.renewed` read green for a bench card). Best-effort: a claim
/// whose room cannot be read is still hers; the walk covers what it can.
pub(crate) async fn record_held_claim(airc: &Arc<Airc>, card_id: WorkCardId, claim_id: ClaimId) {
    let Ok(room) = airc.current_room().await else {
        return;
    };
    crate::persona::held_claims::record(
        airc.peer_id().as_uuid(),
        airc.home(),
        crate::persona::held_claims::HeldClaim {
            room,
            card_id: card_id.as_uuid(),
            claim_id: claim_id.as_uuid(),
            recorded_at_ms: EpochMs::now().millis(),
            refusals: 0,
        },
    );
}

pub(crate) async fn claim_following_card_room(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    ttl_ms: u64,
    origin: airc_work::ClaimOrigin,
) -> Option<Result<ClaimId, airc_lib::AircError>> {
    follow_card_room(airc, card_id, "work/claim").await?;
    Some(
        airc.claim_work_card_with_origin(ClaimWorkCard { card_id, ttl_ms }, origin)
            .await,
    )
}

/// Claim a work card for this persona (as its own airc key).
pub struct WorkClaim {
    pub registry: PersonaAircRuntimeRegistry,
}

/// Selection at the command boundary, persisted with the accepted claim.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkClaimOrigin {
    #[default]
    Explicit,
    Automatic,
}

impl From<WorkClaimOrigin> for airc_work::ClaimOrigin {
    fn from(origin: WorkClaimOrigin) -> Self {
        match origin {
            WorkClaimOrigin::Explicit => Self::Explicit,
            WorkClaimOrigin::Automatic => Self::Automatic,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkClaimParams {
    /// How this claim was selected — and deliberately NOT part of the projected
    /// schema. Every `Automatic` setter is Rust-internal (`persona/work_focus.rs`,
    /// `commands/benchmark.rs`) and builds `airc_work::ClaimOrigin` directly, never
    /// these params, so on the wire this field can only ever be its default. Offering
    /// it therefore bought nothing and cost two things: catalog tokens in every
    /// citizen's turn (#333, the agentic-surface ratchet), and a way for her to declare
    /// a deliberate claim `automatic` — falsifying the very provenance this field was
    /// added to preserve. `#[serde(default)]` keeps an omitted field `Explicit`, so the
    /// wire contract is unchanged; `WorkState` still sets it explicitly in Rust.
    #[serde(default)]
    #[schemars(skip)]
    pub origin: WorkClaimOrigin,
    /// The card id (UUID) to claim — from the board (`airc work board`).
    pub card_id: String,
    /// Lease length in ms before the claim goes stale. Defaults to 30 min;
    /// extend with a heartbeat.
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkClaimResult {
    pub card_id: String,
    pub claim_id: String,
}
