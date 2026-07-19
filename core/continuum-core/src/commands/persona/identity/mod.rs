//! `persona/identity/<verb>` — the persona's self-authored, editable identity card.
//!
//! Her airc `peer_id` is the one immutable anchor; everything else (gender, appearance,
//! voice, role, and the open profile — bio/goals/desires/interests/…) is hers to author
//! ([[persona-identity-is-fully-self-editable-except-the-id]]). [`set`] is the edit verb.

use std::path::PathBuf;
use std::sync::Arc;

use crate::sdk_codegen::DynCommand;

pub mod set;

use set::PersonaIdentitySet;

/// The dep-holding `persona/identity/*` command objects (they resolve persona homes
/// under `continuum_root` to persist edits to the durable card).
pub fn command_objects(continuum_root: PathBuf) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(PersonaIdentitySet { continuum_root })]
}
