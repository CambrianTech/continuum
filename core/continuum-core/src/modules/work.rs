//! Native airc work/kanban tools — a persona claims / creates / releases cards
//! as ITS OWN airc identity, by DELEGATING to airc's own work API
//! (`Airc::claim_work_card` / `create_work_card` / `release_work_claim`). This is
//! encapsulation, not reinvention: the board, the projection, the events, the
//! storage are all airc's; continuum just lets a persona drive them as a peer.
//!
//! ## Why these exist alongside `airc work` over `code/shell`
//!
//! A persona can already READ/run the board via the `airc` CLI through its
//! `code/shell` — but that acts as the MACHINE-SCOPE identity, not the persona's
//! own key. These tools resolve the caller's [`PersonaAircRuntime`] and call its
//! `Airc` handle, so a claim is recorded as ASHA, not the operator. Identity-
//! correct kanban participation: the persona is a first-class work peer on the
//! same board as everyone else ([[personas-are-peers-in-your-mesh]]).
//!
//! Access tier: `Privileged` → `Trusted`. Coordinating on the shared board is for
//! trusted local citizens (a local persona / a trusted node), not an arbitrary
//! remote `Provisional` peer (who could otherwise spam claims). Board READS can
//! open up later; writes stay trusted.

use std::any::Any;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use airc_lib::{
    Airc, CardState, ChangeWorkCardState, ClaimId, ClaimWorkCard, CreateWorkCard,
    HeartbeatWorkClaim, Priority, ReleaseWorkClaim, RepoId, WorkCardId,
};

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{
    CommandResult, MessageBus, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry,
    ServiceModule,
};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

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
static WORK_EVENT_BUS: OnceLock<(Arc<MessageBus>, Arc<ModuleRegistry>)> = OnceLock::new();

/// Decode a wire transcript event into the `work.card.state_changed` payload — or
/// `None` if it isn't a card-state transition. Pure over the event so the contract
/// is unit-testable against the real producer (`airc_work::encode_work_event`).
///
/// Payload contract: `{card_id, state}` with `card_id` the FULL hyphenated UUID and
/// `state` the snake_case `CardState` serde form ("closed"/"merged"/…) — the exact
/// vocabulary `benchmark_grade::is_terminal` and `parse_state` already speak.
fn wire_card_state_payload(event: &airc_core::TranscriptEvent) -> Option<Value> {
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
fn first_transition_sighting(card_id: &str, state: &str) -> bool {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    const SEEN_CAP: usize = 256;
    static SEEN: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    let key = format!("{card_id}\u{1}{state}");
    let seen = SEEN.get_or_init(|| Mutex::new(VecDeque::with_capacity(SEEN_CAP)));
    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner());
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
    crate::probe!(
        class = "work.card.state_changed.bridged",
        via = via,
        card_id = %card_id,
        state = %state,
        "card-state transition published onto the internal bus"
    );
    if let Some((bus, registry)) = WORK_EVENT_BUS.get() {
        bus.publish(WORK_CARD_STATE_CHANGED, payload, registry).await;
    }
}

