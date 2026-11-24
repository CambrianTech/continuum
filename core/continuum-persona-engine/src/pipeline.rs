//! The seated / active / next / retiring pipeline.

mod roles;
#[cfg(test)]
mod tests;

pub use roles::{Identified, Refused, RetireRefusal, Roles, SeatRefusal, SeatedRefusal, StageRefusal};
