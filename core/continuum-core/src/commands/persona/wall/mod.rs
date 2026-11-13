//! `persona/wall/*` — the room board a persona is grounded in.
//!
//! `persona/wall/pin` publishes (or supersedes) a `WallPostPublished` through a
//! live persona's airc citizen; `persona/wall/list` reads the pinned posts with
//! their provenance, a page at a time. The grounding face is
//! [`WallSource`](crate::persona::wall_source::WallSource), which composes the
//! same airc rows into the persona's `[room-wall]` grounding — one shared
//! layer, no continuum-side copy.

use std::sync::Arc;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::DynCommand;

pub mod list;
pub mod pin;

/// The dep-holding `persona/wall/*` command objects, sharing the one live
/// citizen registry the `instances/*` verbs resolve against.
pub fn command_objects(registry: PersonaAircRuntimeRegistry) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(list::PersonaWallList {
            registry: registry.clone(),
        }),
        Arc::new(pin::PersonaWallPin { registry }),
    ]
}
