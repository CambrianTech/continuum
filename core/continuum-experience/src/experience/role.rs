//! The role a citizen plays in an experience: the typed id a recipe's `citizens[]` names and
//! the spawner, resolver and grid allocator reason about. Moved down from
//! `persona::role_template` (which keeps the per-role templates) so an experience recipe names
//! no persona module.

use serde::{Deserialize, Serialize};

/// The role a persona instance plays in the substrate. Roles are
/// substrate-typed (the spawner reasons about them, the resolver picks
/// models for them); persona NAMES are separate and derive from the
/// identity-seed deterministic projection ([[personas-have-names-not-
/// function-labels]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleId {
    /// The clippy-shaped on-ramp. Small + fast + friendly. Always-on,
    /// always-spawned at every hardware tier. The day-one face of the
    /// substrate.
    Helper,
    /// The Swiss-Army programmer. Bash-competent, multi-language,
    /// code-review-capable. Second-priority spawn at every tier
    /// because coders are typical first adopters.
    Coder,
    /// Code-review specialist. Spawned on-demand when a card enters
    /// Review state and needs an adversarial reviewer.
    Sentinel,
    /// Design/UX specialist — CSS, themes, typography, interface
    /// comprehension. Works eyes-first: renders, screenshots, and
    /// iterates against what the pixels actually show (Joel 2026-07-11:
    /// "iterating on graphic design and user experience as if they have
    /// eyes and ears"). First resident of the design-LoRA flywheel;
    /// dogfoods positron themes.
    Designer,
    /// Custom user-defined role. The user supplies the template; the
    /// substrate doesn't have a built-in default.
    Custom,
}

impl RoleId {
    /// Stable kebab-case identifier — used in event headers, kanban
    /// card metadata, logs, etc. Pinned so renames are intentional.
    pub fn as_str(self) -> &'static str {
        match self {
            RoleId::Helper => "helper",
            RoleId::Coder => "coder",
            RoleId::Sentinel => "sentinel",
            RoleId::Designer => "designer",
            RoleId::Custom => "custom",
        }
    }
}
