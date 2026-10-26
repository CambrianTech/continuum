//! The acting provider hands' tests install: the shape of core's answers without the persona
//! registry, the acting-root record or the grid policy (those live above this crate).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use uuid::Uuid;

use crate::identity::acting::ActingProvider;
use crate::identity::hands_context::HandsContext;
use crate::routing::auth_policy::{CallerIdentity, CallerSource};
use crate::routing::trust_level::TrustLevel;
use crate::sdk_codegen::Ctx;

struct TestActing;

impl ActingProvider for TestActing {
    fn acting_peer_id(&self, ctx: &Ctx) -> Option<Uuid> {
        ctx.caller.as_ref().map(|c| c.peer_id.as_uuid())
    }

    fn is_local_identity(&self, _peer: Uuid) -> bool {
        false
    }

    fn acting_root_in(&self, _persona: Uuid, _context: Option<HandsContext>) -> Option<PathBuf> {
        None
    }

    fn instance_python(&self, _checkout: &Path) -> Option<PathBuf> {
        None
    }

    fn caller_trust(&self, caller: Option<&CallerIdentity>) -> TrustLevel {
        let Some(caller) = caller else {
            return TrustLevel::Owner;
        };
        match caller.source {
            CallerSource::Local => TrustLevel::Owner,
            CallerSource::LocalPersona => TrustLevel::Trusted,
            _ => TrustLevel::Provisional,
        }
    }
}

/// Install the test provider (idempotent).
pub(crate) fn install() {
    crate::identity::acting::install(Arc::new(TestActing));
}
