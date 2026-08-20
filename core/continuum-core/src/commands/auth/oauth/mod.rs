//! `auth/oauth/<verb>` — OAuth 2.0 + PKCE via an external browser, as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one verb per file.
//!
//! These verbs once lived only in [`ExternalWebviewAuthModule::handle_command`](crate::modules::auth)'s
//! stringly `match` — dispatchable, but with no descriptor in the registry, so they
//! never reached the persona tool surface, the grid ACL, or codegen. As typed
//! commands each gets a descriptor AND routes through the O(1) lock-free typed path,
//! winning over the (now-dead) legacy prefix arm.
//!
//! Outputs are `serde_json::Value`: token sets, status, and provider lists are
//! genuinely provider-shaped, so `JsonValue` (→ `any` in TS) is the *honest*
//! contract here, not a fake-precise struct. The bodies are transplanted verbatim
//! from the legacy arms; only error mapping is made typed (a service `Err(String)`
//! is a runtime failure → [`CommandError::Internal`](crate::sdk_codegen::CommandError)).
//!
//! Access split follows the resource-authority boundary: `start` (opens a browser +
//! spins a redirect-catcher), `refresh`/`revoke` (mutate stored tokens), and
//! `register` (adds a provider at runtime) are authority mutations → `Privileged`.
//! `status`/`providers` are non-secret reads → `AiSafe`.
//!
//! All six share the module's
//! [`ExternalWebviewAuthService`](crate::modules::auth::ExternalWebviewAuthService),
//! captured by `Arc` so every caller observes the same provider + token maps.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::auth::ExternalWebviewAuthService;
use crate::sdk_codegen::DynCommand;

pub mod providers;
pub mod refresh;
pub mod register;
pub mod revoke;
pub mod start;
pub mod status;

use providers::AuthOauthProviders;
use refresh::AuthOauthRefresh;
use register::AuthOauthRegister;
use revoke::AuthOauthRevoke;
use start::AuthOauthStart;
use status::AuthOauthStatus;

/// Shared input for the per-provider verbs (`start`/`status`/`refresh`/`revoke`):
/// the provider to act on. One type, four commands — the same `{provider_id}`
/// contract, defined once.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/auth/AuthProviderRef.ts"
)]
pub struct AuthProviderRef {
    /// Provider identifier: `"github"`, `"huggingface"`, `"google"`, or a custom
    /// registered provider.
    pub provider_id: String,
}

/// The dep-holding `auth/oauth/*` command objects
/// [`ExternalWebviewAuthModule`](crate::modules::auth::ExternalWebviewAuthModule)
/// contributes to the kernel's typed object map. Each carries the shared
/// `Arc<ExternalWebviewAuthService>`; the executor routes each name straight here.
pub fn command_objects(service: Arc<ExternalWebviewAuthService>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(AuthOauthStart {
            service: service.clone(),
        }),
        Arc::new(AuthOauthStatus {
            service: service.clone(),
        }),
        Arc::new(AuthOauthRefresh {
            service: service.clone(),
        }),
        Arc::new(AuthOauthRevoke {
            service: service.clone(),
        }),
        Arc::new(AuthOauthProviders {
            service: service.clone(),
        }),
        Arc::new(AuthOauthRegister { service }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the six oauth commands carry their `auth/oauth/<verb>` wire
    // names — the routing keys uu / the persona tool surface / the grid bind to. The
    // name mirrors the file path; drift silently breaks "the file tree IS the
    // namespace".
    #[test]
    fn oauth_command_names_mirror_their_path() {
        assert_eq!(AuthOauthStart::NAME, "auth/oauth/start");
        assert_eq!(AuthOauthStatus::NAME, "auth/oauth/status");
        assert_eq!(AuthOauthRefresh::NAME, "auth/oauth/refresh");
        assert_eq!(AuthOauthRevoke::NAME, "auth/oauth/revoke");
        assert_eq!(AuthOauthProviders::NAME, "auth/oauth/providers");
        assert_eq!(AuthOauthRegister::NAME, "auth/oauth/register");
    }

    // what this catches: the authority split — initiating auth, mutating/revoking
    // tokens, and registering a provider are Privileged; non-secret reads (status,
    // provider list) are AiSafe. A regression here would silently widen the persona
    // surface over OAuth credential machinery.
    #[test]
    fn access_levels_follow_the_authority_boundary() {
        use crate::sdk_codegen::AccessLevel;
        assert!(matches!(AuthOauthStart::ACCESS, AccessLevel::Privileged));
        assert!(matches!(AuthOauthRefresh::ACCESS, AccessLevel::Privileged));
        assert!(matches!(AuthOauthRevoke::ACCESS, AccessLevel::Privileged));
        assert!(matches!(AuthOauthRegister::ACCESS, AccessLevel::Privileged));
        assert!(matches!(AuthOauthStatus::ACCESS, AccessLevel::AiSafe));
        assert!(matches!(AuthOauthProviders::ACCESS, AccessLevel::AiSafe));
    }
}
