//! `auth/oauth/register` — register a new OAuth provider configuration at runtime.
//! Adds an authority-bearing provider → `Privileged`.
//!
//! The params type IS [`OAuthClientConfig`](crate::modules::auth::OAuthClientConfig):
//! the legacy arm deserialized the whole param object into it, so the config struct
//! is the wire contract directly (no wrapper).

use std::sync::Arc;

use crate::modules::auth::{AuthRegistered, ExternalWebviewAuthService, OAuthClientConfig};

crate::action_command! {
    /// Register a new OAuth provider configuration at runtime so it can be used by
    /// `auth/oauth/start` and friends. Supply the full client config (id, endpoints,
    /// scopes, redirect port). Returns `{ registered: true }`.
    pub struct AuthOauthRegister { service: Arc<ExternalWebviewAuthService> }
    name: "auth/oauth/register",
    access: Privileged,
    params: OAuthClientConfig,
    output: AuthRegistered,
    run(this, _ctx, p) => {
        this.service.register_provider(p).await;
        Ok(AuthRegistered { registered: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — registering a provider is a Privileged
    // authority mutation, never on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AuthOauthRegister::NAME, "auth/oauth/register");
        assert!(matches!(
            AuthOauthRegister::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: a registered provider becomes visible to the service — the
    // body actually wires the config into the shared provider map (not a no-op stub).
    #[tokio::test]
    async fn register_makes_provider_visible() {
        let service = Arc::new(ExternalWebviewAuthService::new());
        let cmd = AuthOauthRegister {
            service: service.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                OAuthClientConfig {
                    provider_id: "acme".into(),
                    client_id: "cid".into(),
                    client_secret: None,
                    auth_url: "https://acme.test/auth".into(),
                    token_url: "https://acme.test/token".into(),
                    scopes: "read".into(),
                    redirect_port: 49998,
                    revoke_url: None,
                },
            )
            .await
            .unwrap();
        assert!(out.registered);
        let providers = service.list_providers().await;
        let names: Vec<&str> = providers
            .providers
            .iter()
            .map(|p| p.provider_id.as_str())
            .collect();
        assert!(names.contains(&"acme"));
    }
}
