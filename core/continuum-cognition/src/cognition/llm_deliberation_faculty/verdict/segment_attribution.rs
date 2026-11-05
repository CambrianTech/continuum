//! Segment attribution (the rent ledger, compression-ladder rung 2), moved out of `contribute.rs`
//! whole: which prompt segment the engine's KV reuse frontier died in, per turn.

use super::*;

impl LlmDeliberationFaculty {
    /// Name the segment of `view` the server's reused prefix ended in, and record her turn's shape.
    pub(super) fn attribute_segments(&self, view: &DeliberationPromptView, resp: &TextGenerationResponse) {
        // SEGMENT ATTRIBUTION (the rent ledger, compression-ladder rung 2):
        // the server's reused-prefix length lands somewhere in the prompt's
        // segment map — the run containing that frontier NAMES what broke
        // reuse this turn, and the stream of these rows IS each segment's
        // rent. Only when the lane measured timings (same honesty rule as
        // delib.generate.cache: a fabricated attribution is a lying receipt).
        if let Some(t) = resp.timing.as_ref() {
            let cached = t.cached_tokens;
            // Her last turn's shape — what the next work call's occupancy bound is read from.
            LAST_TURN_SHAPE.insert(
                self.persona_id,
                TurnShape {
                    input: resp.usage.input_tokens,
                    cached,
                    output: resp.usage.output_tokens,
                },
            );
            let frontier = view
                .segments
                .iter()
                .find(|(_, end)| *end > cached)
                .map(|(label, _)| *label)
                .unwrap_or("fully-warm"); // reuse extends past every mapped segment — nothing broke
            let mut map = String::with_capacity(64);
            for (label, end) in &view.segments {
                use std::fmt::Write as _;
                let _ = write!(map, "{label}:{end},");
            }
            continuum_base::probe!(
                class = "delib.segment.attribution",
                persona = %self.persona_name,
                cached_tokens = cached as u64,
                broke_in = %frontier,
                cold = cached == 0,
                segments = %map,
                "which prompt segment the KV reuse frontier died in — every miss becomes \
                 an attribution, and the stream is the rent ledger the dream curriculum reads",
            );
        }
    }
}
