//! What a loader must match before any part is read, and what a host offers.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::PackError;
use crate::id::ContentId;

/// Who may open a sealed part: the digests of the public keys it is sealed to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Audience {
    pub keys: BTreeSet<ContentId>,
}

/// One thing a package needs from the host that loads it. Checked from the manifest, before
/// any blob is read; an unmet one is a typed refusal, never a degraded load.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Requirement {
    /// The exact base weights, by content.
    Model(ContentId),
    /// A KV layout produced by `model` in the engine's named cache `format`.
    KvFormat { model: ContentId, format: String },
    /// The exact embedder whose vectors a part holds.
    Embedder(ContentId),
    /// A key of this audience, to open the package's sealed parts.
    Key(Audience),
}

/// What a host can satisfy. A requirement is met when it is offered exactly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Offers(BTreeSet<Requirement>);

impl Offers {
    /// A host that offers nothing (only packages with no requirements load).
    pub fn none() -> Self {
        Self(BTreeSet::new())
    }

    /// This host also offers `requirement`.
    pub fn with(mut self, requirement: Requirement) -> Self {
        self.0.insert(requirement);
        self
    }

    /// Whether `requirement` is met here.
    pub fn satisfies(&self, requirement: &Requirement) -> bool {
        self.0.contains(requirement)
    }
}

/// A field type that is stored as a requirement (`#[pack(requires)]`): it converts to the
/// requirement it names and back, so the requirement is recorded once.
pub trait Required: Sized {
    fn to_requirement(&self) -> Requirement;
    fn from_requirement(part: &str, requirement: &Requirement) -> Result<Self, PackError>;
}

impl Required for Requirement {
    fn to_requirement(&self) -> Requirement {
        self.clone()
    }

    fn from_requirement(_part: &str, requirement: &Requirement) -> Result<Self, PackError> {
        Ok(requirement.clone())
    }
}
