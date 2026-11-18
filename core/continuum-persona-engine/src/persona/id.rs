//! Typed identities: a persona and the epoch her state belongs to.

/// One persona, by her citizen id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PersonaId(pub uuid::Uuid);

/// The generation of her state: a new epoch starts her blocks over (a new model, a revocation),
/// so state from another epoch is never bound as hers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Epoch(pub u64);
