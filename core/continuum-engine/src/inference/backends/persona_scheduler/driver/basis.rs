//! What her blocks were computed under beside her token ids.

use std::sync::Arc;

use continuum_persona_llama::Genes;
use llama::{LoraAdapter, Model};

use crate::inference::backends::persona_scheduler::gene::{signature, Gene};

/// What her blocks were computed under beside the token ids. The model is this context's own, and no
/// native media reaches this path (the prompt is text), so her gene set is the rest: its identity (ids
/// and scales), which is what two bases compare by; the genes themselves, with the files they were
/// loaded from, which are kept with her blocks when she stops; and the adapters while a turn handed
/// them to her or she loaded them again, so her prefix can be prefilled again under exactly them
/// (card 67af8fb2). A mind resumed from disk holds the identity and the genes, not the adapters.
#[derive(Clone, Default)]
pub(super) struct Basis {
    pub(super) genes: String,
    pub(super) sources: Arc<[Gene]>,
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
    /// The basis a turn runs under, and its adapters as the engine applies them.
    pub(super) fn of(turn: &[(Gene, Arc<LoraAdapter>)]) -> (Self, Genes) {
        let adapters: Genes = turn.iter().map(|(gene, adapter)| (Arc::clone(adapter), gene.scale)).collect::<Vec<_>>().into();
        let basis = Self { genes: signature(turn.iter().map(|(gene, _)| gene)), sources: turn.iter().map(|(gene, _)| gene.clone()).collect(), adapters: Some(Arc::clone(&adapters)) };
        (basis, adapters)
    }

    /// The basis of a mind resumed from disk: her genes' identity and the genes themselves, no adapters.
    pub(super) fn resumed(genes: String, sources: Vec<Gene>) -> Self {
        Self { genes, sources: sources.into(), adapters: None }
    }

    /// The adapters to prefill her prefix under: hers, the base model when she has none, or `None` when
    /// only their identity was kept.
    pub(super) fn served_genes(&self) -> Option<Genes> {
        match &self.adapters {
            Some(adapters) => Some(Arc::clone(adapters)),
            None if self.genes.is_empty() => Some(continuum_persona_llama::base_genes()),
            None => None,
        }
    }

    /// Her adapters, loaded again from the files her genes were kept with when only those were kept: a
    /// mind resumed from disk can then learn, or be brought to a lesson's context, before a turn of hers
    /// hands them back. Refused when her genes' files were not kept (a mind stopped before they were) or
    /// no longer name the genes her blocks were computed under: never a prefill under other genes.
    pub(super) fn held_or_loaded(&mut self, model: &Model) -> Result<Genes, String> {
        if let Some(genes) = self.served_genes() {
            return Ok(genes);
        }
        if signature(self.sources.iter()) != self.genes {
            return Err(format!("her genes ({}) were not served, and the files kept with her blocks do not name them", self.genes));
        }
        let mut adapters = Vec::with_capacity(self.sources.len());
        for gene in self.sources.iter() {
            let adapter = model.load_lora(&gene.path).map_err(|why| format!("her gene {} could not be loaded again from {}: {why}", gene.id, gene.path.display()))?;
            adapters.push((Arc::new(adapter), gene.scale));
        }
        let genes: Genes = adapters.into();
        self.adapters = Some(Arc::clone(&genes));
        continuum_base::probe!(class = "persona.engine.genes_loaded", genes = %self.genes, count = self.sources.len() as u64,
            "a resumed mind's genes were loaded again from the files kept with her blocks");
        Ok(genes)
    }
}
