//! A persona's git identity, as the mind knows her: the hands' [`GitActor`] for her peer, named by
//! her live runtime. The one place the mind turns a persona id into the actor her commits are by.

use uuid::Uuid;

pub use crate::code::git_actor::GitActor;

/// The display name of `peer` when this node hosts her: a resident's runtime, or the agent or
/// operator self-peer's. `None` for a peer this node does not host.
pub fn name_of(peer: Uuid) -> Option<String> {
    crate::persona::PersonaAircRuntimeRegistry::try_global()
        .and_then(|registry| registry.get(peer))
        .or_else(|| crate::persona::operator_peer::local_runtime_of(peer))
        .map(|runtime| runtime.agent_name().to_string())
}

/// The actor `persona`'s commits are by: her peer id, named by [`name_of`].
pub fn of_persona(persona: Uuid) -> GitActor {
    GitActor::citizen(persona, name_of(persona).as_deref())
}
