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

    /// Give up the whole unit as its parts (a tier's copy-out takes all of them at once).
    pub fn into_parts(self) -> PersonaParts<B, T, G, L, D> {
        self.parts
    }
}

impl<B, T, G, L, D> Lifecycle for PersonaUnit<B, T, G, L, D>
where
    PersonaParts<B, T, G, L, D>: Lifecycle + Send,
{
    type Suspension = <PersonaParts<B, T, G, L, D> as Lifecycle>::Suspension;
    type Release = <PersonaParts<B, T, G, L, D> as Lifecycle>::Release;

    async fn suspend(&mut self) -> Self::Suspension {
        self.parts.suspend().await
    }

    async fn release(&mut self) -> Self::Release {
        self.parts.release().await
    }
}
