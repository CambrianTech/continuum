//! The active / next / retiring pipeline.

mod roles;
#[cfg(test)]
mod tests;

pub use roles::{HandoffRefusal, Identified, RetireRefusal, Roles, StageRefusal};
