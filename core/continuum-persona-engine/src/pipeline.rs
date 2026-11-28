//! The active / next / retiring pipeline.

mod roles;
#[cfg(test)]
mod tests;

pub use roles::{BesideRefusal, HandoffRefusal, Identified, Refused, RetireRefusal, Roles, StageRefusal};
