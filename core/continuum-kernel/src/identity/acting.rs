//! Who is acting, and where: the contract every domain's commands (code, work, ...) need
//! from the layers above the kernel, installed once at boot. The persona airc registry,
//! the acting-root record and the grid trust policy live above the kernel; a domain crate
//! below them asks through this seam instead of importing them. Not installed is a typed
//! error ([`ActingError::NotInstalled`]), never a default identity or a default trust.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use uuid::Uuid;

use super::hands_context::HandsContext;
use crate::routing::auth_policy::CallerIdentity;
use crate::routing::trust_level::TrustLevel;
use crate::sdk_codegen::{CommandError, Ctx};

/// What the layers above the kernel answer about the acting caller.
pub trait ActingProvider: Send + Sync {
    /// The peer this call acts AS (the authenticated caller, else a local self-peer).
    fn acting_peer_id(&self, ctx: &Ctx) -> Option<Uuid>;
    /// Is `peer` one of this node's local identities (operator or agent self-peer)?
    fn is_local_identity(&self, peer: Uuid) -> bool;
    /// The root a persona's acting context is rooted at, if any.
    fn acting_root_in(&self, persona: Uuid, context: Option<HandsContext>) -> Option<PathBuf>;
    /// The prepared instance environment's python for a checkout, if it has one.
    fn instance_python(&self, checkout: &Path) -> Option<PathBuf>;
    /// The trust tier of a caller (`None` = the substrate-local owner).
    fn caller_trust(&self, caller: Option<&CallerIdentity>) -> TrustLevel;
}

/// Why the acting seam could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActingError {
    /// Boot has not installed the provider yet.
    NotInstalled,
}

impl std::fmt::Display for ActingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("acting provider not installed yet (identity::acting)"),
        }
    }
}

impl std::error::Error for ActingError {}

impl From<ActingError> for CommandError {
    fn from(e: ActingError) -> Self {
        CommandError::Internal(e.to_string())
    }
}

/// An install-once slot for the provider.
pub struct ActingSlot {
    slot: OnceLock<Arc<dyn ActingProvider>>,
}

impl ActingSlot {
    pub const fn new() -> Self {
        Self { slot: OnceLock::new() }
    }

    /// Install the provider; a second install is ignored (the first wins).
    pub fn install(&self, provider: Arc<dyn ActingProvider>) {
        let _ = self.slot.set(provider);
    }

    pub fn require(&self) -> Result<&Arc<dyn ActingProvider>, ActingError> {
        self.slot.get().ok_or(ActingError::NotInstalled)
    }
}

impl Default for ActingSlot {
    fn default() -> Self {
        Self::new()
    }
}

static ACTING: ActingSlot = ActingSlot::new();

/// Boot installs the provider here.
pub fn install(provider: Arc<dyn ActingProvider>) {
    ACTING.install(provider);
}

/// The installed provider, or [`ActingError::NotInstalled`].
pub fn provider() -> Result<&'static Arc<dyn ActingProvider>, ActingError> {
    ACTING.require()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a missing provider answering with a default identity or trust
    // (an unconfined owner) instead of failing as a typed error.
    #[test]
    fn not_installed_is_a_typed_error() {
        let slot = ActingSlot::new();
        assert_eq!(slot.require().err(), Some(ActingError::NotInstalled));
        assert!(matches!(CommandError::from(ActingError::NotInstalled), CommandError::Internal(_)));
    }
}
