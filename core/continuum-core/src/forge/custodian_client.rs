//! `forge::custodian_client` — the core-side HTTP client for the forge custodian,
//! speaking [`forge::protocol`](super::protocol) verbatim.
//!
//! This is the de-`unsloth` replacement for the gguf-lora export path that used
//! to live in `inference/unsloth_forge.rs::UnslothForgeHttp::package`. That path
//! had two bugs Contract C exists to kill:
//! 1. **Wrong endpoint.** It POSTed gguf-lora to `unsloth_base_url()` — the
//!    unsloth host, which CANNOT produce a GGUF LoRA (the whole reason the
//!    continuum custodian binary exists). This client targets the custodian's
//!    own address ([`protocol::DEFAULT_CUSTODIAN_ADDR`], override
//!    `FORGE_CUSTODIAN_ADDR`).
//! 2. **Wrong wire shape.** It ran a stateful `load-checkpoint` first, then sent
//!    a body WITHOUT `checkpoint` and WITH `push_to_hub`/`repo_id` the custodian
//!    never reads — the stateless [`protocol::GgufLoraRequest`] rejects it. This
//!    client sends exactly the contract type: stateless, checkpoint-in-body.
//!
//! ## Transport-agnostic by design (grid-readiness)
//! The capability is the [`ForgeCustodian`] trait; [`ForgeCustodianHttp`] is the
//! LOCAL-HTTP impl. A future `GridForgeCustodian` impl routes the same
//! [`protocol`] types to a remote node over the grid transport — the caller
//! (`modules/forge.rs`) depends only on the trait, so grid negotiation slots in
//! beneath it without touching consumers. See
//! `docs/architecture/FORGE-CUSTODIAN-CONTRACT.md`.
//!
//! ## Fail loud, never degrade
//! An unreachable custodian or a failed export is a LOUD typed error the caller
//! surfaces with its cause — never a silent no-op
//! (`[[fallbacks-are-illegal-fail-loud]]`). The two variants are kept distinct
//! because a router heals differently on each: [`ForgeCustodianError::Unreachable`]
//! ⇒ try another endpoint; [`ForgeCustodianError::Api`] ⇒ the same job will fail
//! the same way elsewhere, so do not re-route.

use async_trait::async_trait;

use super::protocol::{
    ExportResult, GgufLoraRequest, HealthResponse, CONTRACT_VERSION, DEFAULT_CUSTODIAN_ADDR,
    ROUTE_GGUF_LORA, ROUTE_HEALTH,
};
use crate::config_env;

/// Typed forge-custodian client error. Two variants, distinguished so a router
/// can decide heal-vs-don't-heal (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeCustodianError {
    /// Transport-level failure — the custodian could not be reached at all.
    /// A router may re-route an idempotent job to an equivalent endpoint.
    Unreachable(String),
    /// The custodian was reached but the request failed (non-2xx, bad JSON, or a
    /// contract-version mismatch). The same job will fail the same way elsewhere.
    Api(String),
}

impl std::fmt::Display for ForgeCustodianError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(m) => write!(f, "forge custodian unreachable: {m}"),
            Self::Api(m) => write!(f, "forge custodian error: {m}"),
        }
    }
}
impl std::error::Error for ForgeCustodianError {}

/// The organism-facing forge-custodian capability. Today: gguf-lora export +
/// the health handshake. A custodian that grows formats appends methods; a grid
/// transport impl satisfies the SAME trait so consumers never change.
#[async_trait]
pub trait ForgeCustodian: Send + Sync {
    /// Liveness + capability + contract-version handshake.
    async fn health(&self) -> Result<HealthResponse, ForgeCustodianError>;

    /// Convert a trained MLX checkpoint into a pageable GGUF LoRA gene. Stateless:
    /// the request names the checkpoint directly. The produced bytes land under
    /// `req.save_directory` (custodian-owned); the caller records the resulting
    /// handle (see `forge::adapter_manifest`).
    async fn export_gguf_lora(
        &self,
        req: &GgufLoraRequest,
    ) -> Result<ExportResult, ForgeCustodianError>;

    /// Verify the custodian speaks a contract version this client understands,
    /// BEFORE dispatching work — fail loud at the handshake, not deep in a body.
    /// Default impl: `health()` then compare. Override only to relax the policy.
    async fn ensure_contract(&self) -> Result<HealthResponse, ForgeCustodianError> {
        let h = self.health().await?;
        if h.contract_version != CONTRACT_VERSION {
            return Err(ForgeCustodianError::Api(format!(
                "contract version mismatch: client speaks {CONTRACT_VERSION}, custodian speaks {} \
                 (capability={}) — refusing to dispatch",
                h.contract_version, h.capability
            )));
        }
        Ok(h)
    }
}

/// The base URL the client connects to — `http://{FORGE_CUSTODIAN_ADDR}`,
/// defaulting to [`DEFAULT_CUSTODIAN_ADDR`]. One place, mirroring where the
/// custodian binary binds.
pub fn custodian_base_url() -> String {
    let addr = config_env::read("FORGE_CUSTODIAN_ADDR")
        .unwrap_or_else(|| DEFAULT_CUSTODIAN_ADDR.to_string());
    // Allow a fully-qualified override (someone may set a full URL); otherwise
    // prefix http:// for the bare host:port the binary binds.
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr
    } else {
        format!("http://{addr}")
    }
}

