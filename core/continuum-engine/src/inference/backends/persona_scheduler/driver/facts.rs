//! What a turn reports: its facts for her caller and its one probe row.

use super::*;

impl Driver<'_, '_> {
    /// One row per turn: where her seconds went (waiting for her first frame, switches, prefill of
    /// what her blocks did not hold, decode), so the one-mind gate (switch in ms, decode near the
    /// chip's rate, reuse >= 80%, no turn waiting behind another's whole prompt) is read live. Built
    /// from her counters here; written by Record.
    /// What her caller is told the Core measured of her answered turn: the same measurements her row
    /// records, so the response never estimates a count the Core already has.
    pub(super) fn facts(turn: &Turn, generated: usize, end: super::super::TurnEnd) -> super::super::TurnFacts {
        super::super::TurnFacts {
            prompt_tokens: turn.prompt.len(),
            reused_tokens: turn.spent.reused,
            generated,
            prefill: turn.spent.prefill,
            decode: turn.spent.decode,
            elapsed: turn.submitted.elapsed(),
            end,
        }
    }

    pub(super) fn row(turn: &Turn, generated: usize) -> record::TurnRow {
        let ms = record::whole_ms;
        record::TurnRow {
            request: turn.req.request.clone(),
            persona: turn.id.0,
            one_off: turn.one_off,
            prompt_tokens: turn.prompt.len() as u64,
            reused_tokens: turn.spent.reused as u64,
            shared_prefix_tokens: turn.spent.shared_prefix_tokens as u64,
            start: turn.spent.start,
            boundary: turn.boundaries.view.map(|end| end as u64),
            learned_boundary: turn.boundaries.learned.map(|end| end as u64),
            checkpoint_ms: turn.spent.checkpoint_ms,
            wait_ms: turn.spent.first_step.map(ms),
            longest_service_gap_ms: ms(turn.spent.longest_gap),
            switches: turn.spent.switches,
            switch_ms: ms(turn.spent.switch),
            prefill_ms: ms(turn.spent.prefill),
            decode_ms: ms(turn.spent.decode),
            forward_ms: ms(turn.spent.forward),
            graphs_reused: turn.spent.graphs_reused,
            gpu_ms: ms(turn.spent.gpu),
            sample_ms: ms(turn.spent.sample),
            generated: generated as u64,
            total_ms: ms(turn.submitted.elapsed()),
        }
    }

}
