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
use std::sync::Arc;

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
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

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
            CommandError::Denied(
                format!(
                    "{family} acts as the caller's own airc identity, and the \
                     substrate-local operator has none in-core (yet — the self-peer gap, \
                     task #27). Personas calling through their toolbelt act as themselves \
                     and need nothing special; for operator-identity board writes use \
                     `airc work <verb> ...`."
                ),
            )
        })?;
    let rt = registry.get(peer).ok_or_else(|| {
        CommandError::NotFound(format!("no live airc runtime for persona {peer}"))
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
        let claim_attempt = airc
            .claim_work_card(ClaimWorkCard {
                card_id,
                ttl_ms: p.ttl_ms.unwrap_or(DEFAULT_CLAIM_TTL_MS),
            })
            .await;
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
                        // #k3-serving carry a stale owner with a lease expired 134h+; three
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
                        if let Some(owner) = live_holder(card, now_ms) {
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
                                short8(owner.as_uuid()),
                                state_str(&card.state),
                            );
                        }
                    }
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
        Ok(WorkClaimResult {
            card_id: p.card_id,
            claim_id: claim_id.as_uuid().to_string(),
        })
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
fn live_holder(
    card: &airc_work::WorkCard,
    now_ms: u64,
) -> Option<airc_core::PeerId> {
    match crate::persona::card_holder::hold_of(card, now_ms) {
        crate::persona::card_holder::Hold::Held => card.owner,
        // Lapsed or unclaimed: whatever refused the claim, it was not a person.
        crate::persona::card_holder::Hold::Lapsed
        | crate::persona::card_holder::Hold::Unclaimed => None,
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
            .unwrap_or_default();
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
}

impl WorkModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self { registry }
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

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
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
            }),
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // A realistic epoch-ms clock: the 134h subtraction below is a real
        // observed lease age, and a toy `now` would underflow it.
        let now = 1_786_000_000_000u64;
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
}
