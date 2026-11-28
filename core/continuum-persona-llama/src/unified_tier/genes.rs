//! The genes each stream wears (fork #172, llama-adapter-seq.h). A context applied one adapter set to every row of a
//! step, so minds could share a step only while they wore the same genes; on the M5 every resident wore her own and
//! a turn chose a batch 85 times and batched nothing (2026-11-28). Now each stream's sequence names the genes of the
//! mind bound to it, and one step serves minds under different genes, each adapter on her own rows. A stream names
//! hers when she first steps under a set it does not wear, so in practice once per bind, and gives them up when she
//! moves out.

use std::sync::Arc;

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::TierRefusal;
use llama::{LoraAdapter, SeqGenesRefusal};

use super::{refused, Genes, Stream, UnifiedTier};

/// Whether two gene sets are the same adapters at the same scales.
fn same_genes(a: &Genes, b: &Genes) -> bool {
    Arc::ptr_eq(a, b) || (a.len() == b.len() && a.iter().zip(b.iter()).all(|((x, sx), (y, sy))| Arc::ptr_eq(x, y) && sx == sy))
}

impl UnifiedTier<'_, '_> {
    /// `mind`'s stream wears `genes` from her next decode: named on its sequence unless it wears them already.
    pub(super) fn wear(&mut self, mind: MindId, stream: Stream, genes: &Genes) -> Result<(), TierRefusal> {
        let Some(worn) = self.worn(stream) else { return Err(refused(mind, "her stream is outside the context")) };
        if worn.as_ref().is_some_and(|worn| same_genes(worn, genes)) {
            return Ok(());
        }
        let named: Vec<(&LoraAdapter, f32)> = genes.iter().map(|(adapter, scale)| (adapter.as_ref(), *scale)).collect();
        self.ctx.name_seq_genes(stream.seq(), &named).map_err(|refusal| match refusal {
            SeqGenesRefusal::SequenceOutside { .. } => refused(mind, "her stream is outside the context"),
            SeqGenesRefusal::NotOnModel => refused(mind, "her genes name an adapter not loaded on this model"),
            SeqGenesRefusal::Unknown { .. } => refused(mind, "the engine refused her genes"),
        })?;
        if let Some(worn) = self.worn(stream) {
            *worn = Some(Arc::clone(genes));
        }
        Ok(())
    }

    /// `stream` gives up the genes it named: it wears the context's set until a mind names hers.
    pub(super) fn forget_stream_genes(&mut self, stream: Stream) {
        self.ctx.clear_seq_genes(stream.seq());
        if let Some(worn) = self.worn(stream) {
            *worn = None;
        }
    }

    /// What `stream` wears, as the tier named it; `None` for a stream outside the context.
    fn worn(&mut self, stream: Stream) -> Option<&mut Option<Genes>> {
        usize::try_from(stream.seq()).ok().and_then(|at| self.stream_genes.get_mut(at))
    }
}
