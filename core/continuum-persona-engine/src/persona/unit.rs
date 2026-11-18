//! [`PersonaUnit`]: her five parts as one value. A move takes all of them or none.

use super::{Epoch, PersonaId};

use super::{Lifecycle, PersonaParts};

/// A part was missing from a move: the unit is refused whole, never assembled from pieces.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("persona {persona:?}: refused a partial move, missing {missing}")]
pub struct PartialMove {
    pub persona: PersonaId,
    pub missing: &'static str,
}

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

    /// Assemble a unit from parts that arrived separately (a tier's copy-in): every part or a
    /// typed refusal naming the first one missing.
    pub fn assemble(
        id: PersonaId,
        epoch: Epoch,
        blocks: Option<B>,
        tokens: Option<T>,
        genome: Option<G>,
        learning: Option<L>,
        decode: Option<D>,
    ) -> Result<Self, PartialMove> {
        let missing = |part| PartialMove { persona: id, missing: part };
        Ok(Self::new(
            id,
            epoch,
            PersonaParts {
                blocks: blocks.ok_or_else(|| missing("execution blocks"))?,
                tokens: tokens.ok_or_else(|| missing("token history"))?,
                genome: genome.ok_or_else(|| missing("genome generation"))?,
                learning: learning.ok_or_else(|| missing("learning custody"))?,
                decode: decode.ok_or_else(|| missing("decode state"))?,
            },
        ))
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

    async fn suspend(&mut self) -> Self::Suspension {
        self.parts.suspend().await
    }
}
