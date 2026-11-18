//! A persona's execution state, owned once and moved only whole.

mod id;
mod lifecycle;
mod unit;

pub use id::{Epoch, PersonaId};
pub use lifecycle::{Lifecycle, PersonaParts, Preservation, Releasing, Report};
pub use unit::PersonaUnit;

#[cfg(test)]
mod lifecycle_tests;
