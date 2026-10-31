//! Core's answer to `identity::acting`: who is acting and where, from the persona airc
//! registry, the acting-root record that live above the kernel.
//! Boot hands it to each domain state at construction; every domain crate that left core asks through the kernel seam.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::identity::acting::ActingProvider;
use crate::identity::hands_context::HandsContext;
use crate::sdk_codegen::Ctx;

pub struct CoreActing;

impl ActingProvider for CoreActing {
    fn acting_peer_id(&self, ctx: &Ctx) -> Option<Uuid> {
        crate::persona::operator_peer::acting_peer_id(ctx)
    }

    fn is_local_identity(&self, peer: Uuid) -> bool {
        crate::persona::operator_peer::is_local_identity(peer)
    }

    fn acting_root_in(&self, persona: Uuid, context: Option<HandsContext>) -> Option<PathBuf> {
        crate::cognition::persona_workspace::acting_root_in(persona, context)
    }

    fn instance_python(&self, checkout: &Path) -> Option<PathBuf> {
        crate::persona::instance_env_fact::instance_python(checkout)
    }
}
