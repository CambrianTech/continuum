//! `auth/oauth/providers` — list registered OAuth providers (public config only,
//! no secrets). A non-secret read → `AiSafe`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::auth::{ExternalWebviewAuthService, ProviderList};

/// `auth/oauth/providers` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/auth/AuthProvidersParams.ts"
)]
pub struct AuthProvidersParams {}

crate::action_command! {
    /// List all registered OAuth providers. Returns public configuration only —
    /// `provider_id`, `auth_url`, `scopes`, `redirect_port`, whether a revoke endpoint
    /// exists — never client secrets or tokens.
    pub struct AuthOauthProviders { service: Arc<ExternalWebviewAuthService> }
    name: "auth/oauth/providers",
    access: AiSafe,
    params: AuthProvidersParams,
    output: ProviderList,
    run(this, _ctx, _p) => {
        Ok(this.service.list_providers().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — listing providers is a non-secret read
    // on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AuthOauthProviders::NAME, "auth/oauth/providers");
        assert!(matches!(
            AuthOauthProviders::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: a fresh service with no registered providers returns an
    // empty `providers` array (not an error / not null) — callers can always iterate.
    #[tokio::test]
    async fn empty_service_lists_no_providers() {
        let cmd = AuthOauthProviders {
            service: Arc::new(ExternalWebviewAuthService::new()),
        };
        let out = cmd
            .run(&Ctx::default(), AuthProvidersParams {})
            .await
            .unwrap();
        assert_eq!(out.providers.len(), 0);
    }
}
