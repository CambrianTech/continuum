//! The engine's learning ledger, read and written by the core's one issuer (card 4114a2c6).
//!
//! Learning is the served forward pass with backprop on, in place in the serving engine
//! (ONE-RESIDENT-MODEL §9; Joel, 2026-11-08: "Training = inference + backprop etc on"). The
//! engine admits a learning step only against a grant the core REGISTERED with it: an immutable
//! `{id, engine, period, share, expiry}` (fork #87 ledger, wired by Codex at 7051ffd95). A turn
//! carries only the grant's id, so no request authorizes its own share. This module is the wire:
//! `GET /learn/grants`, `POST /learn/grants`, `POST /learn/grants/revoke`, each answering the
//! whole ledger (identity, clock, live grants, measured optimizer memory). An engine without the
//! routes answers [`LedgerAnswer::NoLedger`]; a malformed answer is an error, never an empty
//! ledger.

use super::*;

/// The engine's ledger identity: an id it mints once at start, so a restarted engine is a new
/// ledger by construction and every grant registered with the old one is void.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct LedgerEngine(String);

/// One serving engine PROCESS's learning routes: separate from the owner of the engine's lifecycle,
/// so the learner in the genome reaches the ledger without it. Bound to the physical incarnation
/// (pid and OS start time), never to an endpoint alone: a relaunch on the same port is another
/// engine, and only that process's verified death proves its memory freed.
#[derive(Debug, Clone)]
pub struct LearningEngine {
    pub(super) root: String,
    pub(super) client: reqwest::Client,
    process: crate::inference::engine_residency::EngineIncarnation,
}

impl LearningEngine {
    /// The engine process `process` (its `root_url` is where it serves).
    pub fn of(process: crate::inference::engine_residency::EngineIncarnation) -> Self {
        Self { root: process.root_url(), client: reqwest::Client::new(), process }
    }

    /// A fake engine serving on `port` inside this (test) process, which is its incarnation. `None`
    /// when this process's start time cannot be read.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn serving_here(port: u16) -> Option<Self> {
        crate::inference::engine_residency::EngineIncarnation::of(std::process::id(), port).map(Self::of)
    }

    /// The physical process this engine is.
    pub fn process(&self) -> crate::inference::engine_residency::EngineIncarnation {
        self.process
    }
}

/// A grant the core issues: never reused, so a new share is a new id (and the old is revoked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GrantId(uuid::Uuid);

impl GrantId {
    pub fn issue() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl std::fmt::Display for GrantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// How much of the engine's time a grant lets learning take while another slot serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantShare {
    /// Learning only while no other slot serves; every contended step is refused. The bootstrap
    /// before anything has been measured (Codex, 2026-11-09: share 0, never an invented number).
    UncontendedOnly,
    /// Of every `period`, at most `share_ppm` millionths, each learning chunk at most `max_chunk`
    /// (the latency a neighbour may wait behind one chunk), all from measurement. A share without
    /// its chunk bound is not a measured grant, so it cannot be written.
    Measured { share_ppm: std::num::NonZeroU32, period: std::time::Duration, max_chunk: std::time::Duration },
}

/// What a grant allows, until `expires_in` (the engine turns it into its own clock, so no
/// translation can go wrong).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantTerms {
    pub id: GrantId,
    pub engine: LedgerEngine,
    pub share: GrantShare,
    pub expires_in: std::time::Duration,
}

/// Why the engine refused a ledger request, by the engine's own names (`learn_refusal_name`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRefusal {
    /// A contended step under an uncontended-only grant: measure first, then a measured grant.
    UncontendedOnly,
    /// A step under a grant with no measured chunk bound, or with no measured chunk cost yet.
    PendingCalibration,
    /// The measured chunk is longer than the grant's bound, even while idle.
    ChunkExceedsLatency,
    Invalid,
    StaleEngine,
    GrantMismatch,
    UnknownGrant,
    NeverFits,
    BudgetExhausted,
    Started,
    NotStarted,
    AlreadyStarted,
    UnknownReservation,
}

/// The wire's refusal field: `none`, or a refusal by the route family's own names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
enum WireRefusal<R> {
    None(NoRefusal),
    Refused(R),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum NoRefusal {
    None,
}

impl<R> WireRefusal<R> {
    fn refused(self) -> Option<R> {
        match self {
            WireRefusal::None(NoRefusal::None) => None,
            WireRefusal::Refused(why) => Some(why),
        }
    }
}

/// One grant live in the engine's ledger, by whatever issued it (an id this core never issued is
/// a previous core's, so it stays a string).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RegisteredGrant {
    pub id: String,
    pub period_ms: i64,
    pub share_ppm: i64,
    /// Zero for a grant without a measured chunk bound (uncontended-only).
    pub max_chunk_us: i64,
    pub expires_at_us: i64,
    pub revoked: bool,
    pub outstanding: i64,
    pub committed_steps: i64,
    /// The period's measured use, when the engine can read it.
    #[serde(default)]
    pub used_us: Option<i64>,
}

/// One optimizer-owned buffer, measured by the engine's allocator (never a ratio).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct OptimizerBuffer {
    pub lora_id: i64,
    pub device: String,
    pub buffer_type: String,
    pub kind: String,
    pub bytes: u64,
    pub host: bool,
}

/// The serving scheduler's compute buffers: the engine's CURRENT TOTAL, reported once, not a
/// training increment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ComputeBuffer {
    pub device: String,
    pub buffer_type: String,
    pub bytes: u64,
    pub host: bool,
}

