//! The serving pool the main lane's bytes live in. (The activity holds moved to
//! `inference::serving_holds`.)

use super::*;

/// The pool the main lane's bytes live in: the ONE decision every serving site keyed by a
/// resource kind reads through (the plan's ceiling, the replace-myself credit, the
/// credited-once check, the consumer's own attribution). The mapping is
/// [`crate::inference::llama_server::LanePlacement::pool_kind`]; #4169 made the physical
/// bound follow the placement, and this makes the PLAN follow it.
pub fn serving_pool_kind() -> ResourceKind {
    crate::inference::llama_server::main_lane_placement().pool_kind()
}
