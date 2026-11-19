//! A persona's execution state, owned once and moved only whole.

mod footprint;
mod id;
mod lifecycle;
mod unit;

pub use footprint::{Footprint, Held};
pub use id::{Epoch, PersonaId};
pub use lifecycle::{Lifecycle, PersonaParts, Preservation, Suspension};
pub use unit::PersonaUnit;

#[cfg(test)]
mod lifecycle_tests;