/// What learning holds in the engine, as measured.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LedgerMemory {
    #[serde(rename = "optimizer_owned_buffers")]
    pub optimizer: Vec<OptimizerBuffer>,
    #[serde(rename = "shared_compute_buffers")]
    pub shared_compute: Vec<ComputeBuffer>,
    /// Loaded adapters with no optimizer: stable genes, and shadows that have not learned yet.
    #[serde(rename = "optimizer_unallocated_lora_ids")]
    pub unallocated_lora_ids: Vec<i64>,
}

/// The whole ledger, as every ledger route answers it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct LedgerSnapshot {
    pub engine: LedgerEngine,
    pub clock_us: i64,
    pub grants: Vec<RegisteredGrant>,
    pub memory: LedgerMemory,
    pub residencies: Vec<super::learn_residency::RegisteredResidency>,
    /// The longest period and expiry this engine's ledger accepts, as it reports them: the core
    /// sizes a grant within these, never against a copy of the engine's constant.
    pub max_period_ms: u64,
    pub max_expiry_ms: u64,
}

#[derive(serde::Deserialize)]
#[serde(bound = "R: serde::de::DeserializeOwned")]
pub(super) struct LedgerReply<R> {
    #[serde(flatten)]
    pub(super) snapshot: LedgerSnapshot,
    refusal: WireRefusal<R>,
}

impl<R> LedgerReply<R> {
    pub(super) fn registered(self) -> Registered<R> {
        Registered { refused: self.refusal.refused(), ledger: self.snapshot }
    }
}

/// An engine with the ledger, or one without the routes (older than fork #87's wiring).
#[derive(Debug, Clone, PartialEq)]
pub enum LedgerAnswer<T> {
    Ledger(T),
    NoLedger,
}

impl<T> LedgerAnswer<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> LedgerAnswer<U> {
        match self {
            LedgerAnswer::Ledger(t) => LedgerAnswer::Ledger(f(t)),
            LedgerAnswer::NoLedger => LedgerAnswer::NoLedger,
        }
    }
}

/// A ledger request's answer: accepted or refused by name, and the ledger as it stands after it
/// (the core reconciles from this snapshot, never from what it asked for).
#[derive(Debug, Clone, PartialEq)]
pub struct Registered<R> {
    pub refused: Option<R>,
    pub ledger: LedgerSnapshot,
}

impl LearningEngine {
    /// `GET /learn/grants`: the ledger as it stands.
    pub async fn learn_ledger(&self) -> Result<LedgerAnswer<LedgerSnapshot>, LlamaServerError> {
        let reply = self.ledger_request::<GrantRefusal>(self.client.get(format!("{}/learn/grants", self.root))).await?;
        Ok(reply.map(|reply| reply.snapshot))
    }

    /// `POST /learn/grants`: register `terms` with the engine.
    pub async fn register_learn_grant(&self, terms: &GrantTerms) -> Result<LedgerAnswer<Registered<GrantRefusal>>, LlamaServerError> {
        let (share_ppm, period_ms, max_chunk_us) = match terms.share {
            GrantShare::UncontendedOnly => (0, 0, 0),
            GrantShare::Measured { share_ppm, period, max_chunk } => (share_ppm.get(), duration_ms(period)?, duration_us(max_chunk)?),
        };
        let body = serde_json::json!({
            "id": terms.id.to_string(),
            "engine": terms.engine,
            "period_ms": period_ms,
            "share_ppm": share_ppm,
            "max_chunk_us": max_chunk_us,
            "expires_in_ms": duration_ms(terms.expires_in)?,
        });
        let reply = self.ledger_request::<GrantRefusal>(self.client.post(format!("{}/learn/grants", self.root)).json(&body)).await?;
        Ok(reply.map(LedgerReply::registered))
    }

    /// `POST /learn/grants/revoke`: no new work under `id` (idempotent); its account goes once its
    /// last reservation settles.
    pub async fn revoke_learn_grant(&self, id: &str) -> Result<LedgerAnswer<LedgerSnapshot>, LlamaServerError> {
        let body = serde_json::json!({ "id": id });
        let reply = self.ledger_request::<GrantRefusal>(self.client.post(format!("{}/learn/grants/revoke", self.root)).json(&body)).await?;
        Ok(reply.map(|reply| reply.snapshot))
    }

    /// One ledger route: the engine's answer parsed whole (identity, clock, grants, residencies,
    /// measured memory) with the route family's refusal names, NoLedger for an engine without it.
    pub(super) async fn ledger_request<R: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<LedgerAnswer<LedgerReply<R>>, LlamaServerError> {
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
                return Err(LlamaServerError::Unreachable(format!("learning ledger status {status}: {text}")));
            }
        }
        let bytes = resp.bytes().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        serde_json::from_slice(&bytes) // engine HTTP wire: the ledger reply
            .map(LedgerAnswer::Ledger)
            .map_err(|e| LlamaServerError::Unreachable(format!("the learning ledger answered no ledger: {e}")))
    }
}

/// A duration on the wire in whole milliseconds, refused past what the field holds.
fn duration_ms(d: std::time::Duration) -> Result<u64, LlamaServerError> {
    u64::try_from(d.as_millis()).map_err(|_| LlamaServerError::Unreachable(format!("{d:?} does not fit the ledger's milliseconds")))
}


fn duration_us(d: std::time::Duration) -> Result<u64, LlamaServerError> {
    u64::try_from(d.as_micros()).map_err(|_| LlamaServerError::Unreachable(format!("{d:?} does not fit the ledger's microseconds")))
}