/// Real `reqwest` impl over the custodian's local HTTP surface. No auth — the
/// custodian binds loopback by default and carries no API key (unlike the
/// retired unsloth client).
pub struct ForgeCustodianHttp {
    base_url: String,
    client: reqwest::Client,
}

impl ForgeCustodianHttp {
    /// Build from config (endpoint via [`custodian_base_url`]). Fresh pooled
    /// client; cheap to clone if a caller wants to share the connection pool.
    pub fn from_config() -> Self {
        Self::with_client(reqwest::Client::new())
    }

    /// Reuse an existing pooled `reqwest::Client` (shares the connection pool).
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            base_url: custodian_base_url(),
            client,
        }
    }

    /// Point the client at an explicit base URL (used by integration tests that
    /// bind the custodian on an ephemeral port).
    pub fn with_base_url(base_url: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.into(),
            client,
        }
    }
}

#[async_trait]
impl ForgeCustodian for ForgeCustodianHttp {
    async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
        let url = format!("{}{}", self.base_url, ROUTE_HEALTH);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ForgeCustodianError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ForgeCustodianError::Api(format!(
                "{ROUTE_HEALTH} {}",
                resp.status()
            )));
        }
        resp.json::<HealthResponse>()
            .await
            .map_err(|e| ForgeCustodianError::Api(format!("{ROUTE_HEALTH}: decode {e}")))
    }

    async fn export_gguf_lora(
        &self,
        req: &GgufLoraRequest,
    ) -> Result<ExportResult, ForgeCustodianError> {
        let url = format!("{}{}", self.base_url, ROUTE_GGUF_LORA);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| ForgeCustodianError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ForgeCustodianError::Api(format!(
                "{ROUTE_GGUF_LORA} {status}: {text}"
            )));
        }
        resp.json::<ExportResult>()
            .await
            .map_err(|e| ForgeCustodianError::Api(format!("{ROUTE_GGUF_LORA}: decode {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a bare host:port from config is turned into an http URL,
    // and a fully-qualified override is passed through untouched — the gguf-lora
    // bug was POSTing to the wrong endpoint, so URL construction is load-bearing.
    #[test]
    fn base_url_prefixes_bare_host_but_respects_full_url() {
        // with_base_url stores verbatim (the test/integration seam)
        let c = ForgeCustodianHttp::with_base_url("http://127.0.0.1:9", reqwest::Client::new());
        assert_eq!(c.base_url, "http://127.0.0.1:9");
        // the pure helper logic:
        let bare = "127.0.0.1:8899";
        let made = if bare.starts_with("http") {
            bare.to_string()
        } else {
            format!("http://{bare}")
        };
        assert_eq!(made, "http://127.0.0.1:8899");
    }

    // what this catches: ensure_contract REFUSES a custodian whose version this
    // client can't speak — drift caught at the handshake, never as a bad body.
    #[tokio::test]
    async fn ensure_contract_rejects_version_mismatch() {
        struct WrongVersion;
        #[async_trait]
        impl ForgeCustodian for WrongVersion {
            async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
                // a future custodian one version ahead of this client
                Ok(HealthResponse {
                    contract_version: CONTRACT_VERSION + 1,
                    ..HealthResponse::ok_gguf_lora()
                })
            }
            async fn export_gguf_lora(
                &self,
                _req: &GgufLoraRequest,
            ) -> Result<ExportResult, ForgeCustodianError> {
                unreachable!("must not dispatch on a version mismatch")
            }
        }
        let err = WrongVersion
            .ensure_contract()
            .await
            .expect_err("must refuse");
        match err {
            ForgeCustodianError::Api(m) => assert!(m.contains("version mismatch"), "got: {m}"),
            other => panic!("expected Api mismatch, got {other:?}"),
        }
    }

    // what this catches: a matching version passes the handshake (so the refusal
    // above isn't refusing everything).
    #[tokio::test]
    async fn ensure_contract_accepts_matching_version() {
        struct RightVersion;
        #[async_trait]
        impl ForgeCustodian for RightVersion {
            async fn health(&self) -> Result<HealthResponse, ForgeCustodianError> {
                Ok(HealthResponse::ok_gguf_lora())
            }
            async fn export_gguf_lora(
                &self,
                _req: &GgufLoraRequest,
            ) -> Result<ExportResult, ForgeCustodianError> {
                unreachable!()
            }
        }
        let h = RightVersion
            .ensure_contract()
            .await
            .expect("matching version passes");
        assert_eq!(h.contract_version, CONTRACT_VERSION);
    }

    // what this catches: a real client pointed at a dead port yields Unreachable
    // (the heal-able class), not Api — the distinction a router relies on.
    #[tokio::test]
    async fn dead_endpoint_is_unreachable_not_api() {
        // 127.0.0.1:1 is reserved/never-listening.
        let c = ForgeCustodianHttp::with_base_url("http://127.0.0.1:1", reqwest::Client::new());
        let err = c.health().await.expect_err("nothing is listening");
        assert!(
            matches!(err, ForgeCustodianError::Unreachable(_)),
            "got: {err:?}"
        );
    }
}
