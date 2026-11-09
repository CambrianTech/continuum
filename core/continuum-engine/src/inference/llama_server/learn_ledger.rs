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

/// What a grant allows: of every `period`, at most `share_ppm` millionths of the engine's time
/// for learning while another slot serves, until `expires_in` (the engine turns it into its own
/// clock, so no translation can go wrong).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantTerms {
    pub id: GrantId,
    pub engine: LedgerEngine,
    pub period: std::time::Duration,
    pub share_ppm: std::num::NonZeroU32,
    pub expires_in: std::time::Duration,
}

/// Why the engine refused a ledger request, by the engine's own names (`learn_refusal_name`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRefusal {
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

/// The wire's refusal field: `none`, or a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
enum WireRefusal {
    None(NoRefusal),
    Refused(GrantRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum NoRefusal {
    None,
}

/// One grant live in the engine's ledger, by whatever issued it (an id this core never issued is
/// a previous core's, so it stays a string).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RegisteredGrant {
    pub id: String,
    pub period_ms: i64,
    pub share_ppm: i64,
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LedgerSnapshot {
    pub engine: LedgerEngine,
    pub clock_us: i64,
    pub grants: Vec<RegisteredGrant>,
    pub memory: LedgerMemory,
}

#[derive(serde::Deserialize)]
struct LedgerReply {
    #[serde(flatten)]
    snapshot: LedgerSnapshot,
    refusal: WireRefusal,
}

/// An engine with the ledger, or one without the routes (older than fork #87's wiring).
#[derive(Debug, Clone, PartialEq)]
pub enum LedgerAnswer<T> {
    Ledger(T),
    NoLedger,
}

/// A registration's answer: accepted or refused by name, and the ledger as it stands after it
/// (the core reconciles from this snapshot, never from what it asked for).
#[derive(Debug, Clone, PartialEq)]
pub struct Registered {
    pub refused: Option<GrantRefusal>,
    pub ledger: LedgerSnapshot,
}

impl LlamaServerProcess {
    /// `GET /learn/grants`: the ledger as it stands.
    pub(super) async fn learn_ledger(&self) -> Result<LedgerAnswer<LedgerSnapshot>, LlamaServerError> {
        let reply = self.ledger_request(self.client.get(format!("{}/learn/grants", self.root))).await?;
        Ok(match reply {
            LedgerAnswer::Ledger(reply) => LedgerAnswer::Ledger(reply.snapshot),
            LedgerAnswer::NoLedger => LedgerAnswer::NoLedger,
        })
    }

    /// `POST /learn/grants`: register `terms` with the engine.
    pub(super) async fn register_learn_grant(&self, terms: &GrantTerms) -> Result<LedgerAnswer<Registered>, LlamaServerError> {
        let body = serde_json::json!({
            "id": terms.id.to_string(),
            "engine": terms.engine,
            "period_ms": duration_ms(terms.period)?,
            "share_ppm": terms.share_ppm.get(),
            "expires_in_ms": duration_ms(terms.expires_in)?,
        });
        let reply = self.ledger_request(self.client.post(format!("{}/learn/grants", self.root)).json(&body)).await?;
        Ok(match reply {
            LedgerAnswer::Ledger(reply) => LedgerAnswer::Ledger(Registered {
                refused: match reply.refusal {
                    WireRefusal::None(NoRefusal::None) => None,
                    WireRefusal::Refused(why) => Some(why),
                },
                ledger: reply.snapshot,
            }),
            LedgerAnswer::NoLedger => LedgerAnswer::NoLedger,
        })
    }

    /// `POST /learn/grants/revoke`: no new work under `id` (idempotent); its account goes once its
    /// last reservation settles.
    pub(super) async fn revoke_learn_grant(&self, id: &str) -> Result<LedgerAnswer<LedgerSnapshot>, LlamaServerError> {
        let body = serde_json::json!({ "id": id });
        let reply = self.ledger_request(self.client.post(format!("{}/learn/grants/revoke", self.root)).json(&body)).await?;
        Ok(match reply {
            LedgerAnswer::Ledger(reply) => LedgerAnswer::Ledger(reply.snapshot),
            LedgerAnswer::NoLedger => LedgerAnswer::NoLedger,
        })
    }

    async fn ledger_request(&self, request: reqwest::RequestBuilder) -> Result<LedgerAnswer<LedgerReply>, LlamaServerError> {
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
                return Err(LlamaServerError::Unreachable(format!("/learn/grants status {status}: {text}")));
            }
        }
        let bytes = resp.bytes().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        serde_json::from_slice(&bytes) // engine HTTP wire: the ledger reply
            .map(LedgerAnswer::Ledger)
            .map_err(|e| LlamaServerError::Unreachable(format!("/learn/grants answered no ledger: {e}")))
    }
}

/// A duration on the wire in whole milliseconds, refused past what the field holds.
fn duration_ms(d: std::time::Duration) -> Result<u64, LlamaServerError> {
    u64::try_from(d.as_millis()).map_err(|_| LlamaServerError::Unreachable(format!("{d:?} does not fit the ledger's milliseconds")))
}