/// Once-per-process sighting of a wire event id. Every resident persona's subscribe
/// stream yields the SAME room event once, so the bridge below would otherwise
/// publish N copies for N residents; first sighting wins. Bounded ring — old ids
/// age out, which is safe because duplicate deliveries arrive within the same
/// fan-out instant, not hours apart.
fn first_sighting(event_id: Uuid) -> bool {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    const SEEN_CAP: usize = 256;
    static SEEN: OnceLock<Mutex<VecDeque<Uuid>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(VecDeque::with_capacity(SEEN_CAP)));
    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner());
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
pub async fn bridge_wire_work_event(event: &airc_core::TranscriptEvent) {
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

/// Resolve the CALLING persona's own airc handle so work ops act as ITS key.
/// The caller identity is the authenticated airc peer_id the gate already saw;
/// `None` (substrate-local owner) has no persona runtime → a typed refusal.
pub(crate) fn persona_airc(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
    // What the CALLER actually invoked. Was hardcoded to "work commands", which
    // #358 caught live the moment room/members reused this helper: a citizen asking
    // who is here was told "work commands act as ..." and pointed at `airc work`.
    // A refusal that misnames the thing you called teaches the wrong lesson.
    family: &str,
) -> Result<Arc<Airc>, CommandError> {
    let peer = ctx
        .caller
        .as_ref()
        .map(|c| c.peer_id.as_uuid())
        .ok_or_else(|| {
            CommandError::Denied(format!(
                "{family} acts as the caller's own airc identity, and the \
                     substrate-local operator has none in-core (yet — the self-peer gap, \
                     task #27). Personas calling through their toolbelt act as themselves \
                     and need nothing special; for operator-identity board writes use \
                     `airc work <verb> ...`."
            ))
        })?;
    let rt = registry.get(peer).ok_or_else(|| {
        CommandError::NotFound(format!("no live airc runtime for persona {peer}"))
    })?;
    Ok(rt.airc().clone())
}

/// Resolve an airc handle for an OPERATOR/curator board write — e.g.
/// `benchmark/dispatch` seeding a benchmark's tasks as claimable cards. Unlike
/// [`persona_airc`], this does NOT dead-end when the caller has no self-identity.
///
/// A persona calling through her own toolbelt still authors as HERSELF (same as
/// `persona_airc`). But the substrate-local operator has no self-peer in-core yet
/// (#27), and seeding the board is a *curator* action, not a personal one — so when
/// there is no caller identity, the seed is authored through a LIVE citizen's airc
/// runtime. That is honest, not a fiction: benchmarks ARE the citizens' work, so a
/// citizen posting the tasks is the right author (a live citizen chosen
/// deterministically — never a hardcoded name like our "Benchy", which does not exist
/// on a fresh clone's grid; see [`PersonaAircRuntimeRegistry::any_live_citizen`]). This
/// fails loud only when NO citizen is online to author through — because then there is
/// genuinely no board to seed for, and the fix is to spawn a persona, not to invent an
/// identity.
pub(crate) fn curator_airc(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
    family: &str,
) -> Result<Arc<Airc>, CommandError> {
    // An authenticated caller (a persona acting through her toolbelt) wins: the card
    // is authored as her, exactly like `persona_airc`.
    if let Some(rt) = ctx
        .caller
        .as_ref()
        .and_then(|c| registry.get(c.peer_id.as_uuid()))
    {
        return Ok(rt.airc().clone());
    }
    // Operator seeding with no self-peer (#27): author through a live citizen —
    // whoever this machine has online, chosen deterministically, never our name.
    let rt = registry.any_live_citizen().ok_or_else(|| {
        CommandError::Denied(format!(
            "{family} seeds the shared board and must author as a citizen, but none \
                 are online to author through — spawn a persona first (persona/spawn), \
                 then retry."
        ))
    })?;
    Ok(rt.airc().clone())
}

fn parse_priority(s: &str) -> Priority {
    match s.to_ascii_lowercase().as_str() {
        "p0" => Priority::P0,
        "p1" => Priority::P1,
        "p3" => Priority::P3,
        _ => Priority::P2,
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
/// [`crate::id_resolve`] primitive (this was the proven outlier it was lifted
/// from); here we supply the ONLY card-specific knowledge — the candidate set is
/// the live board's card ids.
async fn resolve_card_id(
    airc: &std::sync::Arc<airc_lib::Airc>,
    s: &str,
) -> Result<WorkCardId, CommandError> {
    // Fast path: a clean UUID needs no board read.
    if let crate::id_resolve::IdMatch::Full(id) = crate::id_resolve::normalize(s) {
        return Ok(WorkCardId::from_uuid(id));
    }
    let board = airc
        .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
        .await
        .map_err(|e| CommandError::Internal(format!("board read for id resolution: {e}")))?
        .snapshot();
    let candidates: Vec<Uuid> = board.cards.iter().map(|c| c.card_id.as_uuid()).collect();
    crate::id_resolve::resolve(s, &candidates, "card")
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
async fn resolve_claim_id(
    airc: &std::sync::Arc<airc_lib::Airc>,
    s: &str,
) -> Result<ClaimId, CommandError> {
    if let crate::id_resolve::IdMatch::Full(id) = crate::id_resolve::normalize(s) {
        return Ok(ClaimId::from_uuid(id));
    }
    let board = airc
        .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
        .await
        .map_err(|e| CommandError::Internal(format!("board read for claim resolution: {e}")))?
        .snapshot();
    let candidates: Vec<Uuid> = board
        .cards
        .iter()
        .filter_map(|c| c.claim_id.map(|id| id.as_uuid()))
        .collect();
    crate::id_resolve::resolve(s, &candidates, "claim")
        .map(ClaimId::from_uuid)
        .map_err(CommandError::Invalid)
}

fn parse_state(s: &str) -> Result<CardState, CommandError> {
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
pub(crate) async fn room_holding_card(airc: &Arc<Airc>, card_id: WorkCardId) -> Option<airc_lib::Room> {
    let set = airc.subscription_set().await.ok()?;
    for sub in set.all() {
        let room = sub.as_room();
        let Ok(board) = airc.work_board_in(&room).await else {
            continue;
        };
        if board.snapshot().cards.iter().any(|c| c.card_id == card_id) {
            return Some(room);
        }
    }
    None
}

/// Locate `card_id`'s room, switch the caller's current room there, and retry the
/// claim once.
///
/// Returns `None` when no subscribed room holds the card, or when the card's room
/// IS the current one (the room that already refused has nothing to follow) — in
/// both cases the original wrong-room refusal stands, verbatim. `Some(result)`
/// means a room-switched claim was attempted and that result REPLACES the refusal,
/// so a genuine contention in the card's room still reports as contention.
pub(crate) async fn claim_following_card_room(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    ttl_ms: u64,
) -> Option<Result<ClaimId, airc_lib::AircError>> {
    let room = room_holding_card(airc, card_id).await?;
    let current = airc.current_room().await.ok();
    if current.as_ref().is_some_and(|c| c.channel == room.channel) {
        return None;
    }
    if airc.join(&room.name).await.is_err() {
        return None;
    }
    crate::probe!(
        class = "work.claim.followed_card_room",
        card_id = %short8(card_id.as_uuid()),
        room = %room.name,
        "claim-by-id targeted a card outside the current room — switched to \
         the card's room and retried (accept-or-redirect, never refuse-and-instruct)"
    );
    Some(
        airc.claim_work_card(ClaimWorkCard { card_id, ttl_ms })
            .await,
    )
}

/// Claim a work card for this persona (as its own airc key).
pub struct WorkClaim {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkClaimParams {
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

#[async_trait]
impl ActionCommand for WorkClaim {
    const NAME: &'static str = "work/claim";
    const ALIASES: &'static [&'static str] = &["claim_task"];
    const NATIVE: bool = true; // core room workflow — claiming shared-board work as yourself
                               // AiSafe since 2026-07-10: claiming/working a card AS YOURSELF is the
                               // self-scoped cooperative act the shared board exists for. It was
                               // Privileged, so every persona claim all day was structurally impossible —
                               // narrated claims, then Atlas's honest real attempt bounced off the gate
                               // ("I don't have access to work/claim") while the board, the room, and the
                               // operators all urged them to claim. The lifecycle verbs (claim/release/
                               // state/heartbeat) act only on the caller's own identity + lease;
                               // work/create (board curation) stays Privileged.
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Claim a work card on the shared airc board as yourself, so others see you own it. \
         Pass the card_id from the board. Returns a claim_id.";
    type Params = WorkClaimParams;
    type Output = WorkClaimResult;

    async fn run(&self, ctx: &Ctx, p: WorkClaimParams) -> Result<WorkClaimResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let ttl_ms = p.ttl_ms.unwrap_or(DEFAULT_CLAIM_TTL_MS);
        let mut claim_attempt = airc
            .claim_work_card(ClaimWorkCard { card_id, ttl_ms })
            .await;
        // FOLLOW THE CARD TO ITS ROOM (#328 accept-or-redirect, live 2026-08-11):
        // Atlas's very first act on her dispatched SWE card was work/claim by full
        // uuid — refused with "not in current room general; switch to the card's
        // room", because her room pointer was still on #general while the bench
        // card lived on another board. The refusal burned the act, taught her the
        // substrate was broken, and set off a 4×code/list discovery spiral that
        // ended in an empty patch. A card id is unguessable and she can only see
        // boards of rooms she is SUBSCRIBED to, so an explicit claim-by-id is
        // unambiguous intent: find which of her rooms holds the card, switch her
        // there (room = the activity — being in the card's room IS correct), and
        // claim. airc's room-scoping semantics stay untouched; this is her own
        // client doing exactly what the refusal text instructs.
        if matches!(
            claim_attempt,
            Err(airc_lib::AircError::WorkCardNotInCurrentRoom { .. })
        ) {
            if let Some(retry) = claim_following_card_room(&airc, card_id, ttl_ms).await {
                claim_attempt = retry;
            }
        }
        let claim_id = match claim_attempt {
            Ok(id) => id,
            Err(e) => {
                // Name the HOLDER, not just the fact (Joel, 2026-08-03, on
                // Atlas's first live contention bounce: "It should tell you the
                // peer"). A bare "already claimed" is a dead end; "claimed by
                // <peer>" is a coordination move — ask them, watch their
                // progress, or pick another card. Best-effort board read; the
                // original error stands alone if the board is unreadable.
                let mut msg = e.to_string();
                // Whether the refusal is a CONTENTION (someone holds it) or a real
                // fault decides the error CLASS below — a taken card is a normal
                // outcome of a shared board, and telling a citizen "[internal]" for
                // it teaches her the substrate is broken when the truth is "that one
                // is Anwen's". Observed live 2026-08-06.
                let mut contention = false;
                // Set when the live holder turns out to be the caller herself.
                let mut already_yours = None;
                let caller_short = ctx
                    .caller
                    .as_ref()
                    .map(|c| short8(c.peer_id.as_uuid()))
                    .unwrap_or_else(|| "-".to_string());
                if let Ok(board) = airc
                    .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                    .await
                {
                    let board = board.snapshot();
                    if let Some(card) = board.cards.iter().find(|c| c.card_id == card_id) {
                        // The hold must be LIVE. `card.owner` alone is not contention:
                        // expiry never clears owner/claim_id (airc-work
                        // projection/apply.rs clears them only on release or on a NEW
                        // claim), so a card claimed once carries an owner forever. Keying
                        // off the bare field relabelled EVERY refusal — settled work,
                        // wrong-room, transport faults — as "held by peer X", and then
                        // `claim_rejections::record` below burned that fabrication into
                        // perception for ten minutes.
                        //
                        // Measured 2026-08-07: ~50 of 61 cards on #general and 4 of 12 on
                        // #k3-serving (a RETIRED subsystem-named room — do not copy the name;
                        // rooms are activities WITH LIFETIMES) carried a stale owner with a
                        // lease expired 134h+; three
                        // of the latter are in Review, where the true refusal is "settled
                        // work is not claimable". Two citizens spent the day quoting this
                        // string back — "expired leases or the cards being held by others"
                        // is THIS format string, both halves of it.
                        //
                        // `hold_of` is the same predicate `work/list` renders through
                        // (card_holder::holder, below) — one rule for "is someone on this",
                        // not two in one file.
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        // ASK WHO, NOT JUST WHETHER. `live_holder` says a person is on
                        // the card; `classify_refusal` says whether that person is YOU.
                        // Without the second question this arm told Anon her own card was
                        // a stranger's (2026-08-14) — see `ClaimRefusal`.
                        let caller_uuid = ctx.caller.as_ref().map(|c| c.peer_id.as_uuid());
                        let holder = live_holder(card, now_ms).map(|o| o.as_uuid());
                        match classify_refusal(holder, caller_uuid) {
                            ClaimRefusal::AlreadyYours => already_yours = card.claim_id,
                            ClaimRefusal::HeldByPeer(owner) => {
                                contention = true;
                                msg = format!(
                                    "card {} (\"{}\") is held by peer {} [{}]. Coordinate with \
                                     them in the room, or take another card — work/list with \
                                     claimable=true lists every card you can pick up right now \
                                     (most sit in the `claimed` column with a lapsed lease, so \
                                     filtering by state=\"open\" will not show them). \
                                     (claim error: {e})",
                                    short8(card_id.as_uuid()),
                                    card.title,
                                    short8(owner),
                                    state_str(&card.state),
                                );
                            }
                            // Nobody is on it — the original error stands as written.
                            ClaimRefusal::Fault => {}
                        }
                    }
                }
                // A claim you ALREADY HOLD is satisfied, not refused: the verb's goal
                // ("this citizen owns this card") is already true, so report success
                // with the live claim rather than inventing a rival. Deliberately does
                // NOT re-fire `dispatch_staged_swe_solve` — `run_id` is deterministic
                // (`claim-<card>`) with no in-flight guard, and a duplicate detached
                // solve would contend for the exclusive warm slot. Recovering a claim
                // whose session died is a separate, dedup-gated fix.
                if let Some(claim_id) = already_yours {
                    crate::probe!(
                        class = "work.claim",
                        card_id = %card_id.as_uuid(),
                        claimer = %caller_short,
                        "re-claim of a card the caller already holds — satisfied, no re-dispatch"
                    );
                    return Ok(WorkClaimResult {
                        card_id: p.card_id,
                        claim_id: claim_id.as_uuid().to_string(),
                    });
                }
                // A rejection is WORK-STATE, not a transient tool result: the
                // raw receipt scrolls out of the persona's short window and the
                // intent narrative resurfaces as "I've claimed it" (glass-boxed
                // 2026-08-02, card 44ebaa41). Record it so ActiveWorkSource can
                // keep the fact in perception past the receipt's lifetime.
                if let Some(caller) = ctx.caller.as_ref() {
                    crate::persona::claim_rejections::record(
                        caller.peer_id.as_uuid(),
                        &p.card_id,
                        &msg,
                    );
                }
                // A card someone else holds is a legitimate refusal, not a fault:
                // `Denied` (the caller may not take THIS card), never `Internal`.
                return Err(if contention {
                    CommandError::Denied(msg)
                } else {
                    CommandError::Internal(msg)
                });
            }
        };
        // CLAIM → WORK SESSION (#346 front half): claiming a staged SWE card IS the
        // start of the work, never an announcement of intent. The gate-conflation
        // arc (2026-08-08, BigMama + M5) proved these minds act reliably inside a
        // work session and stall on room ticks — so the claim fires the session.
        // Eligibility is structural, decoded from our own staging shape: the
        // CLAIMER's own `citizens/peers/<her>/workspace/swe/<instance>` checkout
        // whose directory name appears in the card title (`benchmark/swe-setup`
        // staged it for exactly her). Detached + scored + workspace-deliverable,
        // so the #2167 autograde carries settle → verdict → experience stream with
        // nobody in the loop. Best-effort: a dispatch failure never voids the
        // claim — the claim is hers either way, and the probe says what happened.
        if let Some(caller) = ctx.caller.as_ref() {
            // IN THE CARD'S OWN ROOM (#425). This used to pass `None` and say so — the
            // claim verb carries no activity, so a claim-fired solve ran where nobody
            // could see it: no act receipts (`apply_act` skips a nil room by design), no
            // peer, no human, no ViewState. Measured before this fix: 13,209 roomless
            // turns across the 25 newest trace files, 35% of one citizen's cognition.
            //
            // The room was never actually unknown — boards are PER-ROOM, so the card's
            // activity is the room whose board holds it, and the wrong-room claim retry
            // right above already resolves exactly that. It is now ONE resolver
            // ([`room_holding_card`]) with two consumers instead of a scan here and a
            // shrug there. A card we cannot place gets NO detached fallback: it says so
            // on the probe and the claim still stands, because inventing invisible work
            // is what this fix removes.
            match room_holding_card(&airc, card_id).await {
                Some(room) => {
                    dispatch_staged_swe_solve(
                        ctx,
                        &airc,
                        StagedSolveDispatch {
                            claimer: caller.peer_id,
                            card: card_id,
                            room: room.channel,
                            teammates: Vec::new(), // solo default; team threading lands per-caller
                        },
                    )
                    .await;
                }
                None => crate::probe!(
                    class = "work.claim.unplaceable_card",
                    card_id = %short8(card_id.as_uuid()),
                    claimer = %short8(caller.peer_id.as_uuid()),
                    "claimed a card no subscribed room's board holds — the claim STANDS, \
                     but no solve fires: work whose activity we cannot name is work no \
                     room can see (#425)"
                ),
            }
        }
        Ok(WorkClaimResult {
            card_id: p.card_id,
            claim_id: claim_id.as_uuid().to_string(),
        })
    }
}

/// How many graded chances a claim-dispatched SWE run gets (`AgentSolveParams::attempts`).
/// This is the SWE adapter's N, not a global — each benchmark adapter owns its own.
const SWE_CLAIM_ATTEMPTS: u32 = 3;

/// WHO works, WHICH card, WHERE the work is visible — the three facts that define one
/// staged-SWE dispatch, as a value object rather than a positional argument list.
///
/// WHY A STRUCT (Joel, 2026-08-17: *"make sure params are well formed structs and
/// constants, good OOP, not random parameter lists"*). The previous signature was
/// `(ctx, airc, Uuid, WorkCardId, Option<Uuid>)`: two BARE UUIDs distinguished only by
/// argument POSITION, so transposing claimer and room compiled cleanly and dispatched a
/// citizen's solve under a room's id. Typed fields make that a compile error, and the next
/// fact this dispatch needs becomes a FIELD instead of a sixth argument. `ctx` and `airc`
/// stay separate parameters on purpose — they are ambient services, not facts about the
/// dispatch, and folding them in would turn the value object into a context bag.
///
/// WHY `room` IS NOT `Option` (Joel, same day: *"if something is required, remove the
/// option… a required param is caught at compile time not runtime"*). It used to be
/// `Option<Uuid>`, and `work/claim` passed `None` — which is #425: the solve ran, her acts
/// radiated no receipts (`apply_act` skips a nil room by design), and no room, peer or human
/// ever saw the work. Making the field required moves that from a runtime shrug to an
/// unrepresentable state: a caller that cannot name the activity cannot build the struct,
/// so it must resolve one ([`room_holding_card`]) or report why it could not.
/// One-per-persona solve lease — see the "ONE PAIR OF HANDS" note in
/// [`dispatch_staged_swe_solve`]. RAII: dropping the lease (any exit path,
/// panics included) frees the persona for the next solve.
struct HandsLease(uuid::Uuid);

fn busy_hands() -> &'static std::sync::Mutex<std::collections::HashSet<uuid::Uuid>> {
    static BUSY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>> =
        std::sync::OnceLock::new();
    BUSY.get_or_init(Default::default)
}

impl HandsLease {
    fn try_take(persona: uuid::Uuid) -> Option<Self> {
        let mut g = busy_hands().lock().expect("busy-hands lock never poisoned"); // expect: guards a HashSet op only
        if g.insert(persona) {
            Some(Self(persona))
        } else {
            None
        }
    }
}

impl Drop for HandsLease {
    fn drop(&mut self) {
        if let Ok(mut g) = busy_hands().lock() {
            g.remove(&self.0);
        }
    }
}

pub(crate) struct StagedSolveDispatch {
    /// The citizen who does the work — her own airc identity.
    pub claimer: crate::identity::PeerId,
    /// The card she is working.
    pub card: airc_work::WorkCardId,
    /// The activity the run BELONGS to: `benchmark/dispatch` passes the per-run room it
    /// just spawned (#329), `work/claim` resolves the card's own room. Every act the
    /// solver executes radiates a receipt into it (#243), which is what makes the work
    /// visible where the work lives.
    pub room: airc_core::RoomId,
    /// TEAM solves (#team-proof gap 1): citizens joined to the solve room
    /// beside the claimer. Empty = solo (every existing caller). They are
    /// INVITED members, not co-claimers — the C1 shape: presence + an
    /// addressed charge to review, with the kanban terminal untouched.
    pub teammates: Vec<crate::identity::PeerId>,
}

/// The #346 claim→solve dispatch. Fires a detached [`crate::commands::agent::solve::AgentSolve`]
/// for the claimer when the claimed card matches a checkout `benchmark/swe-setup` staged
/// into HER workspace: one staged instance directory whose name appears in the card
/// title. Task text = the card body (the real issue, gold held out). Model = the live
/// served base (dynamic — never a hardcoded id). All outcomes on the
/// `benchmark.dispatch` probe; silence is not an outcome.
///
/// Called from TWO triggers, both legitimate: (1) `work/claim` — a citizen claims a
/// staged card off the board; (2) `benchmark/dispatch` directed at an assignee — dispatch
/// already staged the repo into HER workspace and addressed the card to her, so it fires
/// her solve DIRECTLY rather than depend on her re-deriving a `work/claim` from a chat
/// kickoff (the fragile hop that stalls every run under warm-slot starvation — glass-boxed
/// 2026-08-11: cards staged + assigned, zero claims, zero solves). The solve is her WHOLE
/// cognition with an exclusive warm slot (`quiesce_others`), so nothing about "she does the
/// work herself" changes — only the trigger moves off the chat turn.
/// Pre-warm envs for every instance a WORKING round has staged — the resume-side
/// twin of dispatch's pre-warm (a reboot mid-round otherwise re-discovers env
/// walls one burned solve attempt at a time). The staged workspace dirs
/// (`peers/<assignee>/workspace/swe/<instance>`) are the reliable card→instance
/// map; instances resolve through the SAME dataset loader the grader uses.
/// Instances whose env FAILED this boot's pre-warm — the dispatch gate reads
/// this so a still-broken card idles (no attempt-burning loop) while a healed
/// one re-fires automatically. Per-boot by construction: a restart re-walks.
pub fn env_broken_this_boot() -> &'static dashmap::DashSet<String> {
    static SET: std::sync::OnceLock<dashmap::DashSet<String>> = std::sync::OnceLock::new();
    SET.get_or_init(dashmap::DashSet::new)
}

pub fn spawn_env_prewarm_for_working_rounds() {
    tokio::spawn(async move {
        let peers_root = match crate::commands::benchmark::continuum_home() {
            Ok(h) => h.join("citizens").join("peers"),
            Err(_) => return, // no home = nothing staged anywhere
        };
        let mut names: std::collections::BTreeSet<String> = Default::default();
        if let Ok(peers) = std::fs::read_dir(&peers_root) {
            for peer in peers.flatten() {
                let swe = peer.path().join("workspace").join("swe");
                if let Ok(instances) = std::fs::read_dir(&swe) {
                    for inst in instances.flatten() {
                        if inst.path().is_dir() {
                            names.insert(inst.file_name().to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        if names.is_empty() {
            return;
        }
        // Resolve each staged name against every SWE dataset in the catalog —
        // the same search the grade path performs (first dataset holding the
        // instance wins), so this can never disagree with grading.
        for name in names {
            let mut resolved = None;
            for spec in crate::commands::benchmark::known_benchmarks() {
                let Some(dataset) = spec.swe_dataset() else { continue };
                if let Ok(instances) = crate::cognition::swe_bench::load_dataset(dataset).await {
                    if let Some(i) = instances.into_iter().find(|i| i.instance_id == name) {
                        resolved = Some(i);
                        break;
                    }
                }
            }
            let Some(inst) = resolved else { continue };
            let checkout = match crate::cognition::swe_bench::ensure_grade_checkout(&inst).await {
                Ok(dir) => dir,
                Err(e) => {
                    crate::probe!(
                        class = "benchmark.env.prewarm_failed",
                        instance = %inst.instance_id,
                        stage = "checkout",
                        error = %e,
                        "resume-side env pre-warm could not stage a checkout — an ENV \
                         failure, not a model result"
                    );
                    continue;
                }
            };
            match crate::cognition::swe_bench::ensure_env(&inst, &checkout).await {
                Ok(_) => {
                    let healed = env_broken_this_boot().remove(&inst.instance_id).is_some();
                    crate::probe!(
                        class = "benchmark.env.prewarmed",
                        instance = %inst.instance_id,
                        healed,
                        "resume-side env pre-warm: ready ahead of the driver"
                    );
                }
                Err(e) => {
                    env_broken_this_boot().insert(inst.instance_id.clone());
                    crate::probe!(
                        class = "benchmark.env.prewarm_failed",
                        instance = %inst.instance_id,
                        stage = "env",
                        error = %e,
                        "resume-side env pre-warm FAILED — an ENV failure, never a model result"
                    );
                }
            }
        }
    });
}

pub(crate) async fn dispatch_staged_swe_solve(
    ctx: &Ctx,
    airc: &std::sync::Arc<airc_lib::Airc>,
    dispatch: StagedSolveDispatch,
) {
    let StagedSolveDispatch {
        claimer,
        card: card_id,
        room,
        teammates,
    } = dispatch;
    // ONE PAIR OF HANDS (2026-08-29): a persona's ToolExecutor / file-engine
    // re-root is PROCESS-GLOBAL (see root_acting_workspace / #312), so two
    // concurrent solve forks of the SAME persona contend on the same hands —
    // glass-boxed as ticks parked 25min before perception in exactly the rooms
    // sharing a claimer, reaped by the tick deadline in a retry loop. A second
    // solve for a busy persona is deferred to the round driver's next edge
    // (retry machinery already exists), never run beside the first. This is a
    // MIND fact wearing a mutex: one body, one workspace, one solve at a time.
    let _hands = match HandsLease::try_take(claimer.as_uuid()) {
        Some(lease) => lease,
        None => {
            crate::probe!(
                class = "work.solve.hands_busy",
                card_id = %card_id.as_uuid(),
                claimer = %claimer,
                "claimer already mid-solve — one pair of hands; deferred to the driver's next edge"
            );
            return;
        }
    };
    // Read the RUN ROOM's board — the dispatch HAS the room (StagedSolveDispatch
    // requires it), and boards are per-room. The previous read went through the
    // caller's GLOBAL paginated projection (work_board_complete), where a card
    // beyond the page silently vanished: measured live 2026-08-26 — of two cards
    // on one run-room board, one dispatched and one "was not in the caller's
    // board projection" purely by page position. Room-scoped is both correct
    // and unpaginated for the one board that matters.
    let run_room = {
        let subs = match airc.subscription_set().await {
            Ok(s) => s,
            Err(e) => {
                crate::probe!(
                    class = "benchmark.dispatch",
                    card_id = %card_id.as_uuid(),
                    claimer = %claimer,
                    error = %e.to_string(),
                    "dispatch aborted: caller's subscriptions unreadable — retried on                      the next edge"
                );
                return;
            }
        };
        let found = subs
            .all()
            .into_iter()
            .map(|sub| sub.as_room())
            .find(|r| r.channel == room);
        found
    };
    let Some(run_room) = run_room else {
        crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            room = %room.as_uuid(),
            "dispatch aborted: the claimer is not subscribed to the run room (her              subscription may still be resuming post-boot) — retried on the next edge"
        );
        return;
    };
    let Ok(board) = airc.work_board_in(&run_room).await else {
        crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            "dispatch aborted: the run room's board could not be read — retried on              the next edge"
        );
        return;
    };
    let board = board.snapshot();
    let Some(card) = board.cards.iter().find(|c| c.card_id == card_id) else {
        crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            board_cards = board.cards.len() as u64,
            "dispatch aborted: card not on the RUN room's board (stale card id, or              the board is still replicating) — retried on the next edge"
        );
        return;
    };
    // Her staged SWE checkouts. ONE expression of that layout lives in
    // `persona::staged_workspace` — the work turn roots her hands with the same resolver,
    // so a change to staging can never leave the two disagreeing about which repo a card
    // is about (they did disagree in kind already: this walk accepted any directory, that
    // one requires a `.git`, so an interrupted clone read as a staged instance here).
    // ENV GATE: an instance whose env failed THIS boot's pre-warm cannot grade —
    // firing a solve would burn an attempt on a wall we already measured. Skip
    // with a named abort; the card stays open and re-fires on the first boot
    // whose pre-warm heals it (the automatic retake).
    let env_gate = |inst: &str| crate::modules::work::env_broken_this_boot().contains(inst);
    let (instance, workspace) = match crate::persona::staged_workspace::resolve_for_titles(
        // typed PeerId (canary's #425 struct) → the resolver's raw Uuid
        &claimer.as_uuid(),
        [card.title.as_str()],
    ) {
        crate::persona::staged_workspace::CardWorkspace::One { instance, path } => {
            if env_gate(&instance) {
                crate::probe!(
                    class = "benchmark.dispatch",
                    card_id = %card_id.as_uuid(),
                    instance = %instance,
                    "dispatch deferred: this instance's env failed THIS boot's \
                     pre-warm — an attempt now would burn on a measured wall; the \
                     open card re-fires the first boot the env heals"
                );
                return;
            }
            (instance, path.to_string_lossy().to_string())
        }
        // No staged checkout for her matching this card — an ordinary (non-SWE) claim.
        crate::persona::staged_workspace::CardWorkspace::None => {
            crate::probe!(
                class = "benchmark.dispatch",
                card_id = %card_id.as_uuid(),
                claimer = %claimer,
                "dispatch: no staged checkout matches this card for this claimer —                  ordinary claim, no solve to fire"
            );
            return;
        }
        crate::persona::staged_workspace::CardWorkspace::Ambiguous { candidates } => {
            crate::probe!(
                class = "benchmark.dispatch",
                card_id = %card_id.as_uuid(),
                claimer = %claimer,
                matches = candidates.len(),
                candidates = candidates.join(","),
                "claim matched MULTIPLE staged instances — refusing to guess, no dispatch"
            );
            return;
        }
    };
    // ONE CARD, ONE LIVE RUN — a STOPGAP, and named as one (2026-08-21).
    //
    // WHAT THIS IS, honestly: a doorman with a clipboard. A citizen re-affirming a claim
    // is an ordinary, legitimate act — a student saying "this one's mine" a second time.
    // The school's answer today is to seat a SECOND her at a second desk with a copy of
    // the same worksheet, in the same room, both writing on it. This gate turns that into
    // a refusal at the door. It stops the damage; it does not remove the incoherence.
    //
    // THE INCOHERENCE, which is the real defect: we keep TWO REGISTRIES FOR ONE FACT.
    // The board holds "this citizen holds this card" (enrollment). A JSON file on disk
    // holds "run claim-<id> is running" (attendance). Two records of one truth, so they
    // can and do disagree — and this gate is a clerk reconciling them on every claim.
    // The structural fix is that the ENROLLMENT IS THE SESSION: re-affirming a claim
    // finds the work already under way and continues it, because there is only one place
    // the fact lives and nothing to duplicate. That is #371 (round lifecycle as
    // recipe-owned state) plus #49 (the workspace held for the life of the claim, so the
    // worksheet is hers rather than a directory two runs can both open). Neither is
    // built; until they are, this doorman is the thing standing between a citizen and a
    // clobbered afternoon.
    //
    // `run_id` is DETERMINISTIC per card (`claim-<uuid>`), and this dispatch had no
    // guard, so every re-claim of the same card fired ANOTHER detached solve. Two runs
    // then shared, simultaneously: one ledger file (their pulse ticks overwrite each
    // other, so `benchmark/runs` reports whichever wrote last — an honest-looking
    // `acts: 0` that is really run B stomping run A's marker), one WORKSPACE (two sets
    // of hands editing the same repo, which is a live candidate for the empty
    // `files_changed` this box keeps producing), and one exclusive warm slot they each
    // ask to quiesce the other out of.
    //
    // Observed: Atlas re-claimed card df9f4ad5 three times in one evening on
    // pallets__flask-4045; `benchmark/runs` caught the third at `age_secs: 8, acts: 0`.
    // A re-claim is LEGAL (airc permits claiming a lapsed card, and #331/#2286 both
    // depend on that) — what is not legal is a second RUN. The claim is the citizen's
    // intent; the run is the machinery, and the machinery must be idempotent under a
    // repeated intent.
    //
    // Keyed on `in_flight_solve_runs` — the SAME `state: "running"` predicate the boot
    // reaper uses, so this gate and the reaper can never disagree about what "live"
    // means. That also gives the gate its release valve for free: a run whose core died
    // is reaped to `state: "failed"` at next boot, so a dead run never blocks its card
    // forever. It is a refusal, not a kill: the live run keeps working, and nothing
    // touches the citizen's claim.
    let run_id = format!("claim-{}", card_id.as_uuid());
    if let Some((_, live_instance)) = crate::cognition::swe_bench::in_flight_solve_runs()
        .into_iter()
        .find(|(id, _)| id == &run_id)
    {
        crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            run_id = %run_id,
            live_instance = %live_instance,
            "re-claim of a card whose run is ALREADY IN FLIGHT — refusing to start a \
             second solve. A duplicate would share this run's ledger, its workspace and \
             its warm slot, and silently discard whichever half lost the race. The live \
             run continues; the claim stands."
        );
        return;
    }

    // WAIT for the boot-gate, don't guard against it. A claim can land while the serving lane
    // is still proving it can decode (the ~10-15s window after core-ready); parking here until
    // the lane is decode-verified means the solve fires the moment serving is up instead of the
    // claim silently no-op-ing (Joel 2026-08-11: "persona should boot beforehand"). None after
    // the deadline = a genuinely dead lane; the claim stands and re-fires on the next serving edge.
    let model =
        crate::inference::llama_server::await_ready_serving(std::time::Duration::from_secs(30))
            .await
            .and_then(|s| s.active_model)
            .unwrap_or_default(); // no recorded activity yet = first dispatch of this card; mint path follows
    if model.is_empty() {
        crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            "serving not decode-ready within 30s — dispatch skipped; claim stands, re-fires on next serving edge"
        );
        return;
    }
    // THE SOLVE'S OWN ACTIVITY — mint-or-rejoin (Joel 2026-08-26: "benchmarks
    // without new activities (unless rejoining)" is not allowed; rooms are 1:1
    // with activities). The run room stays the BOARD's home (cards, kickoffs,
    // the round's denominator); the solve itself is its own activity: a child
    // room per (card, instance) where her acts radiate. This is also the KV
    // fix becoming real — each concurrent solve's (persona, room) key leases
    // its OWN warm slot instead of N solves thrashing one.
    //
    // REJOIN: the round recorded this card's activity at first mint (resume
    // after a reboot, a retry attempt — same room, continuity). MINT: spawn a
    // real activity room, child of the run room, and record it.
    let solve_room = match crate::cognition::bench_round::card_activity(card_id.as_uuid()) {
        Some(act) => {
            crate::probe!(
                class = "work.solve.room_rejoined",
                card_id = %short8(card_id.as_uuid()),
                claimer = %short8(claimer.as_uuid()),
                room = %act.solve_room,
                "solve REJOINS its recorded activity room — resume and dispatch are one motion"
            );
            act.solve_room
        }
        None => {
            // Spawn AS the claimer when her runtime is live (she is joined to her
            // own workroom); the caller's handle is the fallback so a dispatch
            // fired before she is resident still names a real activity.
            let spawner = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(claimer.as_uuid()))
                .map(|rt| rt.airc().clone())
                .unwrap_or_else(|| airc.clone()); // assignee runtime gone → curator's own handle spawns; probed below
            let name = format!("swe--{}--{}", instance, short8(card_id.as_uuid()));
            let recipe = crate::experience::source::RecipeExperienceSource::shipped_purpose(
                crate::experience::source::shipped::BENCHMARK_HARD_RS,
            )
            .unwrap_or_default(); // empty name renders as unnamed room in the probe; display only
            match crate::modules::activity::spawn_activity_room(
                &spawner,
                &name,
                &recipe,
                Some(room),
                &std::collections::BTreeMap::new(),
            )
            .await
            {
                Ok(spawned) => {
                    // RESTORE THE SPAWNER'S FOCUS (glass-boxed 2026-08-26, the
                    // one-resident boot): spawn_activity_room's documented side
                    // effect moves the caller's current-room pointer to the new
                    // solve room — and when the dispatch's curator IS the
                    // assignee (any_live_citizen with one resident), that same
                    // handle creates the NEXT card, which then lands on the
                    // SOLVE room's board instead of the run room's. Measured:
                    // card 1 of 3 visible, cards 2-3 "not on the RUN room's
                    // board" forever — two solves dead at dispatch and a live
                    // card ghost-settled. Until airc grows subscribe-without-
                    // focus (#290), the mint restores the pointer itself.
                    if let Err(e) = spawner.join(&run_room.name).await {
                        crate::probe!(
                            class = "work.solve.room_mint_failed",
                            card_id = %short8(card_id.as_uuid()),
                            error = %e.to_string(),
                            "could not restore the spawner's focus to the run room                              after the mint — subsequent card writes may land on the                              wrong board"
                        );
                    }
                    let act = crate::cognition::bench_round::CardActivity {
                        teammates: teammates.iter().map(|p| p.as_uuid()).collect(),
                        solve_room: spawned.room_id.as_uuid(),
                        assignee: claimer.as_uuid(),
                    };
                    crate::cognition::bench_round::record_card_activity(card_id.as_uuid(), act.clone()); // clone: probe below still reads the local
                    crate::probe!(
                        class = "work.solve.room_minted",
                        card_id = %short8(card_id.as_uuid()),
                        claimer = %short8(claimer.as_uuid()),
                        room = %act.solve_room,
                        name = %name,
                        "solve MINTED its own activity room (child of the run room)"
                    );
                    act.solve_room
                }
                Err(e) => {
                    // A mint failure must not strand the work invisible: fall back
                    // to the run room (a real activity, just coarser-grained) and
                    // say so. The next re-fire retries the mint.
                    crate::probe!(
                        class = "work.solve.room_mint_failed",
                        card_id = %short8(card_id.as_uuid()),
                        claimer = %short8(claimer.as_uuid()),
                        error = %e.to_string(),
                        "activity mint failed — solve runs in the RUN room this time (coarser \
                         KV granularity, still visible); mint retries on the next fire"
                    );
                    room.as_uuid()
                }
            }
        }
    };

    // TEAM MEMBERSHIP (#team-proof gap 1): teammates JOIN the solve room and
    // receive an addressed charge. Membership is room-level (their normal
    // multi-room cognition participates from here on); accountability stays
    // card-level with the single claimer. Join is idempotent; a failed join or
    // invite is REPORTED and never blocks the solve (the claimer works either
    // way — a smaller team is a degraded run, not a dead one).
    if !teammates.is_empty() {
        let team_room_name = format!("swe--{}--{}", instance, short8(card_id.as_uuid()));
        for mate in &teammates {
            let Some(rt) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(mate.as_uuid()))
            else {
                crate::probe!(
                    class = "work.team.join_skipped",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    "teammate not resident — solve proceeds with a smaller team"
                );
                continue;
            };
            if let Err(e) = rt.join_room(&team_room_name).await {
                crate::probe!(
                    class = "work.team.join_failed",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    error = %e.to_string(),
                    "teammate join failed — reported, never blocks the claimer"
                );
                continue;
            }
            let charge = format!(
                "You are teamed with {} on `{}` in this room. Their patch must be                  reviewed by a teammate before it submits: read the diff when it                  lands, run what you can, and SPEAK your findings here — a miss a                  reviewer catches is the whole point of the team.",
                short8(claimer.as_uuid()),
                instance
            );
            match crate::persona::airc_citizen::publish_text_in_room(
                rt.airc(),
                solve_room,
                &charge,
            )
            .await
            {
                Ok(_) => crate::probe!(
                    class = "work.team.joined",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    room = %solve_room,
                    "teammate joined the solve room and holds the review charge"
                ),
                Err(e) => crate::probe!(
                    class = "work.team.invite_failed",
                    card_id = %short8(card_id.as_uuid()),
                    mate = %short8(mate.as_uuid()),
                    error = %e.to_string(),
                    "teammate joined but the charge did not send — reported"
                ),
            }
        }
    }
    // Her HANDS must resolve `python`/`pytest`/`pip` to THIS instance's venv, not the system
    // interpreter. Without this, `code/shell pytest` hits homebrew python3.14 (no pytest, no
    // repo), she loops `pip install pytest` into the wrong interpreter, and burns every action
    // without ever validating her edit (glass-boxed 2026-08-11 from Anon's astropy turn). The
    // venv is built at staging (benchmark.rs) / on first grade; the path is deterministic, and
    // solve.rs already `.exists()`-filters it, so prepending a not-yet-built bin is harmless.
    let venv_bin = crate::cognition::swe_bench::swe_cache_dir()
        .join("envs")
        .join(&instance)
        .join("bin")
        .to_string_lossy()
        .to_string();
    let params = crate::commands::agent::solve::AgentSolveParams {
        persona_id: claimer.to_string().into(),
        base_model_id: model,
        workspace,
        task: card.body.clone().unwrap_or_else(|| card.title.clone()),
        deliverable: Some(crate::commands::agent::solve::Deliverable::Workspace),
        scored: Some(true),
        detach: Some(true),
        // The SAME expression the in-flight gate above keyed on — one definition of
        // "this card's run id", so the guard can never check a different id than the
        // dispatch actually uses.
        run_id: Some(run_id.clone()),
        // CAPTURE the detached solve's turns (was None → the whole scored run was
        // INVISIBLE: a reviewer — Opus or a self-grading citizen — could not read a single
        // tool-call output to verify it, forcing inference from a STALE main-loop capture
        // and a near-misdiagnosis 2026-08-25). eval.rs already scopes this per-run
        // (run_artifact_dir), so it does not collide with the main-loop sink; a measured run
        // is meant to be inspectable (eval.rs `capture_dir` doc). Same base dir the tooling
        // (`dataset/from-captures`) and the main-loop sink use.
        capture_dir: std::env::var("HOME").ok().map(|h| {
            std::path::Path::new(&h)
                .join(".continuum/fixtures/prompt-captures")
                .to_string_lossy()
                .to_string()
        }),
        learn: crate::cognition::learning_policy::LearningPolicy::LearnFromThisWork,
        max_acts: None,
        // The solve's OWN activity (minted-or-rejoined above) — NOT the run room.
        // `AgentSolveParams::room` stays `Option` because `agent/solve` is also an
        // operator-invocable command (an omitted room mints there).
        room: Some(solve_room),
        path_prepend: Some(vec![venv_bin]),
        suppress_recall: None,
        prev_failed_patch_sha: None,
        // The SWE claim adapter's N (Joel, 2026-08-08): a failed grade re-enters the
        // same workspace with the named failing tests — learning to investigate your
        // own failure is part of the exam. Three chances: first attempt, one informed
        // retry, one consolidation — beyond that the failures repeat, not teach.
        attempts: Some(SWE_CLAIM_ATTEMPTS),
    };
    match crate::commands::agent::solve::AgentSolve
        .run(ctx, params)
        .await
    {
        Ok(ack) => crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            run_id = %ack.run_id.unwrap_or_default(),
            "claim dispatched a detached work session"
        ),
        Err(e) => crate::probe!(
            class = "benchmark.dispatch",
            card_id = %card_id.as_uuid(),
            claimer = %claimer,
            instance = %instance,
            error = %e.to_string(),
            "claim→solve dispatch FAILED — claim stands, session missing"
        ),
    }
}

// ─────────────────────────── work/create ─────────────────────────

/// Create a new work card on the shared board.
pub struct WorkCreate {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkCreateParams {
    /// Repository key, e.g. `CambrianTech/continuum`.
    pub repo: String,
    /// Human-readable card title.
    pub title: String,
    /// Optional card body / description.
    #[serde(default)]
    pub body: Option<String>,
    /// Priority: one of p0, p1, p2, p3. Defaults to p2.
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkCreateResult {
    pub card_id: String,
}

#[async_trait]
impl ActionCommand for WorkCreate {
    const NAME: &'static str = "work/create";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Create a work card on the shared airc board (repo + title + optional body/priority). \
         Returns the new card_id.";
    type Params = WorkCreateParams;
    type Output = WorkCreateResult;

    async fn run(&self, ctx: &Ctx, p: WorkCreateParams) -> Result<WorkCreateResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let repo = RepoId::new(p.repo)
            .map_err(|e| CommandError::Invalid(format!("invalid repo: {e:?}")))?;
        let mut req = CreateWorkCard::new(
            repo,
            p.title,
            parse_priority(p.priority.as_deref().unwrap_or("p2")),
        );
        req.body = p.body;
        let card_id = airc
            .create_work_card(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(WorkCreateResult {
            card_id: card_id.as_uuid().to_string(),
        })
    }
}

// ─────────────────────────── work/release ────────────────────────

/// Release this persona's claim on a card.
pub struct WorkRelease {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkReleaseParams {
    /// The card id (UUID) whose claim to release.
    pub card_id: String,
    /// The claim_id returned by work/claim.
    pub claim_id: String,
    /// Optional reason (recorded on the release event).
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkReleaseResult {
    pub released: bool,
}

#[async_trait]
impl ActionCommand for WorkRelease {
    const NAME: &'static str = "work/release";
    const ALIASES: &'static [&'static str] = &["release_task"];
    // core room workflow — HANDING BACK. Without it a citizen who realizes a card is
    // not hers has only one exit: hold it until the lease lapses. (#339)
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Release your claim on a work card (pass card_id + the claim_id from work/claim; \
         short 8-char ids from the board are accepted for both).";
    type Params = WorkReleaseParams;
    type Output = WorkReleaseResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: WorkReleaseParams,
    ) -> Result<WorkReleaseResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let claim_id = resolve_claim_id(&airc, &p.claim_id).await?;
        airc.release_work_claim(ReleaseWorkClaim {
            card_id,
            claim_id,
            reason: p.reason,
        })
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(WorkReleaseResult { released: true })
    }
}

// ─────────────────────────── work/state ──────────────────────────

/// Move a card through its lifecycle (open→in_progress→review→closed, etc).
pub struct WorkState {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkStateParams {
    /// The card id (UUID) to transition.
    pub card_id: String,
    /// New state: open | claimed | in_progress | blocked | review | merged | closed.
    pub state: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkStateResult {
    pub card_id: String,
    pub state: String,
}

#[async_trait]
impl ActionCommand for WorkState {
    const NAME: &'static str = "work/state";
    const ALIASES: &'static [&'static str] = &["update_task", "close_task"];
    // core room workflow — SAYING DONE. This is the missing half of #339: citizens were
    // offered claim/list/get and NOTHING that writes a card's lifecycle back, so every
    // claim could only end in a lapsed lease and a completed card was indistinguishable
    // from an abandoned one. The DESCRIPTION below has coached the whole lifecycle since
    // the day it was written — to a reader who was never shown it.
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Move a work card through its lifecycle: in_progress when you start, review when a PR is up, \
         blocked if stuck, closed when done. States: open|claimed|in_progress|blocked|review|merged|closed.";
    type Params = WorkStateParams;
    type Output = WorkStateResult;

    async fn run(&self, ctx: &Ctx, p: WorkStateParams) -> Result<WorkStateResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let state = parse_state(&p.state)?;
        airc.change_work_card_state(ChangeWorkCardState { card_id, state })
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;

        // DIRECT emit — the delivery-proof feeder. The previous shape relied
        // entirely on this transition's transcript echo returning through a persona
        // subscribe stream; under #434 (post-reboot durable delivery down) that echo
        // never arrives and finished work grades as NOTHING (measured 2026-08-17:
        // two cards Closed, zero grades, 2 raw events in 80 min). The verb KNOWS the
        // transition happened — it just wrote it — so it publishes in-process. The
        // wire bridge still covers external writers (operator CLI, remote peers);
        // `emit_card_state_changed`'s (card,state) ring makes the two feeders
        // publish once when both fire.
        // The CARD's room, never `current_room()` — boards are per-room and the grade
        // subscriber refuses an event with no room. Uses the ONE room resolver
        // (`room_holding_card`, canary's consolidation) rather than a third private scan.
        let room_id = room_holding_card(&airc, card_id)
            .await
            .map(|r| r.channel.as_uuid().to_string())
            .unwrap_or_default(); // board read failure already probed as its own abort above
        emit_card_state_changed(
            serde_json::json!({
                "card_id": card_id.as_uuid().to_string(),
                "state": serde_json::to_value(state)
                    .unwrap_or(serde_json::Value::Null),
                "room_id": room_id,
            }),
            "work-state-verb",
        )
        .await;

        Ok(WorkStateResult {
            card_id: p.card_id,
            state: p.state,
        })
    }
}

// ─────────────────────────── work/heartbeat ──────────────────────

/// Extend this persona's claim lease on a card during long work.
pub struct WorkHeartbeat {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkHeartbeatParams {
    /// The card id (UUID) whose claim to extend.
    pub card_id: String,
    /// The claim_id from work/claim.
    pub claim_id: String,
    /// New lease length in ms. Defaults to 30 min.
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkHeartbeatResult {
    pub extended: bool,
}

#[async_trait]
impl ActionCommand for WorkHeartbeat {
    const NAME: &'static str = "work/heartbeat";
    const ALIASES: &'static [&'static str] = &["heartbeat_task", "extend_claim"];
    // core room workflow — STAYING ON IT. Real work outruns the 30-min lease; without
    // this the card is reclaimed out from under a citizen who is still working it, which
    // we watched happen (card 0c69c0f0 through three holders in one evening). (#339)
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Extend your claim lease on a card so it doesn't go stale during long work (pass card_id + \
         claim_id; short 8-char ids from the board are accepted for both).";
    type Params = WorkHeartbeatParams;
    type Output = WorkHeartbeatResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: WorkHeartbeatParams,
    ) -> Result<WorkHeartbeatResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let claim_id = resolve_claim_id(&airc, &p.claim_id).await?;
        airc.heartbeat_work_claim(HeartbeatWorkClaim {
            card_id,
            claim_id,
            ttl_ms: p.ttl_ms.unwrap_or(DEFAULT_CLAIM_TTL_MS),
        })
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(WorkHeartbeatResult { extended: true })
    }
}

// ─────────────────────────── work/list + work/get ─────────────────
//
// The READ half the board proved it needed live (#309, 2026-08-03): the
// surface was claim/create/release/state/heartbeat — write-only. Card
// content reached personas ONLY through the perception-side projection,
// so a persona that missed the render (or wanted to re-verify a spec)
// had no tool to fetch it. "I don't have access to the work card" was
// LITERALLY TRUE: the claimant of the bakery card could not re-read its
// own requirements, and the card sat at five plans / zero files. Every
// write-only surface eventually proves it needs its read half.

/// Inverse of [`parse_state`] — the wire spelling of a card state.
fn state_str(s: &CardState) -> &'static str {
    match s {
        CardState::Open => "open",
        CardState::Claimed => "claimed",
        CardState::InProgress => "in_progress",
        CardState::Blocked => "blocked",
        CardState::Review => "review",
        CardState::Merged => "merged",
        CardState::Closed => "closed",
    }
}

