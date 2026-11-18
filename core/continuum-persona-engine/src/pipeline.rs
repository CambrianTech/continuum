//! The active / next / retiring pipeline.

mod roles;
#[cfg(test)]
mod tests;

pub use roles::{HandoffRefusal, Identified, Refused, RetireRefusal, Roles, StageRefusal};
