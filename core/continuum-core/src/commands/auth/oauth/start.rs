//! `auth/oauth/start` — begin an OAuth 2.0 + PKCE flow: open the system browser at
//! the authorization URL and spin up a temporary localhost redirect-catcher.
//! Initiating auth (browser + listener) is an authority op → `Privileged`.

use std::sync::Arc;

use crate::modules::auth::ExternalWebviewAuthService;
use crate::sdk_codegen::CommandError;

use super::AuthProviderRef;

crate::action_command! {
    /// Begin OAuth 2.0 + PKCE flow. Opens the system browser at the provider's
    /// authorization URL and spins up a temporary localhost redirect-catcher that
    /// exchanges the returned code for tokens and persists them. Use `auth/oauth/status`
    /// to confirm completion.
    pub struct AuthOauthStart { service: Arc<ExternalWebviewAuthService> }
    name: "auth/oauth/start",
    access: Privileged,
    params: AuthProviderRef,
    output: serde_json::Value,
    run(this, _ctx, p) => {
        this.service
            .start_flow(&p.provider_id)
            .await
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — starting a browser auth flow is a
    // Privileged authority op, never offered on the AiSafe read surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AuthOauthStart::NAME, "auth/oauth/start");
        assert!(matches!(
            AuthOauthStart::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