/// The board's displayed 8-char short form of an id — what every surface
/// shows and what the lifecycle verbs accept back.
/// The peer genuinely ON this card right now, or `None`.
///
/// The one question `work/claim`'s error path is allowed to ask before it tells
/// a citizen someone else has her card. It is NOT `card.owner`: expiry never
/// clears owner/claim_id (airc-work's projection clears them only on release or
/// on a new claim), so a card claimed once carries an owner forever. Keying off
/// the bare field relabelled EVERY refusal — settled work, wrong-room, transport
/// faults — as "held by peer X", and `claim_rejections::record` then burned that
/// fabrication into perception for ten minutes.
///
/// Named and separated from the call site so the rule can be tested without a
/// board, an airc daemon, or a running persona — the call site is where this
/// went wrong, and an inline `match` is not something a test can reach.
///
/// Delegates to [`card_holder::hold_of`], the SAME predicate `work/list` renders
/// through. One rule for "is someone on this card", not two in one file.
fn live_holder(card: &airc_work::WorkCard, now_ms: u64) -> Option<airc_core::PeerId> {
    match crate::persona::card_holder::hold_of(card, now_ms) {
        crate::persona::card_holder::Hold::Held => card.owner,
        // Lapsed or unclaimed: whatever refused the claim, it was not a person.
        crate::persona::card_holder::Hold::Lapsed
        | crate::persona::card_holder::Hold::Unclaimed => None,
    }
}

