//! The algorithms of one use case, by name (OpenCV's `Algorithm::create`): a caller names the one it
//! wants and gets it built, never a type wired into its code. A new algorithm registers itself; no
//! caller changes.

use super::policy::Policy;
use super::use_case::UseCase;

/// What builds one algorithm.
pub type Factory<U> = fn() -> Box<dyn Policy<U>>;

/// No algorithm by this name is registered for this use case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unknown {
    pub use_case: &'static str,
    pub name: &'static str,
}

/// The registry of one use case's algorithms.
pub struct Algorithms<U: UseCase> {
    entries: Vec<(&'static str, Factory<U>)>,
}

impl<U: UseCase> Default for Algorithms<U> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

impl<U: UseCase> Algorithms<U> {
    /// Register `factory` under `name`. A second registration under one name replaces nothing: the
    /// first stands and the second is refused, so a name always means one algorithm.
    pub fn register(mut self, name: &'static str, factory: Factory<U>) -> Result<Self, Unknown> {
        if self.entries.iter().any(|(known, _)| *known == name) {
            return Err(Unknown { use_case: U::NAME, name });
        }
        self.entries.push((name, factory));
        Ok(self)
    }

    /// Build the algorithm registered under `name`.
    pub fn create(&self, name: &'static str) -> Result<Box<dyn Policy<U>>, Unknown> {
        self.entries.iter().find(|(known, _)| *known == name).map(|(_, factory)| factory()).ok_or(Unknown { use_case: U::NAME, name })
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.iter().map(|(name, _)| *name)
    }
}
