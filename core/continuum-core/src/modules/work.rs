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
    Airc, ClaimId, ClaimWorkCard, CreateWorkCard, Priority, ReleaseWorkClaim, RepoId, WorkCardId,
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
        .map(|c| c.peer_id)
        .ok_or_else(|| CommandError::Denied("work commands require a persona caller".into()))?;
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

fn parse_card_id(s: &str) -> Result<WorkCardId, CommandError> {
    Uuid::parse_str(s)
        .map(WorkCardId::from_uuid)
        .map_err(|e| CommandError::Invalid(format!("invalid card_id '{s}': {e}")))
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
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Claim a work card on the shared airc board as yourself, so others see you own it. \
         Pass the card_id from the board. Returns a claim_id.";
    type Params = WorkClaimParams;
    type Output = WorkClaimResult;

    async fn run(&self, ctx: &Ctx, p: WorkClaimParams) -> Result<WorkClaimResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx)?;
        let card_id = parse_card_id(&p.card_id)?;
        let claim_id = airc
            .claim_work_card(ClaimWorkCard {
                card_id,
                ttl_ms: p.ttl_ms.unwrap_or(DEFAULT_CLAIM_TTL_MS),
            })
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
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
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Release your claim on a work card (pass card_id + the claim_id from work/claim).";
    type Params = WorkReleaseParams;
    type Output = WorkReleaseResult;

    async fn run(&self, ctx: &Ctx, p: WorkReleaseParams) -> Result<WorkReleaseResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx)?;
        let card_id = parse_card_id(&p.card_id)?;
        let claim_id = ClaimId::from_uuid(
            Uuid::parse_str(&p.claim_id)
                .map_err(|e| CommandError::Invalid(format!("invalid claim_id: {e}")))?,
        );
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

// ─────────────────── one registry: descriptors + objects ─────────────────

crate::register_command!(WorkClaim);
crate::register_command!(WorkCreate);
crate::register_command!(WorkRelease);

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
            Arc::new(WorkClaim { registry: self.registry.clone() }),
            Arc::new(WorkCreate { registry: self.registry.clone() }),
            Arc::new(WorkRelease { registry: self.registry.clone() }),
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