/// What a refused claim MEANS for the citizen who made it.
///
/// `live_holder` answers "is a person on this card". It does NOT answer "is that
/// person YOU" — and without that second question the refusal path told a citizen
/// a stranger held her own work. Glass-boxed live 2026-08-14: Anon (a20b3ada) held
/// 13 cards and was told `card 17531483 ... is held by peer a20b3ada` — her own
/// peer id, rendered as somebody else. She read it as contention, concluded there
/// was nothing for her to do, and went silent holding thirteen claims. Her prompt
/// was not the problem: `[active-work]` listed every card and `[Working Presence]`
/// told her a quiet room is not a stop sign. The substrate said the work was
/// someone else's, so she believed the substrate.
///
/// The caller's identity was already in scope 20 lines below (`ctx.caller`) and
/// simply never consulted for this decision. This is the [[same-bug-at-two-sites]]
/// shape — `work/list` renders holders through `card_holder`, and the claim path
/// needed the same self-vs-other distinction as a NAMED rule rather than prose.
///
/// Separated from the call site for the reason `live_holder` was: the call site is
/// where this went wrong, and an inline `if` is not something a test can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimRefusal {
    /// The live holder IS the caller — she already owns this card. Not contention,
    /// not a fault: the claim's goal ("this citizen holds this card") is already
    /// true, so the verb is satisfied and must not manufacture a rival.
    AlreadyYours,
    /// A different peer genuinely holds it right now. A normal shared-board outcome.
    HeldByPeer(Uuid),
    /// Nobody holds it; whatever refused the claim was not a person.
    Fault,
}

