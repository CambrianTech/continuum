//! A persona's execution state, owned once and moved only whole.

mod id;
mod parts;
mod unit;

pub use id::{Epoch, PersonaId};
pub use parts::PersonaParts;
pub use unit::PersonaUnit;

