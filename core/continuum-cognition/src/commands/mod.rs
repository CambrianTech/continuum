//! The stateless `cognition/*` commands: pure functions over this crate's own cognition modules,
//! self-registered through `inventory` (cut out of continuum-core, plan phase 2). The commands that hold
//! the persona module's state live in continuum-mind, which re-exports these under the old path.

pub mod embed_tools;
pub mod generate_recipe;
pub mod generate_response;
pub mod plan_turn_batch;
pub mod rate_proposals;
pub mod semantic_search_tools;
pub mod should_respond;
pub mod validate_response_decision;
