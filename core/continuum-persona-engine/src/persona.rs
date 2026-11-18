//! A persona's execution state, owned once and moved only whole.

mod id;
mod unit;

pub use id::{Epoch, PersonaId};
pub use unit::{PersonaParts, PersonaUnit, PartialMove};
