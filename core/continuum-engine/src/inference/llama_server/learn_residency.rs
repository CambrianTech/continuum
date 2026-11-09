//! What a mind's learning occupies in the engine, leased by the core BEFORE it is allocated
//! (card 4114a2c6; Joel, 2026-11-08: "Everything must be under vram and ram budgeting").
//!
//! Learning is in place in the serving engine, so its memory is a small claim on the same budget:
//! the shadow's weights, its optimizer state, and one per-engine training workspace. The engine
//! measures each without allocating (`dry_run` on `/lora-adapters/fresh|load`, `/learn/preflight`),
//! the core leases exactly those bytes from its one resource authority, then registers a
//! RESIDENCY with the engine that mirrors those leases: the engine admits an allocation only inside
//! a registered residency's rows (fork, Codex at 7885c2932 / 08bc3ccb3). The engine holds no second
//! ledger, only this reference to the core's.

use super::learn_ledger::{LearningEngine, LedgerAnswer, LedgerEngine, LedgerReply, Registered};
use super::*;

/// A residency the core mints, never reused (a released id is retired by the engine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResidencyId(uuid::Uuid);

impl ResidencyId {
    pub fn issue() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl std::fmt::Display for ResidencyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What a residency covers. Every field of a fresh shadow is sent: the engine never fills one in.
#[derive(Debug, Clone, PartialEq)]
pub enum ResidencySubject {
    /// A new zero-B shadow the engine writes and loads (`/lora-adapters/fresh`).
    Fresh(FreshShadow),
    /// An existing adapter file under the engine's train dir (`/lora-adapters/load`).
    Load { name: String },
    /// The engine's one training workspace, shared by every mind's chunks.
    Workspace,
}

/// A fresh shadow's shape, all of it explicit.
#[derive(Debug, Clone, PartialEq)]
pub struct FreshShadow {
    pub name: String,
    pub rank: u32,
    pub alpha: f32,
    pub targets: Vec<String>,
    pub top_layers: u32,
    pub seed: u32,
}

impl FreshShadow {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name, "rank": self.rank, "alpha": self.alpha,
            "targets": self.targets.join(","), "top_layers": self.top_layers, "seed": self.seed,
        })
    }
}

impl ResidencySubject {
    fn json(&self) -> serde_json::Value {
        match self {
            ResidencySubject::Fresh(fresh) => serde_json::json!({ "fresh": fresh.json() }),
            ResidencySubject::Load { name } => serde_json::json!({ "load": { "name": name } }),
            ResidencySubject::Workspace => serde_json::json!({ "workspace": true }),
        }
    }
}

/// What one row of a residency holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Weights,
    OptimizerStaticRequired,
    OptimizerHostRequired,
    SharedComputeRequired,
}

/// One measured row: bytes on one device's buffer type. `host` marks memory the core leases as
/// RAM (pinned host buffers included); everything else is VRAM.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct MeasuredRow {
    pub device: String,
    pub buffer_type: String,
    pub bytes: u64,
    pub host: bool,
}

/// A measured row named for what it holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ResidencyRow {
    pub device: String,
    pub buffer_type: String,
    pub kind: RowKind,
    pub bytes: u64,
    pub host: bool,
}

impl ResidencyRow {
    pub fn of(row: MeasuredRow, kind: RowKind) -> Self {
        Self { device: row.device, buffer_type: row.buffer_type, kind, bytes: row.bytes, host: row.host }
    }
}

/// One of the core's leases covering a residency, by the resource authority's id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CoveringLease {
    pub kind: LeaseKind,
    pub lease_id: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseKind {
    Vram,
    Ram,
}

/// A residency to register: the measured rows and the core leases that cover them.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidencyTerms {
    pub id: ResidencyId,
    pub engine: LedgerEngine,
    /// The core process that holds the leases (one per process, so a restarted core's residency is
    /// never this one's).
    pub owner: uuid::Uuid,
    pub subject: ResidencySubject,
    pub rows: Vec<ResidencyRow>,
    pub leases: Vec<CoveringLease>,
}

/// Why the engine refused a residency request, by its own names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidencyRefusal {
    Invalid,
    StaleEngine,
    Retired,
    ResidencyMismatch,
    LeaseAlreadyBound,
    Uncovered,
    UnknownResidency,
    SubjectMismatch,
    Busy,
}

/// A residency as the engine holds it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct RegisteredResidency {
    pub id: String,
    pub engine: LedgerEngine,
    pub owner: String,
    pub subject: serde_json::Value,
    pub rows: Vec<ResidencyRow>,
    pub leases: Vec<CoveringLease>,
    pub pending: bool,
    pub live: bool,
    pub revoked: bool,
    pub lora_id: i64,
}

/// The engine's measurement of something not yet allocated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct WeightsPlan {
    pub engine: LedgerEngine,
    pub weights: Vec<MeasuredRow>,
}

