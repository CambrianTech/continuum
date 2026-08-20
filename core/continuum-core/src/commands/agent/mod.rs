//! `agent/<verb>` — the autonomous coding-agent surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! These verbs once lived only in [`AgentModule::handle_command`](crate::modules::agent)'s
//! stringly `match` — dispatchable, but with no descriptor in the registry, so a
//! persona was never OFFERED them. As typed commands each gets a descriptor (so it
//! appears in the persona tool surface, the grid ACL, codegen, `uu`) AND routes
//! through the O(1) lock-free typed path. The wire name mirrors the file path —
//! `commands/agent/start.rs` ⟺ `agent/start`.
//!
//! Access split follows the resource-authority boundary: `start`/`stop` control a
//! background agent that runs arbitrary shell, writes files, and drives git → an
//! authority mutation, gated `Privileged`. `status`/`list`/`wait` are introspection
//! → `AiSafe`.
//!
//! All five share the module's [`AgentService`](crate::modules::agent::AgentService)
//! (the live agent map + runtime handle + bus), captured by `Arc` so one caller's
//! `start` and another's `status` observe the same agents.

use std::sync::Arc;

use crate::modules::agent::AgentService;
use crate::sdk_codegen::DynCommand;

pub mod list;
pub mod start;
pub mod status;
pub mod stop;
pub mod wait;

/// `agent/solve` — the headless single-task benchmark keystone. Unlike the dep-holding
/// `start`/`stop`/… verbs above (which share `Arc<AgentService>`), `solve` is a stateless
/// composition over the cognition-drive seams and self-registers via
/// `register_stateless_command!` — it needs no `AgentService`, so it is NOT wired into the
/// object map below.
pub mod solve;

use list::AgentList;
use start::AgentStart;
use status::AgentGetStatus;
use stop::AgentStop;
use wait::AgentWait;

/// The dep-holding `agent/*` command objects [`AgentModule`](crate::modules::agent::AgentModule)
/// contributes to the kernel's typed object map. Each carries the shared
/// `Arc<AgentService>`; the executor routes each name straight here, winning over
/// the (now-dead) legacy prefix arm.
pub fn command_objects(service: Arc<AgentService>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(AgentStart {
            service: service.clone(),
        }),
        Arc::new(AgentGetStatus {
            service: service.clone(),
        }),
        Arc::new(AgentStop {
            service: service.clone(),
        }),
        Arc::new(AgentList {
            service: service.clone(),
        }),
        Arc::new(AgentWait { service }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the five agent commands carry their `agent/<verb>` wire
    // names — the routing keys uu / the persona tool surface / the grid bind to. The
    // name mirrors the file path; drift silently breaks "the file tree IS the
    // namespace".
    #[test]
    fn agent_command_names_mirror_their_path() {
        assert_eq!(AgentStart::NAME, "agent/start");
        assert_eq!(AgentGetStatus::NAME, "agent/status");
        assert_eq!(AgentStop::NAME, "agent/stop");
        assert_eq!(AgentList::NAME, "agent/list");
        assert_eq!(AgentWait::NAME, "agent/wait");
    }

    // what this catches: the authority split — spawning/stopping a shell-running
    // agent is Privileged; reads are AiSafe. A regression here would silently widen
    // (or close) the persona surface for a high-capability tool.
    #[test]
    fn access_levels_follow_the_authority_boundary() {
        use crate::sdk_codegen::AccessLevel;
        assert!(matches!(AgentStart::ACCESS, AccessLevel::Privileged));
        assert!(matches!(AgentStop::ACCESS, AccessLevel::Privileged));
        assert!(matches!(AgentGetStatus::ACCESS, AccessLevel::AiSafe));
        assert!(matches!(AgentList::ACCESS, AccessLevel::AiSafe));
        assert!(matches!(AgentWait::ACCESS, AccessLevel::AiSafe));
    }
}
