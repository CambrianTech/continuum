//! [`PersonaUnit`]: her five parts as one value. A partial unit cannot be built: the only
//! constructor takes a complete [`PersonaParts`], so a move is whole by type.

use super::{Epoch, Lifecycle, PersonaId, PersonaParts};

/// One persona's execution state. No part of her is held anywhere else: a slot's KV, a cycle's
/// gene set, a learner's pass are views of this unit, never copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaUnit<Blocks, Tokens, Genome, Learning, Decode> {
    id: PersonaId,
    epoch: Epoch,
    parts: PersonaParts<Blocks, Tokens, Genome, Learning, Decode>,
}

impl<B, T, G, L, D> PersonaUnit<B, T, G, L, D> {
    pub fn new(id: PersonaId, epoch: Epoch, parts: PersonaParts<B, T, G, L, D>) -> Self {
        Self { id, epoch, parts }
    }

    pub fn id(&self) -> PersonaId {
        self.id
    }

    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    pub fn parts(&self) -> &PersonaParts<B, T, G, L, D> {
        &self.parts
    }

    /// Her parts, to change in place: her history grows, her checkpoint is replaced, her blocks move.
    /// Her identity is not among them; a new epoch is a new unit.
    pub fn parts_mut(&mut self) -> &mut PersonaParts<B, T, G, L, D> {
        &mut self.parts
    }

    /// Her parts, whole, as this generation of her ends (a stop hands her blocks' files to the next
    /// start): the unit is consumed, so no part of her is left behind to be used after.
    pub fn into_parts(self) -> PersonaParts<B, T, G, L, D> {
        self.parts
    }
}

/// A unit that carries her own blocks: the tier moves the blocks it is handed and holds no copy of
/// them (CONTINUUM-ENGINE §2). The persona owns this, and residency depends on it, never the reverse.
pub trait Carries<B> {
    fn blocks_mut(&mut self) -> &mut B;
}

/// A unit carries her own blocks: the tier is handed them to move.
impl<B, T, G, L, D> Carries<B> for PersonaUnit<B, T, G, L, D> {
    fn blocks_mut(&mut self) -> &mut B {
        &mut self.parts.blocks
    }
}

impl<B, T, G, L, D> Lifecycle for PersonaUnit<B, T, G, L, D>
where
    PersonaParts<B, T, G, L, D>: Lifecycle + Send,
{
    type Suspension = <PersonaParts<B, T, G, L, D> as Lifecycle>::Suspension;

    async fn suspend(&mut self) -> Self::Suspension {
        self.parts.suspend().await
    }
}
