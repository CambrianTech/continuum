//! One engine step for several bound minds at once (THE-LANE-ON-SAGE.md, open question 1, answered): each mind's
//! tokens at her own positions on the stream she is bound to, decoded together, so the model's weights are read
//! once for all of them. Measured on the M5 (Qwen3.6-35B-A3B, 8k each, 2026-11-28 04:37Z, llama-batched-bench): one
//! mind 66.6 tok/s, four in one step 139.0. A single mind's step is a batch of one: there is one decode path.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::{Execute, Native, TierRefusal};
use llama::{LoraAdapter, Pos};

use super::{refused, stream_at, Decoded, Genes, Step, UnifiedTier, TIER};

/// Whether two gene sets are the same adapters at the same scales: what one decode of several minds needs, since
/// the engine applies one set to the whole context.
fn same_genes(a: &Genes, b: &Genes) -> bool {
    Arc::ptr_eq(a, b) || (a.len() == b.len() && a.iter().zip(b.iter()).all(|((x, sx), (y, sy))| Arc::ptr_eq(x, y) && sx == sy))
}

impl UnifiedTier<'_, '_> {
    /// Decode `steps` in one engine step: each her tokens from her `from` on the stream she is bound to. The
    /// decode's outputs are the steps that asked for logits at their end, in order: output `i` is the `i`-th of
    /// them. Refused, decoding nothing, when a mind is not bound, the steps wear different genes, a step is empty,
    /// or together they are longer than the engine's batch.
    pub fn submit_batch(&mut self, steps: &[(MindId, &Step)]) -> Result<Decoded, TierRefusal> {
        let Some(&(first, lead)) = steps.first() else { return Ok(self.landed_now()) };
        // A step at `from` replaces whatever she held from there (a re-decoded last token, a restart at 0, a
        // failed turn's partial tail), so her stream never holds two tokens at one position.
        let mut total = 0usize;
        for &(mind, step) in steps {
            if !same_genes(&step.genes, &lead.genes) {
                return Err(refused(mind, "the minds of one step wear different genes"));
            }
            let stream = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?;
            if step.tokens.as_slice().is_empty() {
                return Err(refused(mind, "an empty step"));
            }
            total = total.saturating_add(step.tokens.as_slice().len());
            if self.ctx.memory_seq_pos_max(stream.seq()).is_some_and(|held| held >= step.from) {
                self.settle()?;
                if !self.ctx.memory_seq_truncate(stream.seq(), step.from) {
                    return Err(refused(mind, "the engine could not drop her positions"));
                }
            }
        }
        if u32::try_from(total).map_or(true, |total| total > self.ctx.n_batch()) {
            return Err(refused(first, "a step longer than the engine's batch"));
        }
        if !self.applied_genes.as_ref().is_some_and(|applied| same_genes(applied, &lead.genes)) {
            let genes: Vec<(&LoraAdapter, f32)> = lead.genes.iter().map(|(adapter, scale)| (adapter.as_ref(), *scale)).collect();
            let applied = if genes.is_empty() { self.ctx.clear_loras() } else { self.ctx.set_loras(&genes) };
            applied.map_err(|message| TierRefusal { tier: TIER, mind: first, reason: "the engine refused her genes", native: Native::Reported(message.into()) })?;
            self.applied_genes = Some(Arc::clone(&lead.genes));
        }
        // one batch, sized to the engine's and refilled each step: never allocated per step
        self.batch.clear();
        for &(mind, step) in steps {
            let seq = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?.seq();
            let tokens = step.tokens.as_slice();
            let last = tokens.len() - 1; // checked non-empty above
            for (i, &token) in tokens.iter().enumerate() {
                let pos = step.from.advanced(i).map_err(|_| refused(mind, "a step past what a sequence can address"))?;
                self.batch.push(token, pos, &[seq], step.logits_at_end && i == last);
            }
        }
        self.ctx.decode(&self.batch).map_err(|message| {
            // The refusal says what the engine held when it refused (the M5, 2026-11-21: a cold 26-36k replay
            // prefill refused with no KV slot ~31 s in; the cause is not yet known).
            let occupied: Vec<(usize, Option<MindId>, Option<Pos>)> =
                self.streams.iter().enumerate().map(|(at, held)| (at, *held, self.ctx.memory_seq_pos_max(stream_at(at).seq()))).collect();
            continuum_base::probe!(
                class = "persona.engine.decode_refused",
                persona = ?first,
                minds = steps.len(),
                from = lead.from.get(),
                tokens = total,
                n_ctx = self.ctx.n_ctx(),
                n_ctx_seq = self.ctx.n_ctx_seq(),
                n_seq_max = self.ctx.n_seq_max(),
                n_batch = self.ctx.n_batch(),
                n_ubatch = self.ctx.n_ubatch(),
                streams = ?occupied,
                native = %message,
                "the engine refused a decode: what it held then"
            );
            TierRefusal { tier: TIER, mind: first, reason: "the engine refused her decode", native: Native::Reported(message.into()) }
        })?;
        let landed = Rc::new(Cell::new(false));
        self.in_flight = Some(Rc::clone(&landed));
        Ok(Decoded { landed })
    }

    /// A completion that has landed: nothing was submitted.
    fn landed_now(&self) -> Decoded {
        Decoded { landed: Rc::new(Cell::new(true)) }
    }
}
