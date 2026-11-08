use super::*;

pub(crate) use crate::persona::operator_peer::{persona_airc, persona_runtime};

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
/// The reading is airc's [`Shown`], the one owner of an id as it is typed back (ONE-ACT
/// stage 6): a full UUID is exact, a prefix keeps every digit, a UUID one digit off reads
/// as its short form. Where a card id lives is the horizon's [`airc_core::shown_id::ShownScope`]: the cards of
/// her subscribed boards.
pub(super) async fn resolve_card_id(
    airc: &std::sync::Arc<airc_lib::Airc>,
    s: &str,
) -> Result<WorkCardId, CommandError> {
    // Fast path: a full id needs no board read.
    let shown = shown::<WorkCardId>(s)?;
    if let Some(id) = shown.full() {
        return Ok(id);
    }
    let horizon = board_horizon(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("board read for id resolution: {e}")))?;
    resolve_card_id_in_boards(&horizon, s)
}

/// Resolve against an already-read view, so reading a card need not fold the
/// subscribed boards again after expanding its short id.
pub(super) fn resolve_card_id_in_boards(horizon: &BoardHorizon, s: &str) -> Result<WorkCardId, CommandError> {
    shown::<WorkCardId>(s)?.resolve_in(horizon).map_err(|e| refused::<WorkCardId>(horizon, s, e))
}

/// A typed-back id that named no candidate. An empty scope is the horizon's story to tell
/// (outage, no rooms, or empty boards), not the resolver's "no cards exist".
fn refused<T: ShownKind>(horizon: &BoardHorizon, s: &str, e: ShownIdError) -> CommandError {
    match e {
        ShownIdError::NoMatch { total: 0, .. } => horizon.not_found(T::LABEL, s),
        e => CommandError::Invalid(e.to_string()),
    }
}

/// `s` as an id typed back at a boundary, refused in the id type's own words.
pub(super) fn shown<T: ShownKind>(s: &str) -> Result<Shown<T>, CommandError> {
    Shown::try_from(s).map_err(|e| CommandError::Invalid(e.to_string()))
}

/// Resolve a claim id the same way [`resolve_card_id`] resolves cards (#164:
/// short-form ids must resolve on EVERY id param, not just card_id). The board
/// projection and the [work] grounding facts render claim ids in short form, so
/// the lifecycle verbs (release/heartbeat) must accept the handle the persona
/// was SHOWN. Candidates are the horizon's live claims ([`airc_core::shown_id::ShownScope`]); the reading is
/// [`Shown`] — one resolution behavior for every id kind, no second copy of the rules.
pub(super) async fn resolve_claim_id(
    airc: &std::sync::Arc<airc_lib::Airc>,
    s: &str,
) -> Result<ClaimId, CommandError> {
    let shown = shown::<ClaimId>(s)?;
    if let Some(id) = shown.full() {
        return Ok(id);
    }
    let horizon = board_horizon(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("board read for claim resolution: {e}")))?;
    shown.resolve_in(&horizon).map_err(|e| match refused::<ClaimId>(&horizon, s, e) {
        CommandError::Invalid(why) => CommandError::Invalid(claim_gone_hint(why)),
        other => other,
    })
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
/// Locate `card_id`'s room, switch the caller's current room there, and retry the
/// claim once.
///
/// Returns `None` when no subscribed room holds the card, or when the card's room
/// IS the current one (the room that already refused has nothing to follow) — in
/// both cases the original wrong-room refusal stands, verbatim. `Some(result)`
/// means a room-switched claim was attempted and that result REPLACES the refusal,
/// so a genuine contention in the card's room still reports as contention.

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
    let claimed = airc.claim_work_card_with_origin(ClaimWorkCard { card_id, ttl_ms }, origin).await;
    if claimed.is_ok() {
        crate::persona::work_board::board_index::wrote_card(airc, card_id);
    }
    Some(claimed)
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
    /// Claim ownership stands even when provisioning fails. Retry work/claim
    /// after the reported substrate fault is repaired; shell cannot bind itself.
    pub workspace: super::ClaimWorkspace,
}
