//! `auth/oauth/revoke` — revoke tokens server-side (if a revocation endpoint is
//! configured) and delete them from config.env. Destroys credentials → `Privileged`.

use std::sync::Arc;

use crate::modules::auth::{ExternalWebviewAuthService, TokenRevoked};
use crate::sdk_codegen::CommandError;

use super::AuthProviderRef;

crate::action_command! {
    /// Revoke tokens server-side (if the provider exposes a revocation endpoint) and
    /// delete the stored tokens from config.env. Idempotent: revoking when no tokens
    /// are stored is a no-op success.
    pub struct AuthOauthRevoke { service: Arc<ExternalWebviewAuthService> }
    name: "auth/oauth/revoke",
    access: Privileged,
    params: AuthProviderRef,
    output: TokenRevoked,
    run(this, _ctx, p) => {
        this.service
            .revoke_tokens(&p.provider_id)
            .await
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — revoking destroys credentials, a
    // Privileged authority op, never on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AuthOauthRevoke::NAME, "auth/oauth/revoke");
        assert!(matches!(
            AuthOauthRevoke::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