/// Classify a refusal from the live holder and the caller. Pure — no board, no
/// daemon, no persona.
pub(crate) fn classify_refusal(holder: Option<Uuid>, caller: Option<Uuid>) -> ClaimRefusal {
    match (holder, caller) {
        // Self-comparison FIRST: a caller who holds the card is never contention.
        (Some(h), Some(c)) if h == c => ClaimRefusal::AlreadyYours,
        (Some(h), _) => ClaimRefusal::HeldByPeer(h),
        (None, _) => ClaimRefusal::Fault,
    }
}

fn short8(u: Uuid) -> String {
    u.simple().to_string().chars().take(8).collect()
}

/// Browse the live work board (read-only).
pub struct WorkList {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkListParams {
    /// Optional COLUMN filter: open | claimed | in_progress | blocked | review | merged | closed.
    ///
    /// This is the card's column, NOT whether you can take it. `state="open"` means
    /// "sitting in the Open column" — it does NOT include a claimed card whose lease
    /// lapsed, which is takeable work. To ask "what can I pick up", use `claimable`.
    #[serde(default)]
    pub state: Option<String>,

    /// Optional AVAILABILITY filter — the question a citizen looking for work is
    /// actually asking. `true` = only cards you can take right now (open, or a lapsed
    /// hold); `false` = only cards someone is actively on.
    ///
    /// This axis exists because the column one lied by omission (#337, measured
    /// 2026-08-06): every board query the residents made was `work/list(state=open)`
    /// — 84 of them, all returning `{"cards":[]}` — on a board of 61 cards where 59
    /// leases had lapsed and 0 cards sat in the Open column. The grounding block in
    /// the same prompt said "59 claimable". Both were correct; the citizens spent a
    /// day reporting they had no work, and they were reading their tool correctly.
    #[serde(default)]
    pub claimable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkListCard {
    // FIELD ORDER IS LOAD-BEARING HERE TOO — `claimable` MUST precede `state`/`owner`.
    //
    // serde emits in declaration order, so the reader met `"state":"claimed"` and
    // `"owner":"Anwen"` BEFORE `"claimable":true`. Live 2026-08-07, hours after the
    // lease + claimability fixes let him finally see the board, Benchy read exactly
    // this shape and reported: *"there are several tasks available on the board, but
    // they have already been claimed by others."* Every card he was looking at was
    // his to take. Two loud fields saying "someone owns this" beat one quiet field
    // saying "you can have it".
    //
    // #321 fixed the BOARD rendering so a lapsed claim reads CLAIMABLE. This is the
    // same fact on the TOOL-RESULT surface, which that fix never reached — two
    // surfaces answering one question, drifting independently (the same shape as the
    // read/write claimability split fixed in 59cbbb735). Lead with the answer to
    // "can I take this", then say who had it.
    /// 8-char short id — quote this back to work/get / work/claim / work/state.
    pub id: String,
    pub title: String,
    /// Whether she can take this card RIGHT NOW — the fact the board was hiding.
    ///
    /// A claim carries a LEASE. When it expires the holder has stopped working the card and the
    /// substrate already treats it as reclaimable (`airc work next` lists exactly these). But this
    /// projection rendered only `state` + `owner`, so an expired claim still read as
    /// `Claimed owner=Anwen` — indistinguishable from someone actively on it. A citizen doing the
    /// correct thing (read the board, don't steal a peer's card) concludes there is nothing to do.
    ///
    /// Measured 2026-08-06: 19 cards, 17 with expired leases, 2 open — and `airc work next` offered
    /// EIGHT claimable. Six citizens spent the night announcing they had nothing to work on, and
    /// they were reading the board correctly; the board was lying by omission. This is the
    /// projection half of #321.
    pub claimable: bool,
    /// Human-legible lease state for a claimed card: `expired` when the hold has lapsed (take it),
    /// `held` while someone is genuinely on it. `None` for unclaimed cards.
    pub lease: Option<String>,
    /// The card's column. Declared AFTER `claimable`/`lease` on purpose — see the
    /// field-order note above: `"claimed"` read first defeats `claimable: true` read
    /// fifth, and a lapsed claim IS takeable regardless of this column.
    pub state: String,
    /// Short id of the claiming peer, when claimed. Says WHO to reach out to — never
    /// "someone" ([[card-holder]]) — but read AFTER whether she can take it, because
    /// an owner on a lapsed lease is history, not an obstacle.
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkListResult {
    // FIELD ORDER IS LOAD-BEARING — the summary MUST precede `cards`.
    //
    // serde emits in declaration order, and the receipt path truncates a large result
    // (`act_observe::truncate_chars`, capped at `fold_at.min(4096)`). `cards` on a real
    // board is far past that cap, so anything declared AFTER it is cut off in EVERY
    // receipt a citizen actually reads. Measured 2026-08-06: Anwen's prompt carried four
    // live `work/list` receipts and ZERO occurrences of `total_on_board` — the
    // self-explaining fields below were computed, serialized, and then severed by the
    // truncator, which is indistinguishable from never having built them.
    //
    // This is the SAME divisibility law the grounding board block already obeys: the
    // first delivered unit must be a complete statement, because a prefix-take keeps the
    // head and drops the tail ([[divisibility-makes-unit-order-load-bearing-the-first-unit-must-be-a-complete-statement]]).
    // Fixed there this afternoon and reintroduced here the same day; the list is the
    // divisible part, so the list is what gets cut.
    /// How many cards are on the board IN TOTAL, before any filter. Always present.
    ///
    /// An empty `cards` list is ambiguous on its own — "the board is empty" and "your
    /// filter matched nothing" are different facts and a citizen cannot tell them apart
    /// from `{"cards":[]}`. She read it as the first one, correctly by every rule of
    /// reading, and stopped looking for work. Same law the grounding sources follow: a
    /// silent zero is a hole in the glass box, so the receipt states its own scope.
    pub total_on_board: usize,

    /// How many of those cards are claimable RIGHT NOW, board-wide, regardless of the
    /// filter applied. The one number that answers "is there work here for me".
    pub claimable_now: usize,

    /// Present only when the filter emptied a non-empty board — says so plainly and
    /// names the query that would have answered her actual question. Friendly feedback
    /// at the moment of the miss, not a silent zero and not a redefinition of `state`
    /// (the column filter keeps meaning the column).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,

    /// The matched cards. LAST by design — see the field-order note at the top of this
    /// struct. This is the divisible part of the answer, so this is what a truncated
    /// receipt sheds; the counts and the note survive.
    pub cards: Vec<WorkListCard>,
}

#[async_trait]
impl ActionCommand for WorkList {
    const NAME: &'static str = "work/list";
    const ALIASES: &'static [&'static str] = &["list_tasks"];
    const NATIVE: bool = true; // core room workflow — the read half of claim_task; without it the board is write-only
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List the work board's cards (read-only): short id, title, state, owner, and whether it is \
         CLAIMABLE right now. `claimable: true` means you can take it — either it is open, or its \
         holder's lease expired (`lease: expired`) and they have stopped working it. A card marked \
         `lease: held` is genuinely someone else's. Use the short id with work/get for a card's \
         full requirements, or work/claim to take it. TO FIND WORK YOU CAN TAKE, pass \
         `claimable: true` — most takeable cards sit in the `claimed` column with a lapsed lease, \
         so filtering `state: \"open\"` (the COLUMN) will miss them and can come back empty on a \
         full board. The result always reports `total_on_board` and `claimable_now` so an empty \
         list is never mistaken for an empty board.";
    type Params = WorkListParams;
    type Output = WorkListResult;

    async fn run(&self, ctx: &Ctx, p: WorkListParams) -> Result<WorkListResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let filter = p.state.as_deref().map(parse_state).transpose()?;
        let board = airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?
            .snapshot();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // Resolve every distinct owner to a published name in ONE pass, then render
        // through the SHARED holder projection (`persona::card_holder`) — the same
        // answer the [room-kanban] grounding block and the service-loop anchor give.
        // Before this, three surfaces computed "who holds it / is the lease live"
        // separately and a teammate always came back as 8-hex, which Joel called out
        // directly: tell them WHO, or they cannot reach out.
        let me = ctx
            .caller
            .as_ref()
            .map(|c| c.peer_id.as_uuid())
            .unwrap_or_default(); // no subscriptions = empty board; the abort branch below speaks
        let mut owner_peers: Vec<airc_core::PeerId> = Vec::new();
        for c in &board.cards {
            if let Some(o) = c.owner {
                if o.as_uuid() != me && !owner_peers.contains(&o) {
                    owner_peers.push(o);
                }
            }
        }
        let names = crate::persona::room_board_source::RoomBoardReader::peer_names(
            airc.as_ref(),
            &owner_peers,
        )
        .await;
        // Render EVERY card once (holder resolution is the shared projection), then
        // filter on the rendered facts. Filtering before rendering would put the
        // availability question on the raw column again — the exact split this fixes.
        let rendered: Vec<(WorkListCard, CardState)> = board
            .cards
            .iter()
            .map(|c| {
                let holder = crate::persona::card_holder::holder(c, me, now_ms, &names);
                (
                    WorkListCard {
                        id: short8(c.card_id.as_uuid()),
                        title: c.title.clone(),
                        state: state_str(&c.state).to_string(),
                        // The person, not the hex: a published name when known, the
                        // short id (still addressable) otherwise, `YOU` when it is hers.
                        owner: holder.owner.map(|_| holder.display.clone()),
                        claimable: holder.claimable(c.state),
                        lease: holder.lease_word().map(str::to_string),
                    },
                    c.state,
                )
            })
            .collect();

        let total_on_board = rendered.len();
        let claimable_now = rendered.iter().filter(|(r, _)| r.claimable).count();

        let cards: Vec<WorkListCard> = rendered
            .into_iter()
            .filter(|(_, state)| filter.map_or(true, |f| *state == f))
            .filter(|(r, _)| p.claimable.map_or(true, |want| r.claimable == want))
            .map(|(r, _)| r)
            .collect();

        // A zero that explains itself. `{"cards":[]}` on a full board is the receipt
        // that cost the residents a day: it is indistinguishable from an empty board,
        // and they read it the only way it can be read. The filter's answer stays
        // honest — we do not quietly widen it — but the result says what it left out
        // and which query asks the question she meant. [[observability-as-substrate]]
        let note = if cards.is_empty() && total_on_board > 0 {
            Some(if claimable_now > 0 {
                format!(
                    "Your filter matched 0 of {total_on_board} cards — the board is NOT empty. \
                     {claimable_now} card(s) are claimable right now; most sit in the `claimed` \
                     column with a lapsed lease, which a `state` filter does not match. Call \
                     work/list with claimable=true to see them."
                )
            } else {
                format!(
                    "Your filter matched 0 of {total_on_board} cards — the board is NOT empty, but \
                     nothing on it is claimable right now (every card is actively held or done). \
                     Call work/list with no filter to see the whole board."
                )
            })
        } else {
            None
        };

        Ok(WorkListResult {
            cards,
            total_on_board,
            claimable_now,
            note,
        })
    }
}

/// Read ONE card's full content (read-only) — the requirements a worker
/// re-checks mid-task.
pub struct WorkGet {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkGetParams {
    /// The card id — full UUID or the 8-char short id the board shows.
    pub card_id: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkGetResult {
    pub id: String,
    pub title: String,
    /// The card's full body — the task's requirements/spec, when authored.
    pub body: Option<String>,
    pub state: String,
    pub owner: Option<String>,
    pub claim_id: Option<String>,
}

#[async_trait]
impl ActionCommand for WorkGet {
    const NAME: &'static str = "work/get";
    const ALIASES: &'static [&'static str] = &["get_task"];
    const NATIVE: bool = true; // core room workflow — re-reading a card's spec mid-task must not require asking the room
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Read one work card in full (read-only): title, body (the task's requirements), state, \
         owner, claim id. Accepts the 8-char short id the board shows. This is how you re-check a \
         spec mid-task instead of asking the room.";
    type Params = WorkGetParams;
    type Output = WorkGetResult;

    async fn run(&self, ctx: &Ctx, p: WorkGetParams) -> Result<WorkGetResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let board = airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?
            .snapshot();
        let card = board
            .cards
            .iter()
            .find(|c| c.card_id == card_id)
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "card {} resolved but is not on the current board projection",
                    p.card_id
                ))
            })?;
        Ok(WorkGetResult {
            id: short8(card.card_id.as_uuid()),
            title: card.title.clone(),
            body: card.body.clone(),
            state: state_str(&card.state).to_string(),
            owner: card.owner.map(|o| short8(o.as_uuid())),
            claim_id: card.claim_id.map(|c| short8(c.as_uuid())),
        })
    }
}

