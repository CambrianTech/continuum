//! Who is acting, and where: the contract every domain's commands (code, work, ...) need
//! from the layers above the kernel. The persona airc registry and the acting-root record
//! live above the kernel; a domain crate below them asks through this seam instead of
//! importing them. The provider is handed to its consumer at construction (a field of the
//! domain's state), never a global: there is no default identity and no ambient slot.
//! Trust is not here: it has one owner, the executor's policy.

use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::hands_context::HandsContext;
use crate::sdk_codegen::Ctx;

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
}
