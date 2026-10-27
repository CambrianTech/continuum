//! The acting provider hands' tests hand to `CodeState`: the shape of core's answers without the persona
//! registry, or the acting-root record (those live above this crate).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use uuid::Uuid;

use crate::identity::acting::ActingProvider;
use crate::identity::hands_context::HandsContext;
use crate::sdk_codegen::Ctx;

pub(crate) struct TestActing;

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
}

/// A code state whose executor is bound to the kernel's default test policy
/// (`AllowAllPolicy`: the substrate and a `Local` caller are the owner, every other source
/// resolves no trust). The one place a hands test gets trust, as production does: from the
/// executor's policy.
pub(crate) fn state_with_default_policy(rt: tokio::runtime::Handle) -> Arc<crate::modules::code::CodeState> {
    let state = Arc::new(crate::modules::code::CodeState::new(
        Arc::new(dashmap::DashMap::new()),
        Arc::new(dashmap::DashMap::new()),
        rt,
        Arc::new(crate::test_acting::TestActing),
    ));
    state.executor.install(Arc::new(crate::runtime::CommandExecutor::new(Arc::new(crate::runtime::ModuleRegistry::new()))));
    state
}
