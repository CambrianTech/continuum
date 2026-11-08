//! What a resident's shell carries beyond the environment every shell on this node inherits: her
//! identity on every commit.
//!
//! A resident has the environment an agent session has (Joel, 2026-11-08: "Stop caging them",
//! "I just said parity"): her shell inherits the core's whole environment, its GitHub token
//! included, so plain `git` and `gh` work for her exactly as for an agent. The one thing set for
//! her is her name as author and committer, set after every other variable so neither the session
//! nor the inherited environment can put another name on her commits.

use crate::code::git_actor::GitActor;

/// The variables a resident's command runs with: her identity, as author and committer.
pub fn resident_env(actor: &GitActor) -> Vec<(String, String)> {
    actor.env().iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}