/// The engine's measurement of a loaded shadow's learning: optimizer state and the workspace.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LearningPlan {
    pub engine: LedgerEngine,
    pub requirements: Vec<PlannedRow>,
}

/// A preflight row: `*_required` is what learning would allocate, `shared_compute_held` is what
/// serving holds now (evidence, never a lease row).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct PlannedRow {
    pub device: String,
    pub buffer_type: String,
    pub kind: String,
    pub bytes: u64,
    pub host: bool,
}

/// Why the engine would not measure learning, by its own names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightRefusal {
    None,
    StaleEngine,
    Busy,
    Unsupported,
    Invalid,
}

#[derive(serde::Deserialize)]
struct PreflightReply {
    engine: LedgerEngine,
    refusal: PreflightRefusal,
    #[serde(default)]
    detail: Option<String>,
    requirements: Vec<PlannedRow>,
}

/// A learning preflight's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preflight {
    Planned(LearningPlan),
    Refused { refusal: PreflightRefusal, detail: Option<String> },
}

impl LearningEngine {
    /// `dry_run` of a fresh shadow: the weights it would take, nothing written or allocated.
    pub async fn plan_fresh_shadow(&self, shadow: &FreshShadow) -> Result<LedgerAnswer<WeightsPlan>, LlamaServerError> {
        let mut body = shadow.json();
        body["dry_run"] = serde_json::Value::Bool(true);
        self.plan_request(self.client.post(format!("{}/lora-adapters/fresh", self.root)).json(&body)).await
    }

    /// `dry_run` of loading an adapter file: the weights it would take, nothing allocated.
    pub async fn plan_load(&self, name: &str) -> Result<LedgerAnswer<WeightsPlan>, LlamaServerError> {
        let body = serde_json::json!({ "name": name, "dry_run": true });
        self.plan_request(self.client.post(format!("{}/lora-adapters/load", self.root)).json(&body)).await
    }

    /// `POST /learn/preflight`: what learning on loaded shadow `lora_id` would allocate, measured on a
    /// disposable scheduler (serving untouched).
    pub async fn learning_preflight(&self, engine: &LedgerEngine, lora_id: i64) -> Result<LedgerAnswer<Preflight>, LlamaServerError> {
        let body = serde_json::json!({ "engine": engine, "lora_id": lora_id });
        let reply: LedgerAnswer<PreflightReply> =
            self.plan_request(self.client.post(format!("{}/learn/preflight", self.root)).json(&body)).await?;
        Ok(reply.map(|reply| match reply.refusal {
            PreflightRefusal::None => Preflight::Planned(LearningPlan { engine: reply.engine, requirements: reply.requirements }),
            refusal => Preflight::Refused { refusal, detail: reply.detail },
        }))
    }

    /// `POST /learn/residency`: mirror the core's leases for `terms` in the engine.
    pub async fn register_residency(&self, terms: &ResidencyTerms) -> Result<LedgerAnswer<Registered<ResidencyRefusal>>, LlamaServerError> {
        let body = serde_json::json!({
            "id": terms.id.to_string(),
            "engine": terms.engine,
            "owner": terms.owner.to_string(),
            "subject": terms.subject.json(),
            "rows": terms.rows,
            "leases": terms.leases,
        });
        let reply = self.ledger_request::<ResidencyRefusal>(self.client.post(format!("{}/learn/residency", self.root)).json(&body)).await?;
        Ok(reply.map(LedgerReply::registered))
    }

    /// `POST /learn/residency/release`: the engine forgets a residency whose allocations have left;
    /// refused (busy) while any is pending or live.
    pub async fn release_residency(&self, id: &str) -> Result<LedgerAnswer<Registered<ResidencyRefusal>>, LlamaServerError> {
        let body = serde_json::json!({ "id": id });
        let reply = self.ledger_request::<ResidencyRefusal>(self.client.post(format!("{}/learn/residency/release", self.root)).json(&body)).await?;
        Ok(reply.map(LedgerReply::registered))
    }

    async fn plan_request<T: serde::de::DeserializeOwned>(&self, request: reqwest::RequestBuilder) -> Result<LedgerAnswer<T>, LlamaServerError> {
        let resp = request
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        match resp.status().as_u16() {
            200..=299 => {}
            404 | 405 => return Ok(LedgerAnswer::NoLedger),
            status => {
                let text = resp.text().await.unwrap_or_default(); // unwrap_or_default: the status alone still names the failure
                return Err(LlamaServerError::Unreachable(format!("learning plan status {status}: {text}")));
            }
        }
        let bytes = resp.bytes().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        serde_json::from_slice(&bytes) // engine HTTP wire: a measured plan
            .map(LedgerAnswer::Ledger)
            .map_err(|e| LlamaServerError::Unreachable(format!("the learning plan answered no plan: {e}")))
    }
}
