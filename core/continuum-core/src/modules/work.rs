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
    Airc, CardState, ChangeWorkCardState, ClaimId, ClaimWorkCard, CreateWorkCard, HeartbeatWorkClaim,
    Priority, ReleaseWorkClaim, RepoId, WorkCardId,
};

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// Default claim lease (ms) — 30 min — when the caller doesn't set one. The claim
/// is heartbeat-extendable; this is just the initial TTL.
const DEFAULT_CLAIM_TTL_MS: u64 = 30 * 60 * 1000;

/// Resolve the CALLING persona's own airc handle so work ops act as ITS key.
/// The caller identity is the authenticated airc peer_id the gate already saw;
/// `None` (substrate-local owner) has no persona runtime → a typed refusal.
fn persona_airc(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
) -> Result<Arc<Airc>, CommandError> {
    let peer = ctx
        .caller
        .as_ref()
        .map(|c| c.peer_id.as_uuid())
        .ok_or_else(|| {
            CommandError::Denied(
                "work commands act as the caller's own airc identity, and the \
                 substrate-local operator has none in-core (yet — the self-peer gap, \
                 task #27). Until the core carries a machine-scope airc runtime, use \
                 `airc work <verb> ...` for operator-identity board writes; personas \
                 calling through their toolbelt act as themselves and need nothing \
                 special."
                    .into(),
            )
        })?;
    let rt = registry
        .get(peer)
        .ok_or_else(|| CommandError::NotFound(format!("no live airc runtime for persona {peer}")))?;
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
        let airc = persona_airc(&self.registry, ctx)?;
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
                if let Ok(board) = airc
                    .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                    .await
                {
                    let board = board.snapshot();
                    if let Some(card) = board.cards.iter().find(|c| c.card_id == card_id) {
                        if let Some(owner) = card.owner {
                            msg = format!(
                                "card {} (\"{}\") is held by peer {} [{}]. Coordinate with \
                                 them in the room, or pick an unclaimed card via list_tasks. \
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
                return Err(CommandError::Internal(msg));
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
        let airc = persona_airc(&self.registry, ctx)?;
        let repo = RepoId::new(p.repo)
            .map_err(|e| CommandError::Invalid(format!("invalid repo: {e:?}")))?;
        let mut req =
            CreateWorkCard::new(repo, p.title, parse_priority(p.priority.as_deref().unwrap_or("p2")));
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
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Release your claim on a work card (pass card_id + the claim_id from work/claim; \
         short 8-char ids from the board are accepted for both).";
    type Params = WorkReleaseParams;
    type Output = WorkReleaseResult;

    async fn run(&self, ctx: &Ctx, p: WorkReleaseParams) -> Result<WorkReleaseResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx)?;
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
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Move a work card through its lifecycle: in_progress when you start, review when a PR is up, \
         blocked if stuck, closed when done. States: open|claimed|in_progress|blocked|review|merged|closed.";
    type Params = WorkStateParams;
    type Output = WorkStateResult;

    async fn run(&self, ctx: &Ctx, p: WorkStateParams) -> Result<WorkStateResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx)?;
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
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Extend your claim lease on a card so it doesn't go stale during long work (pass card_id + \
         claim_id; short 8-char ids from the board are accepted for both).";
    type Params = WorkHeartbeatParams;
    type Output = WorkHeartbeatResult;

    async fn run(&self, ctx: &Ctx, p: WorkHeartbeatParams) -> Result<WorkHeartbeatResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx)?;
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
fn short8(u: Uuid) -> String {
    u.simple().to_string().chars().take(8).collect()
}

/// Browse the live work board (read-only).
pub struct WorkList {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkListParams {
    /// Optional state filter: open | claimed | in_progress | blocked | review | merged | closed.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkListCard {
    /// 8-char short id — quote this back to work/get / work/claim / work/state.
    pub id: String,
    pub title: String,
    pub state: String,
    /// Short id of the claiming peer, when claimed.
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkListResult {
    pub cards: Vec<WorkListCard>,
}

#[async_trait]
impl ActionCommand for WorkList {
    const NAME: &'static str = "work/list";
    const ALIASES: &'static [&'static str] = &["list_tasks"];
    const NATIVE: bool = true; // core room workflow — the read half of claim_task; without it the board is write-only
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List the work board's cards (read-only): short id, title, state, owner. Use the short id \
         with work/get for a card's full requirements, or work/claim to take it. Optionally filter \
         by state (open|claimed|in_progress|blocked|review|merged|closed).";
    type Params = WorkListParams;
    type Output = WorkListResult;

    async fn run(&self, ctx: &Ctx, p: WorkListParams) -> Result<WorkListResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx)?;
        let filter = p.state.as_deref().map(parse_state).transpose()?;
        let board = airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?
            .snapshot();
        let cards = board
            .cards
            .iter()
            .filter(|c| filter.map_or(true, |f| c.state == f))
            .map(|c| WorkListCard {
                id: short8(c.card_id.as_uuid()),
                title: c.title.clone(),
                state: state_str(&c.state).to_string(),
                owner: c.owner.map(|o| short8(o.as_uuid())),
            })
            .collect();
        Ok(WorkListResult { cards })
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
        let airc = persona_airc(&self.registry, ctx)?;
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
        Err(format!("work command '{command}' is a typed object, not prefix-routed"))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![
            Arc::new(WorkList { registry: self.registry.clone() }),
            Arc::new(WorkGet { registry: self.registry.clone() }),
            Arc::new(WorkClaim { registry: self.registry.clone() }),
            Arc::new(WorkCreate { registry: self.registry.clone() }),
            Arc::new(WorkRelease { registry: self.registry.clone() }),
            Arc::new(WorkState { registry: self.registry.clone() }),
            Arc::new(WorkHeartbeat { registry: self.registry.clone() }),
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(normalize("d7cfe47e-8e39-41f5-bb2a-4e5d36e558e1"), IdMatch::Full(_)));
        assert_eq!(
            normalize("d7cfe47e0-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Prefix("d7cfe47e".to_string())
        );
        assert_eq!(
            normalize("d7cfe47e08e3941f5bb2a4e5d36e558e1"), // 33 hex chars
            IdMatch::Prefix("d7cfe47e".to_string())
        );
        assert_eq!(normalize("08ece9e8"), IdMatch::Prefix("08ece9e8".to_string()));
    }
}
