//! What her blocks were computed under beside her token ids.

use std::sync::Arc;

use continuum_persona_llama::Genes;

/// What her blocks were computed under beside the token ids. The model is this context's own, and no
/// native media reaches this path (the prompt is text), so her gene set is the rest: its identity (ids
/// and scales), which is what two bases compare by, and the adapters themselves while a turn handed them
/// to her, so her prefix can be prefilled again under exactly them (card 67af8fb2). A mind resumed from
/// disk holds only the identity.
#[derive(Clone, Default)]
pub(super) struct Basis {
    pub(super) genes: String,
    pub(super) adapters: Option<Genes>,
}

impl PartialEq for Basis {
    fn eq(&self, other: &Self) -> bool {
        self.genes == other.genes
    }
}

impl Eq for Basis {}

impl std::fmt::Debug for Basis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Basis({:?})", self.genes)
    }
}

impl Basis {
    /// The adapters to prefill her prefix under: hers, the base model when she has none, or `None` when
    /// only their identity was kept.
    pub(super) fn served_genes(&self) -> Option<Genes> {
        match &self.adapters {
            Some(adapters) => Some(Arc::clone(adapters)),
            None if self.genes.is_empty() => Some(continuum_persona_llama::base_genes()),
            None => None,
        }
    }
}