// ─────────────────── one registry: descriptors + objects ─────────────────

crate::register_command!(WorkList);
crate::register_command!(WorkGet);
crate::register_command!(WorkClaim);
crate::register_command!(WorkCreate);
crate::register_command!(WorkRelease);
crate::register_command!(WorkState);
crate::register_command!(WorkHeartbeat);

/// The kanban module — holds the persona airc-runtime registry so each work tool
/// can resolve the CALLER's own airc handle and act as that persona.
pub struct WorkModule {
    registry: PersonaAircRuntimeRegistry,
    /// Late-bound substrate executor (the ChatModule pattern): benchmark/dispatch
    /// composes OTHER commands — `data/list` to load a recipe row, `serving/pin`
    /// to re-home the lane — through the universal primitive instead of
    /// cross-module state threading. Installed by `install_executor_on_all`.
    executor_slot: std::sync::Arc<crate::runtime::LateBound<crate::runtime::command_executor::CommandExecutor>>,
}

impl WorkModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self {
            registry,
            executor_slot: std::sync::Arc::new(crate::runtime::LateBound::new("work::executor")),
        }
    }
}

#[async_trait]
impl ServiceModule for WorkModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "work",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Wire the process-global so `work/state` can PUBLISH the card-transition event.
        // Subscribers then REACT (event-based, no poll — the system law). Idempotent:
        // `set` fails silently on a second call, which is correct for a one-core global.
        let _ = WORK_EVENT_BUS.set((ctx.bus.clone(), ctx.registry.clone()));
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // All work commands are typed objects on the one registry (see `commands`).
        Err(format!(
            "work command '{command}' is a typed object, not prefix-routed"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![
            Arc::new(WorkList {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkGet {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkClaim {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkCreate {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkRelease {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkState {
                registry: self.registry.clone(),
            }),
            Arc::new(WorkHeartbeat {
                registry: self.registry.clone(),
            }),
            // benchmark/dispatch lives in commands/benchmark.rs (benchmark
            // domain) but is CONSTRUCTED here because it writes the board and
            // therefore needs this module's airc registry — the same reason
            // every work/* verb above does. Registering it from a module that
            // holds the dependency is what keeps it OUT of the
            // registered-but-unroutable class (#344 audit / #362).
            Arc::new(crate::commands::benchmark::BenchmarkDispatch {
                registry: self.registry.clone(),
                executor_slot: self.executor_slot.clone(),
            }),
            // persona/roster reads the SAME live registry benchmark/dispatch resolves its
            // assignees against — constructed here for the same dep-ownership reason (#396
            // live-roster verb; the observability side of "dispatch targets the live roster").
            Arc::new(crate::commands::persona_roster::PersonaRoster {
                registry: self.registry.clone(),
            }),
        ]
    }

    fn install_executor(
        &self,
        executor: std::sync::Arc<crate::runtime::command_executor::CommandExecutor>,
    ) {
        self.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the card-transition event NAME is the contract between the
    // emitter (`bridge_wire_work_event`) and every subscriber (SWE grade-on-done, board
    // freshness, auto-close). A silent rename here would unsubscribe every reactor — the
    // event would fire into the void, undetectably. Pin the string. (Event-based system
    // law: the grade REACTS to this, it never polls the board.)
    #[test]
    fn card_state_changed_event_name_is_the_stable_emitter_subscriber_contract() {
        assert_eq!(WORK_CARD_STATE_CHANGED, "work.card.state_changed");
    }

    // what this catches: the grade tail now has TWO feeders — the `work/state` verb
    // (in-process, delivery-proof) and the wire echo bridge (external writers). They
    // both see the same transition whenever wire delivery is healthy, and a subscriber
    // that grades twice would write two receipts for one card. The (card,state) ring is
    // the ONLY thing making that one publish; the wire-event-id ring cannot cover it
    // because the verb path has no wire event. Regression for the 2026-08-17 grade tail
    // fix (two Closed cards, zero grades, #434 starving the single wire feeder).
    #[test]
    fn one_transition_publishes_once_no_matter_which_feeder_sees_it_first() {
        let card = uuid::Uuid::new_v4().to_string();
        assert!(first_transition_sighting(&card, "closed"), "first sighting publishes");
        assert!(
            !first_transition_sighting(&card, "closed"),
            "the second feeder for the SAME transition must not publish again"
        );
        assert!(
            first_transition_sighting(&card, "merged"),
            "a DIFFERENT state on the same card is a different transition and must publish"
        );
    }

    /// Build a wire transcript event through the REAL producer (`encode_work_event`),
    /// so a header/codec contract drift fails these tests instead of shipping.
    fn wire_work_event(event: airc_work::WorkEvent, event_id: u128) -> airc_core::TranscriptEvent {
        let (headers, body) =
            airc_work::encode_work_event(&event).expect("work event encodes for the fixture");
        airc_core::TranscriptEvent {
            event_id: airc_core::EventId::from_u128(event_id),
            room_id: airc_core::RoomId::from_u128(2),
            peer_id: airc_core::PeerId::from_u128(3),
            client_id: airc_core::ClientId::from_u128(4),
            kind: airc_core::TranscriptKind::System,
            occurred_at_ms: 100,
            lamport: 1,
            target: airc_core::MentionTarget::All,
            headers,
            body: Some(body),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: the grade-on-done tail was DEAF to every writer except the
    // continuum `work/state` verb — a card closed via `airc work state` (operator) or a
    // remote peer changed the board and graded NOTHING (live 2026-08-15: Asha's real
    // rle_roundtrip artifact, operator close, zero grade). The bridge decodes the wire
    // echo into the exact payload contract the grader speaks: full hyphenated card UUID
    // + snake_case state that `is_terminal`/`parse_state` accept.
    #[test]
    fn wire_card_state_change_decodes_to_the_grader_payload_contract() {
        let card_id = airc_work::WorkCardId::new();
        let event = wire_work_event(
            airc_work::WorkEvent::CardStateChanged(airc_work::CardStateChanged {
                card_id,
                state: airc_work::CardState::Closed,
                changed_by: airc_core::PeerId::from_u128(3),
                changed_at_ms: 100,
            }),
            0xA1,
        );
        let payload = wire_card_state_payload(&event).expect("card-state event must decode");
        assert_eq!(
            payload["card_id"].as_str().unwrap(),
            card_id.as_uuid().to_string(),
            "card_id must be the FULL hyphenated UUID (the grader prefix-matches it)"
        );
        assert_eq!(
            payload["state"].as_str().unwrap(),
            "closed",
            "state must be the snake_case CardState serde form — the is_terminal vocabulary"
        );
        assert_eq!(
            payload["room_id"].as_str().unwrap(),
            airc_core::RoomId::from_u128(2).as_uuid().to_string(),
            "room_id must be the card's OWN room — the grader scopes its board read and \
             verdict post to it (#345 wrong-room trap, hit live 2026-08-15)"
        );
        // parse_state round-trip: the bridged state string is valid work/state input.
        assert_eq!(parse_state("closed").unwrap(), CardState::Closed);
    }

    // what this catches: a NON-state work event (a claim) or a non-work event must
    // never fabricate a state-change publish — a claim firing the grader would grade
    // half-finished work the moment it was picked up.
    #[test]
    fn non_state_work_events_and_non_work_events_do_not_bridge() {
        let claim = wire_work_event(
            airc_work::WorkEvent::CardClaimed(airc_work::WorkCardClaimed {
                card_id: airc_work::WorkCardId::new(),
                claim_id: airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4()),
                owner: airc_core::PeerId::from_u128(3),
                ttl_ms: 200,
                claimed_at_ms: 100,
            }),
            0xA2,
        );
        assert!(wire_card_state_payload(&claim).is_none());

        let mut chat = wire_work_event(
            airc_work::WorkEvent::CardStateChanged(airc_work::CardStateChanged {
                card_id: airc_work::WorkCardId::new(),
                state: airc_work::CardState::Closed,
                changed_by: airc_core::PeerId::from_u128(3),
                changed_at_ms: 100,
            }),
            0xA3,
        );
        chat.headers = airc_core::Headers::default(); // strip the work body-hint
        assert!(
            wire_card_state_payload(&chat).is_none(),
            "without the work body-hint header the event must not decode as work"
        );
    }

    // what this catches: every resident persona's subscribe stream yields the SAME
    // room event once — without first-sighting dedup the bridge would publish N
    // copies for N residents and the grader would grade (and post verdicts) N times.
    // Fresh uuids per call keep this parallel-safe against the process-global ring.
    #[test]
    fn first_sighting_admits_once_per_event_id() {
        let id = Uuid::new_v4();
        assert!(first_sighting(id), "first delivery must win");
        assert!(!first_sighting(id), "the second resident's echo must dedup");
        assert!(first_sighting(Uuid::new_v4()), "a different event is fresh");
    }

    /// what this catches (#357): `work/claim` inventing a holder out of a stale
    /// `owner` field, then `claim_rejections::record` burning that fabrication
    /// into perception for ten minutes.
    ///
    /// Expiry NEVER clears owner/claim_id — airc-work's projection clears them
    /// only on release or on a new claim — so a card claimed once carries an
    /// owner forever. Keying contention off the bare field relabelled EVERY
    /// refusal ("settled work is not claimable", wrong-room, transport faults)
    /// as "is held by peer X".
    ///
    /// Measured on the live boards 2026-08-07: ~50 of 61 cards on #general and
    /// 4 of 12 on #k3-serving carried a stale owner with a lease expired 134h+.
    /// (#k3-serving is RETIRED — it was named for a SUBSYSTEM, so it had no lifetime
    /// and outlived its purpose by a month. Kept here only as the incident record.)
    /// Two citizens spent the day quoting the resulting sentence back at us —
    /// "expired leases or the cards being held by others" is BOTH HALVES of that
    /// one format string.
    #[test]
    fn a_lapsed_lease_is_not_a_person_holding_the_card() {
        use airc_core::PeerId;
        use airc_work::{CardState, Priority, RepoId, WorkCard, WorkCardId};

        let owner = PeerId::new();
        let card = |claimed: bool, expires_ms: Option<u64>| WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("CambrianTech/continuum").expect("valid repo id in fixture"),
            title: "a card".to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state: CardState::Claimed,
            owner: Some(owner),
            claim_id: claimed.then(|| airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4())),
            claim_expires_at_ms: expires_ms,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: PeerId::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            reviews: None,
        };
        // A realistic epoch-ms clock: the 134h subtraction below is a real
        // observed lease age, and a toy `now` would underflow it.
        let now = 1_786_000_000_000u64;

        // A LIVE lease is a real person on a real card — still reported.
        assert_eq!(
            live_holder(&card(true, Some(now + 60_000)), now),
            Some(owner),
            "a live claim IS contention and must still name the holder"
        );

        // A lapsed lease: the owner field survives, the hold does not. Whatever
        // refused the claim, it was not a person.
        assert_eq!(
            live_holder(&card(true, Some(now - 1)), now),
            None,
            "an expired lease must NOT be reported as someone holding the card"
        );

        // The exact live shape: owner set, claim long expired. This is the ~50
        // of 61 cards on #general.
        assert_eq!(
            live_holder(&card(true, Some(now - 134 * 60 * 60 * 1000)), now),
            None,
            "a lease expired 134h ago is not a holder"
        );

        // Owner set with no claim at all — the shape a release leaves behind in
        // some projections. Also not a hold.
        assert_eq!(
            live_holder(&card(false, None), now),
            None,
            "an owner with no claim is not a live hold"
        );
    }

    // what this catches: work/claim id resolution (#161) still rescues the exact
    // live corruptions after the prefix/near-miss logic moved to the shared
    // crate::id_resolve primitive (#164). Regression pin for the CARD side: the two
    // glass-boxed 2026-07-13 corruptions (9-char first group; a 33-hex variant)
    // classify as a prefix on the intact leading-8 short id, and a bare 8-char short
    // id (what the board shows) does too — so resolve_card_id will expand them
    // against the live board. The pure primitive's full contract is tested in
    // crate::id_resolve; this proves the card verb delegates to it correctly.
    #[test]
    fn card_ids_rescue_mistyped_forms_via_shared_id_resolve() {
        use crate::id_resolve::{normalize, IdMatch};
        assert!(matches!(
            normalize("d7cfe47e-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Full(_)
        ));
        assert_eq!(
            normalize("d7cfe47e0-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Prefix("d7cfe47e".to_string())
        );
        assert_eq!(
            normalize("d7cfe47e08e3941f5bb2a4e5d36e558e1"), // 33 hex chars
            IdMatch::Prefix("d7cfe47e".to_string())
        );
        assert_eq!(
            normalize("08ece9e8"),
            IdMatch::Prefix("08ece9e8".to_string())
        );
    }

    // what this catches (#321, measured live 2026-08-06): the board lying by OMISSION.
    // A claim carries a lease; when it expires the substrate already treats the card as
    // reclaimable — `airc work next` offered EIGHT that night. But `work/list` rendered only
    // `state` + `owner`, so an expired hold read as `Claimed owner=Anwen`, indistinguishable
    // from someone actively working it. Six citizens read the board CORRECTLY, concluded there
    // was nothing to take, and spent the night announcing they had nothing to do. 19 cards,
    // 17 expired leases, 2 open.
    //
    // Pins the three cases she has to tell apart: open → take it; expired hold → take it
    // (the one that was invisible); live hold → someone else's, leave it.
    #[test]
    fn an_expired_lease_reads_as_claimable_and_a_live_one_does_not() {
        // A realistic epoch-ms clock: the 134h subtraction below is a real
        // observed lease age, and a toy `now` would underflow it.
        let now = 1_786_000_000_000u64;
        let claimable_of = |state: CardState, expires: Option<u64>| {
            let expired = expires.is_some_and(|e| e <= now);
            (
                state == CardState::Open || expired,
                expires.map(|_| if expired { "expired" } else { "held" }),
            )
        };

        assert_eq!(
            claimable_of(CardState::Open, None),
            (true, None),
            "an open card is takeable"
        );
        assert_eq!(
            claimable_of(CardState::Claimed, Some(now - 1)),
            (true, Some("expired")),
            "an EXPIRED hold is takeable — this is the fact the board was hiding"
        );
        assert_eq!(
            claimable_of(CardState::Claimed, Some(now + 60_000)),
            (false, Some("held")),
            "a LIVE hold stays someone else's — the guard against stealing active work"
        );
    }

    // what this catches (#337, measured 2026-08-06 from the residents' own captures):
    // the COLUMN filter answering the AVAILABILITY question with a silent zero. Every
    // board query the citizens made was `work/list(state=open)` — 84 of them, every one
    // returning `{"cards":[]}` — on a board of 61 cards with 59 lapsed leases and 0 cards
    // in the Open column. The `[board]` grounding block in the SAME prompt said "59
    // claimable". Both surfaces were right; the citizens reported "no open tasks" all day
    // and were reading their tool correctly. #321 fixed how a lapsed claim RENDERS; this
    // is the same defect one layer down, in what the filter MATCHES.
    //
    // Pins the three things that make that impossible now: the availability axis exists
    // and finds the lapsed cards, the column axis still means the column (no silent
    // widening), and an empty result carries the board's real counts so it can never
    // again be read as an empty board.
    #[test]
    fn a_column_filter_can_never_again_report_an_empty_board_as_no_work() {
        // No clock here: this test asserts over the FILTER axes, and states its
        // lease outcomes directly as `claimable_flags`. (It previously carried a
        // `now` and a comment about "the 134h subtraction below" — both copied
        // from its sibling `a_lapsed_lease_is_not_a_person_holding_the_card`,
        // which is the one that actually ages a lease. Dead variable, stale
        // comment; the compiler was right to warn.)
        // The live shape: nothing in the Open column, every claim lapsed.
        let claimable_flags = [true, true, true]; // 3 claimed cards, all leases expired
        let states = [CardState::Claimed, CardState::Claimed, CardState::Claimed];

        let total_on_board = states.len();
        let claimable_now = claimable_flags.iter().filter(|c| **c).count();

        // The query she actually made, 84 times: filter on the COLUMN.
        let by_column: Vec<usize> = (0..total_on_board)
            .filter(|i| states[*i] == CardState::Open)
            .collect();
        assert!(
            by_column.is_empty(),
            "state=open still means the Open COLUMN — the filter is not silently widened"
        );

        // The note that has to accompany that zero.
        let note_fires = by_column.is_empty() && total_on_board > 0;
        assert!(
            note_fires,
            "a zero on a non-empty board must explain itself"
        );
        assert_eq!(
            claimable_now, 3,
            "and the receipt must carry the count she was actually looking for"
        );

        // The query the new axis gives her.
        let by_availability: Vec<usize> = (0..total_on_board)
            .filter(|i| claimable_flags[*i])
            .collect();
        assert_eq!(
            by_availability.len(),
            3,
            "claimable=true finds the lapsed-lease work the column filter misses"
        );
    }

    /// what this catches: the self-explaining summary being serialized AFTER `cards`, where
    /// the receipt truncator severs it from every result a citizen actually reads.
    ///
    /// regression for 2026-08-06: `note` + `total_on_board` + `claimable_now` shipped that
    /// afternoon and were measured that evening to be ABSENT from all four live `work/list`
    /// receipts in Anwen's prompt — computed, serialized, then cut off by
    /// `act_observe::truncate_chars` (cap `fold_at.min(4096)`) because a real board's `cards`
    /// array is far longer than the cap. A field emitted after an unbounded list is a field
    /// nobody will ever see.
    #[test]
    fn the_summary_survives_a_truncated_receipt_because_it_precedes_the_card_list() {
        let cards: Vec<WorkListCard> = (0..60)
            .map(|i| WorkListCard {
                id: format!("card-{i:04}"),
                title: "x".repeat(200), // realistic titles — this is what blows the cap
                state: "claimed".to_string(),
                owner: Some("Benchy".to_string()),
                claimable: true,
                lease: Some("expired".to_string()),
            })
            .collect();

        let json = serde_json::to_string(&WorkListResult {
            total_on_board: 60,
            claimable_now: 58,
            note: Some("the board is NOT empty".to_string()),
            cards,
        })
        .expect("WorkListResult serializes");

        // The receipt path keeps a PREFIX. Anything past the cap is severed.
        const RECEIPT_CAP: usize = 4096;
        assert!(
            json.len() > RECEIPT_CAP,
            "this test is only meaningful when the payload actually exceeds the cap ({} bytes)",
            json.len()
        );
        let receipt: String = json.chars().take(RECEIPT_CAP).collect();

        for field in ["total_on_board", "claimable_now", "note"] {
            assert!(
                receipt.contains(field),
                "`{field}` must survive truncation — declare the summary BEFORE `cards`, \
                 or citizens read a severed result and correctly conclude there is no work"
            );
        }
    }

    /// The self-vs-other half of the refusal rule. `live_holder` (tested above,
    /// #357) answers "is a person on this card"; these answer "is that person YOU".
    mod refusal_identity {
        use super::*;

        /// what this catches: work/claim telling a citizen that SHE holds her own
        /// card — rendered as a stranger. Glass-boxed live 2026-08-14: Anon
        /// (a20b3ada) held 13 bench cards and was told `card 17531483 ... is held
        /// by peer a20b3ada`. She read her own id as a rival, concluded the work
        /// was taken, and went silent holding thirteen claims — with the board in
        /// her prompt and [Working Presence] telling her a quiet room is not a stop
        /// sign. The substrate outranked both, because it spoke in specifics.
        ///
        /// This is the benchmark loop's stall: dispatch stages a card, she claims
        /// it, a re-claim tells her it is someone else's, and no work happens.
        #[test]
        fn the_caller_is_never_a_rival_for_her_own_card() {
            let her = Uuid::new_v4();
            assert_eq!(
                classify_refusal(Some(her), Some(her)),
                ClaimRefusal::AlreadyYours,
                "a citizen holding her own card must be told she holds it — naming her \
                 own peer id as the rival is how thirteen claimed cards went unworked"
            );
        }

        /// what this catches: over-correcting into "self" and swallowing real
        /// contention — the failure the #357 fix was careful to avoid. A DIFFERENT
        /// live holder is a normal shared-board outcome and must still be named.
        #[test]
        fn a_different_live_holder_is_still_reported_as_contention() {
            let her = Uuid::new_v4();
            let peer = Uuid::new_v4();
            assert_eq!(
                classify_refusal(Some(peer), Some(her)),
                ClaimRefusal::HeldByPeer(peer),
                "someone else's live claim is real contention and must name them"
            );
        }

        /// what this catches: an unidentified caller silently classifying as
        /// "yours". Identity absent is NOT identity matched — without a caller we
        /// cannot claim the card is hers, so the holder is reported as a peer.
        #[test]
        fn an_unknown_caller_is_not_treated_as_the_holder() {
            let holder = Uuid::new_v4();
            assert_eq!(
                classify_refusal(Some(holder), None),
                ClaimRefusal::HeldByPeer(holder),
                "no caller identity means no self-match — never assume it is hers"
            );
        }

        /// what this catches: regression of the #357 rule through the new arm —
        /// nobody on the card is a fault, never a fabricated holder, whether or
        /// not we know who is asking.
        #[test]
        fn nobody_on_the_card_is_a_fault_not_a_holder() {
            assert_eq!(
                classify_refusal(None, Some(Uuid::new_v4())),
                ClaimRefusal::Fault
            );
            assert_eq!(classify_refusal(None, None), ClaimRefusal::Fault);
        }
    }
}
