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
    let mut cum = est_tokens(system);
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
        cum += est_tokens(&body) + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS;
        match runs.last_mut() {
            Some((l, end)) if *l == label => *end = cum as u32,
            _ => runs.push((label, cum as u32)),
        }
    }
    runs
}
