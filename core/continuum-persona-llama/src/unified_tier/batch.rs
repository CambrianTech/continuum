//! One engine step for several bound minds at once (THE-LANE-ON-SAGE.md, open question 1, answered): each mind's
//! tokens at her own positions on the stream she is bound to, decoded together, so the model's weights are read
//! once for all of them. Measured on the M5 (Qwen3.6-35B-A3B, 8k each, 2026-11-28 04:37Z, llama-batched-bench): one
//! mind 66.6 tok/s, four in one step 139.0. A single mind's step is a batch of one: there is one decode path.
//!
//! Each mind wears her own genes on her own rows (fork #172, `genes.rs`), so the minds of one step need not share
//! them. Her rows go to the engine in stream order: with a stream per mind (kv_unified off) the engine takes a
//! ubatch's sequences only in consecutive increasing ids and splits a step out of that order into several, each
//! building its own graph (fork #172, measured). The caller's token indices are kept: `logits_ith` maps them.

use std::cell::Cell;
use std::rc::Rc;

use continuum_persona_engine::persona::MindId;
use continuum_persona_engine::residency::{Execute, Native, TierRefusal};
use llama::Pos;

use super::{refused, stream_at, Decoded, Step, UnifiedTier, TIER};

/// A step's rows in the engine's order, kept across steps so a step allocates nothing.
#[derive(Default)]
pub(super) struct RowOrder {
    /// The steps, by their streams' order.
    by_stream: Vec<usize>,
    /// Where each step's tokens start in the caller's order.
    caller_from: Vec<usize>,
    /// The engine's batch index of each of the caller's tokens.
    engine_row: Vec<i32>,
}

impl RowOrder {
    /// The engine's batch index of the caller's token `i` (negative counts from the end, as the engine's does).
    pub(super) fn engine_index(&self, i: i32) -> i32 {
        usize::try_from(i).ok().and_then(|at| self.engine_row.get(at).copied()).unwrap_or(i) // unwrap_or: a negative index is the engine's own
    }
}

impl UnifiedTier<'_, '_> {
    /// Decode `steps` in one engine step: each her tokens from her `from` on the stream she is bound to, under her
    /// own genes. The caller's token `i` (its steps' tokens in the order given) is read back with `logits_ith(i)`.
    /// Refused, decoding nothing, when a mind is not bound, a step is empty, or together they are longer than the
    /// engine's batch.
    pub(super) fn decode_steps(&mut self, steps: &[(MindId, &Step)]) -> Result<Decoded, TierRefusal> {
        let Some(&(first, lead)) = steps.first() else { return Ok(self.landed_now()) };
        // A step at `from` replaces whatever she held from there (a re-decoded last token, a restart at 0, a
        // failed turn's partial tail), so her stream never holds two tokens at one position.
        let mut total = 0usize;
        for &(mind, step) in steps {
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
            self.wear(mind, stream, &step.genes)?;
        }
        if u32::try_from(total).map_or(true, |total| total > self.ctx.n_batch()) {
            return Err(refused(first, "a step longer than the engine's batch"));
        }
        self.order_rows(steps);
        // one batch, sized to the engine's and refilled each step: never allocated per step
        self.batch.clear();
        let mut row = 0i32; // the engine's batch index of the next token pushed; the total fits the batch (checked above)
        for &at in &self.order.by_stream {
            let (mind, step) = steps[at];
            let seq = self.stream_of(mind).ok_or_else(|| refused(mind, "not bound"))?.seq();
            let tokens = step.tokens.as_slice();
            let last = tokens.len() - 1; // checked non-empty above
            for (i, &token) in tokens.iter().enumerate() {
                let pos = step.from.advanced(i).map_err(|_| refused(mind, "a step past what a sequence can address"))?;
                self.order.engine_row[self.order.caller_from[at] + i] = row;
                self.batch.push(token, pos, &[seq], step.logits_at_end && i == last);
                row += 1;
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

    /// `steps` in their streams' order, and where each step's tokens sit in the caller's order. Every mind of
    /// `steps` is bound (checked by the caller).
    fn order_rows(&mut self, steps: &[(MindId, &Step)]) {
        let (streams, order) = (&self.streams, &mut self.order);
        order.caller_from.clear();
        let mut from = 0usize;
        for &(_, step) in steps {
            order.caller_from.push(from);
            from += step.tokens.as_slice().len();
        }
        order.engine_row.clear();
        order.engine_row.resize(from, 0);
        order.by_stream.clear();
        order.by_stream.extend(0..steps.len());
        order.by_stream.sort_unstable_by_key(|&at| streams.iter().position(|held| *held == Some(steps[at].0)));
    }

    /// A completion that has landed: nothing was submitted.
    fn landed_now(&self) -> Decoded {
        Decoded { landed: Rc::new(Cell::new(true)) }
    }
}
