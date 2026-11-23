//! Typed identities: a persona and the epoch her state belongs to.

/// One persona, by her citizen id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PersonaId(pub uuid::Uuid);

/// The generation of her state: a new epoch starts her blocks over (a new model, a revocation),
/// so state from another epoch is never bound as hers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Epoch(pub u64);

/// One mind: her persona and the epoch of her state. Every owner names a mind by this, never a
/// persona and an epoch carried apart (CONTINUUM-ENGINE 4.17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MindId {
    pub persona: PersonaId,
    pub epoch: Epoch,
}
