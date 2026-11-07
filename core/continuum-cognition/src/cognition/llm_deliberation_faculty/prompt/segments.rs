use super::*;

/// Build the segment map for a rendered prompt — the rent ledger's spine
/// ([`DeliberationPromptView::segments`]). Labels are STRUCTURAL (no model
/// call, zero copies of content): the system head, then message runs
/// classified by shape — her own `assistant` turns and peers' plain turns are
/// `history`; bracketed blocks and act results (`[recall]`, `[context]`,
/// `[pattern]`, `[now …]`, `Full result of …`, `⚙`) are `grounding`.
/// Consecutive same-label messages merge into one run, so the map stays a
/// handful of entries however long the thread.
pub(in super::super) fn segment_map(system: &str, messages: &[ChatMessage]) -> Vec<(&'static str, u32)> {
    let mut runs: Vec<(&'static str, u32)> = Vec::with_capacity(8);
    let mut cum = estimate_guard_tokens(system);
    runs.push(("system", cum as u32));
    for m in messages {
        let body = m.content_text();
        // Occurrence-time framing belongs to speech history, not grounding.
        let dated_history = body.starts_with("[occurred ")
            || body.starts_with("[occurrence time unknown] ");
        let label = if !dated_history
            && (body.starts_with('[')
                || body.starts_with("Full result of")
                || body.starts_with('⚙'))
        {
            "grounding"
        } else {
            "history"
        };
        cum += estimate_guard_tokens(&body);
        match runs.last_mut() {
            Some((l, end)) if *l == label => *end = cum as u32,
            _ => runs.push((label, cum as u32)),
        }
    }
    runs
}

/// Segment attribution (the rent ledger, compression-ladder rung 2): which segment of the map above
/// the engine's KV reuse frontier died in, per turn (moved here whole from `verdict/contribute.rs`).

impl LlmDeliberationFaculty {
    /// Name the segment of `view` the server's reused prefix ended in, and record her turn's shape.
    pub(in super::super) fn attribute_segments(&self, view: &DeliberationPromptView, resp: &TextGenerationResponse) {
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
