//! `auth/oauth/status` — report whether tokens exist for a provider and whether
//! they are expired. A non-secret read → `AiSafe`.

use std::sync::Arc;

use crate::modules::auth::ExternalWebviewAuthService;

use super::AuthProviderRef;

crate::action_command! {
    /// Check whether tokens exist for a provider and whether they are expired. Returns
    /// `{ authenticated, expired, ... }` — no secrets, just the auth state.
    pub struct AuthOauthStatus { service: Arc<ExternalWebviewAuthService> }
    name: "auth/oauth/status",
    access: AiSafe,
    params: AuthProviderRef,
    output: serde_json::Value,
    run(this, _ctx, p) => {
        Ok(this.service.token_status(&p.provider_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — token status is a non-secret read on
    // the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AuthOauthStatus::NAME, "auth/oauth/status");
        assert!(matches!(
            AuthOauthStatus::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: an unauthenticated provider reports `authenticated: false`
    // (not an error) — preserving the legacy soft-status contract callers branch on.
    #[tokio::test]
    async fn unauthenticated_provider_reports_false() {
        let cmd = AuthOauthStatus {
            service: Arc::new(ExternalWebviewAuthService::new()),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                AuthProviderRef {
                    provider_id: "github".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(out["authenticated"], false);
    }
}
