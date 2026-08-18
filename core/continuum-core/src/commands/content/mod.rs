//! `content/*` — dereferencing content that stayed at its source.
//!
//! See [docs/architecture/CONTENT-TRAVELS-BY-HANDLE.md] and [`crate::content`].
//! Oversized content is never cut down and handed over; it is parked by its producer and
//! reached through a handle. This module is where a citizen calls that handle.

pub mod fetch;

use std::sync::Arc;

use crate::content::ContentRegistry;
use crate::sdk_codegen::DynCommand;

/// The dep-holding `content/*` command objects
/// [`ContentModule`](crate::modules::content::ContentModule) contributes, sharing the one
/// [`ContentRegistry`] every producer publishes into.
pub fn command_objects(registry: Arc<ContentRegistry>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(fetch::ContentFetch { registry })]
}
