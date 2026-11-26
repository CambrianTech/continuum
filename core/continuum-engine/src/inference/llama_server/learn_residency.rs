//! What a gene seated on a llama-server engine occupies, leased by the core BEFORE it is allocated
//! (card 4114a2c6; Joel, 2026-11-08: "Everything must be under vram and ram budgeting").
//!
//! The engine measures an adapter's weights without allocating (`dry_run` on
//! `/lora-adapters/load`), the core leases exactly those bytes from its one resource authority,
//! then registers a RESIDENCY with the engine that mirrors those leases: the engine admits an
//! allocation only inside a registered residency's rows (fork, Codex at 7885c2932 / 08bc3ccb3). The
//! engine holds no second ledger, only this reference to the core's.
//!
//! Learning no longer goes over this HTTP surface: a mind learns on the node's in-process engine
//! through `inference::learning` (IN-PROCESS-LEARNING.md L3, card 28aac827).

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

/// What a residency covers.
#[derive(Debug, Clone, PartialEq)]
pub enum ResidencySubject {
    /// An existing adapter file under the engine's train dir (`/lora-adapters/load`).
    Load { name: String },
}

impl ResidencySubject {
    fn json(&self) -> serde_json::Value {
        match self {
            ResidencySubject::Load { name } => serde_json::json!({ "load": { "name": name } }),
        }
    }
}

/// What one row of a residency holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Weights,
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

/// An adapter the engine holds: its id and the file it was loaded from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LoadedAdapter {
    pub id: i64,
    pub path: String,
}

/// The engine's measurement of something not yet allocated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct WeightsPlan {
    pub engine: LedgerEngine,
    pub weights: Vec<MeasuredRow>,
}

impl LearningEngine {
    /// `dry_run` of loading an adapter file: the weights it would take, nothing allocated.
    pub async fn plan_load(&self, name: &str) -> Result<LedgerAnswer<WeightsPlan>, LlamaServerError> {
        let body = serde_json::json!({ "name": name, "dry_run": true });
        self.plan_request(self.client.post(format!("{}/lora-adapters/load", self.root)).json(&body)).await
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
    pub async fn release_residency(&self, engine: &LedgerEngine, id: &str) -> Result<LedgerAnswer<Registered<ResidencyRefusal>>, LlamaServerError> {
        // the incarnation the residency was registered with: another one refuses stale_engine unchanged
        let body = serde_json::json!({ "engine": engine, "id": id });
        let reply = self.ledger_request::<ResidencyRefusal>(self.client.post(format!("{}/learn/residency/release", self.root)).json(&body)).await?;
        Ok(reply.map(LedgerReply::registered))
    }

    /// `POST /lora-adapters/load` inside `residency`: the engine claims the residency, checks its rows
    /// cover the adapter file `name` (under its train dir), and loads it. The loaded adapter.
    pub async fn seat_load(&self, name: &str, residency: ResidencyId) -> Result<LoadedAdapter, LlamaServerError> {
        let body = serde_json::json!({ "name": name, "residency": residency.to_string() });
        self.adapter_change(self.client.post(format!("{}/lora-adapters/load", self.root)).json(&body)).await
    }

    /// `POST /lora-adapters/unload`: the shadow leaves the engine (its optimizer released first).
    pub async fn unseat(&self, engine: &LedgerEngine, lora_id: i64) -> Result<LoadedAdapter, LlamaServerError> {
        // bound to the incarnation that loaded it: a relaunch reusing the id refuses, never unloads
        let body = serde_json::json!({ "engine": engine, "id": lora_id });
        self.adapter_change(self.client.post(format!("{}/lora-adapters/unload", self.root)).json(&body)).await
    }

    async fn adapter_change(&self, request: reqwest::RequestBuilder) -> Result<LoadedAdapter, LlamaServerError> {
        let resp = request.timeout(super::genome::LORA_LOAD_TIMEOUT).send().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if status.as_u16() == 503 {
            return Err(LlamaServerError::AdapterChange(AdapterChangeRefusal::Busy));
        }
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&bytes).into_owned();
            return Err(LlamaServerError::AdapterChange(AdapterChangeRefusal::Refused { status: status.as_u16(), detail }));
        }
        serde_json::from_slice::<LoadedAdapter>(&bytes) // engine HTTP wire: the adapter change reply
            .map_err(|e| LlamaServerError::Unreachable(format!("adapter change answered no id: {e}")))
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
