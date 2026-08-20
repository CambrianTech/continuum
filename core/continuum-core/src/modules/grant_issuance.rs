//! `grid/grant/issue` — the operator command that mints a capability grant.
//!
//! The owner-facing front door to [`issue_grant`](crate::routing::grant_issuance::issue_grant):
//! an operator (or an autonomous owner persona) names which of its running personas
//! should sign, the grantee, and the capabilities to sell, and gets back the base64
//! grant blob to deliver. The grantee stores it (a
//! [`PresentedGrantStore`](crate::routing::presented_grant_store::PresentedGrantStore))
//! and presents it; the owner's handler verifies it. This closes the contracted-grid
//! loop with an operator surface instead of hand-written Rust.
//!
//! ## Why it signs with a PERSONA's key, and why that's safe
//!
//! Each persona is its own citizen/owner that sells ITS compute, so the issuer is a
//! persona's airc identity (looked up in the [`PersonaAircRuntimeRegistry`]). The
//! `issuerPersonaId` is a parameter — but `grid/grant/issue` is OWNER-gated (it is
//! not in the cross-grid ACL allow-list, so it falls to the `""`=Owner wildcard),
//! meaning only the local operator can call it. The operator owns all its personas
//! and chooses which one issues; a remote peer can never reach this command.

use airc_core::PeerId;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::routing::grant_issuance::{issue_grant, IssueGrantParams};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// The command this module owns.
pub const ISSUE: &str = "grid/grant/issue";

fn default_epoch() -> u64 {
    1
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `grid/grant/issue` request — what to grant, to whom, signed by which persona.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueRequest {
    /// Which of this node's running personas signs the grant (the issuer/seller).
    issuer_persona_id: Uuid,
    /// The peer the grant is for (must be enrolled in the issuer's airc registry).
    grantee: Uuid,
    /// Capability tags conferred — the SAME vocabulary the verifier's `confers()`
    /// + the command ACL match on (e.g. `"ai/generate"`, `"compute/run"`).
    capabilities: Vec<String>,
    /// Optional expiry (epoch-ms). A paid grant SHOULD set this.
    #[serde(default)]
    expires_at_ms: Option<u64>,
    /// Monotonic epoch per grantee (default 1). Re-issue higher to update; revoke
    /// with a higher epoch + empty capabilities.
    #[serde(default = "default_epoch")]
    epoch: u64,
}

/// `grid/grant/issue` response — the blob to deliver to the grantee.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IssueResponse {
    /// base64 `SignedCapabilityGrant` — deliver to the grantee, which holds it in a
    /// `PresentedGrantStore` keyed by this issuer and presents it on requests.
    grant: String,
    issuer_persona_id: Uuid,
    grantee: Uuid,
}

/// Mints capability grants via a running persona's airc identity. Holds the
/// [`PersonaAircRuntimeRegistry`] (shared with the instance manager) to resolve the
/// issuing persona's airc handle.
pub struct GrantIssuanceModule {
    registry: PersonaAircRuntimeRegistry,
}

impl GrantIssuanceModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServiceModule for GrantIssuanceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "grant-issuance",
            priority: ModulePriority::Normal,
            command_prefixes: &[ISSUE],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, _command: &str, params: Value) -> Result<CommandResult, String> {
        let req: IssueRequest =
            serde_json::from_value(params).map_err(|e| format!("decode {ISSUE} params: {e}"))?;

        // The issuer must be a persona running on THIS node — only a live local
        // persona owns the airc key that signs. Per [[no-fallbacks-ever]] a missing
        // issuer is a hard error, not a silent skip.
        let runtime = self.registry.get(req.issuer_persona_id).ok_or_else(|| {
            format!(
                "issuer persona {} is not running on this node — only a live local \
                 persona can issue grants (it signs with its own airc identity)",
                req.issuer_persona_id
            )
        })?;

        let grant = issue_grant(
            runtime.airc(),
            now_ms(),
            IssueGrantParams {
                grantee: PeerId(req.grantee),
                capabilities: req.capabilities,
                expires_at_ms: req.expires_at_ms,
                epoch: req.epoch,
            },
        )
        .await
        .map_err(|e| format!("{ISSUE}: {e}"))?;

        let response = IssueResponse {
            grant,
            issuer_persona_id: req.issuer_persona_id,
            grantee: req.grantee,
        };
        serde_json::to_value(response)
            .map(CommandResult::Json)
            .map_err(|e| format!("encode {ISSUE} response: {e}"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: a malformed request (missing required fields) is surfaced
    // as a typed decode error, not a panic or a silent empty grant.
    #[tokio::test]
    async fn rejects_malformed_request() {
        let module = GrantIssuanceModule::new(PersonaAircRuntimeRegistry::new());
        let err = module
            .handle_command(ISSUE, json!({"grantee": "not-even-the-right-shape"}))
            .await
            .expect_err("malformed params must error");
        assert!(
            err.contains(ISSUE),
            "error should name the command, got: {err}"
        );
    }

    // what this catches: issuing as a persona that is NOT running on this node is a
    // hard error (the issuer owns the signing key; we never fabricate one). Covers
    // the registry-miss branch without standing up a full persona runtime — the
    // happy path is proven end-to-end in tests/capability_grant_e2e.rs.
    #[tokio::test]
    async fn rejects_issuer_not_running_on_this_node() {
        let module = GrantIssuanceModule::new(PersonaAircRuntimeRegistry::new());
        let err = module
            .handle_command(
                ISSUE,
                json!({
                    "issuerPersonaId": Uuid::new_v4(),
                    "grantee": Uuid::new_v4(),
                    "capabilities": ["ai/generate"],
                }),
            )
            .await
            .expect_err("unknown issuer persona must error");
        assert!(
            err.contains("not running on this node"),
            "error should explain the issuer must be a live local persona, got: {err}"
        );
    }
}
