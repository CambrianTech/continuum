//! `auth/oauth/refresh` — exchange the stored refresh_token for a fresh access
//! token. Mutates persisted tokens → `Privileged`.

use std::sync::Arc;

use crate::modules::auth::ExternalWebviewAuthService;
use crate::sdk_codegen::CommandError;

use super::AuthProviderRef;

crate::action_command! {
    /// Refresh the access token using the stored refresh_token. Persists the new
    /// token set. Errors if no refresh_token is stored or the provider rejects it.
    pub struct AuthOauthRefresh { service: Arc<ExternalWebviewAuthService> }
    name: "auth/oauth/refresh",
    access: Privileged,
    params: AuthProviderRef,
    output: serde_json::Value,
    run(this, _ctx, p) => {
        this.service
            .refresh_token(&p.provider_id)
            .await
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — refreshing rewrites stored tokens, a
    // Privileged authority op, not the AiSafe read surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AuthOauthRefresh::NAME, "auth/oauth/refresh");
        assert!(matches!(
            AuthOauthRefresh::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
