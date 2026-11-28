//! Each sequence's own adapter set (the fork's llama-adapter-seq.h): one decode serves minds wearing different
//! genes, each adapter's delta added to her own rows only. A context applied one set to every row, so the M5's
//! residents (each her own genes) could share a step only while they wore the same: a live turn chose a batch 85
//! times and batched nothing (2026-11-28). A sequence names her set once, when her mind is bound to it.

use super::{Context, LoraAdapter};
use crate::sys;

/// Why a sequence's genes were not named. Nothing changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SeqGenesRefusal {
    #[error("sequence {seq} is outside the context")]
    SequenceOutside { seq: i32 },
    #[error("an adapter is not loaded on the context's model")]
    NotOnModel,
    #[error("the engine refused the set with code {code}")]
    Unknown { code: i32 },
}

impl Context<'_> {
    /// Name the genes sequence `seq` wears: each adapter with its scale; none is the base model. Decodes after this
    /// add each adapter's delta to `seq`'s rows only, whatever the other rows of the step wear.
    pub fn name_seq_genes(&mut self, seq: i32, genes: &[(&LoraAdapter, f32)]) -> Result<(), SeqGenesRefusal> {
        let mut adapters: Vec<*mut sys::llama_adapter_lora> = genes.iter().map(|(adapter, _)| adapter.ptr.as_ptr()).collect();
        let scales: Vec<f32> = genes.iter().map(|(_, scale)| *scale).collect();
        // SAFETY: called between decodes on the context's own thread; the adapters are loaded on this context's model
        // (the fork checks, refusing -2) and the arrays live across the call, which copies them.
        let code = unsafe { sys::llama_set_seq_adapters_lora(self.ptr.as_ptr(), seq, adapters.as_mut_ptr(), adapters.len(), scales.as_ptr()) };
        match code {
            0 => Ok(()),
            -1 => Err(SeqGenesRefusal::SequenceOutside { seq }),
            -2 => Err(SeqGenesRefusal::NotOnModel),
            code => Err(SeqGenesRefusal::Unknown { code }),
        }
    }

    /// Return sequence `seq` to the context's set: when her mind leaves it.
    pub fn clear_seq_genes(&mut self, seq: i32) {
        // SAFETY: as name_seq_genes; it erases one entry of the context's table.
        unsafe { sys::llama_clear_seq_adapters_lora(self.ptr.as_ptr(), seq) }
    }

    /// The ubatches whose sequences' genes interleaved, so their rows were gathered instead of viewed: the fast
    /// path's misses.
    pub fn seq_genes_gathered(&self) -> u64 {
        // SAFETY: a read of the context's own counter.
        unsafe { sys::llama_seq_adapters_gathered(self.ptr.as_ptr()) }
    }
}
