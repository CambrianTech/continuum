//! A turn's measured shape: occupancy, the turn bound, the latency fill cap, and the
//! metrics and wire capture each generation leaves behind.

use super::*;

impl LlmDeliberationFaculty {
    /// Does this workspace name a card she HOLDS (Claimed or InProgress)? The lane
    /// gate's key for `LanePriority::Work` — wider than `holds_live_work`, which keys
    /// the working-presence contract on InProgress only.
    /// This mind's next call's bound on the lane: prefill of the prompt she did not have
    /// cached last turn at this box's measured prefill rate, plus her last output at its
    /// measured decode rate. Read from her LAST turn's shape (`LAST_TURN_SHAPE`) — the
    /// prompt is rendered after the gate, so the previous turn is the honest predictor.
    /// `None` when any term is unmeasured: an absence is not a number, and the reserve is
    /// then not lent (`resource_admission::reserve_lendable`).
    pub(super) fn expected_occupancy(&self) -> Option<std::time::Duration> {
        expected_occupancy_for(self.persona_id)
    }

    /// The bound this turn carries on the wire (`TextGenerationRequest::turn_bound`):
    /// her expected occupancy with headroom (`inference::turn_bound`). Every waiting
    /// seam — the header wait, the stream's queue budget, the remote deadline — takes
    /// `max(its floor, this)`, so a 30k prompt on a 25 tok/s box (~1200 s to the first
    /// byte) is waited out, never read as dead (card ba82d0a0). `None` until her first
    /// measured turn on a measured box: the floors govern alone, an absence is not a
    /// number.
    pub(super) fn turn_bound(&self) -> Option<std::time::Duration> {
        let expected = self.expected_occupancy();
        let bound = crate::inference::turn_bound::from_expectation(expected);
        if let (Some(expected), Some(bound)) = (expected, bound) {
            crate::probe!(
                class = "delib.turn.bound",
                persona = %self.persona_name,
                expected_secs = expected.as_secs(),
                turn_bound_secs = bound.as_secs(),
                "the turn's wire bound — her measured expectation with headroom"
            );
        }
        bound
    }

    /// The conversation fill this turn may cost, and the rate it was derived from.
    ///
    /// `PREFILL_TARGET_SECONDS` is the latency INTENT; the tokens it buys are THIS
    /// node's, read through the same ladder the decode side landed (#4283) — fresh, else
    /// stale, else the box's most conservative measured rate, and only with nothing ever
    /// measured a named floor ([`crate::inference::prefill_rate::latency_fill_cap`]).
    /// Use the captured request binding: a degraded serving snapshot may temporarily
    /// clear active_model while this same model still owns the pending turn.
    pub(super) fn latency_fill_cap(
        model: Option<&str>,
        rate_for: impl FnOnce(&str) -> crate::inference::prefill_rate::MeasuredRate,
    ) -> (usize, crate::inference::prefill_rate::MeasuredRate) {
        let rate = model.map(rate_for).unwrap_or(crate::inference::prefill_rate::MeasuredRate::UNKNOWN); // No bound model means no attributable rate; retain the existing unmeasured allowance.
        (
            crate::inference::prefill_rate::latency_fill_cap(PREFILL_TARGET_SECONDS, rate),
            rate,
        )
    }
}

/// Build the segment map for a rendered prompt — the rent ledger's spine
/// ([`DeliberationPromptView::segments`]). Labels are STRUCTURAL (no model
/// call, zero copies of content): the system head, then message runs
/// classified by shape — her own `assistant` turns and peers' plain turns are
/// `history`; bracketed blocks and act results (`[recall]`, `[context]`,
/// `[pattern]`, `[now …]`, `Full result of …`, `⚙`) are `grounding`.
/// Consecutive same-label messages merge into one run, so the map stays a
/// handful of entries however long the thread.
pub(super) fn segment_map(system: &str, messages: &[ChatMessage]) -> Vec<(&'static str, u32)> {
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

/// Lift the speed/latency cost of one generation off the adapter response — the
/// measured wall-clock + the prompt/completion token counts. The brain stamps this
/// onto its verdict [`Contribution`] so latency and throughput leave the mind on
/// the same path as the decision, and the settle loop folds it into the per-task
/// total. Token counts are 0 when the gateway omitted `usage` (older endpoints);
/// `latency_ms` is always present (the adapter times every request).
/// A mind's last generation, as the next work call's occupancy bound reads it.
#[derive(Debug, Clone, Copy)]
pub(super) struct TurnShape {
    pub(super) input: u32,
    pub(super) cached: u32,
    pub(super) output: u32,
}
pub(super) static LAST_TURN_SHAPE: std::sync::LazyLock<dashmap::DashMap<uuid::Uuid, TurnShape>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// A MIND'S MEASURED TURN, READABLE FROM OUTSIDE HER FACULTY (card ebce2ba0). The same
/// number [`LlmDeliberationFaculty::expected_occupancy`] serves — her last turn's shape at
/// this box's measured prefill and decode rates — exposed so the seams ABOVE the faculty
/// can size their own bounds from it instead of from a constant. The act deadline
/// (`act_observe::settle`) is the first such caller: it was reaping generations that were
/// still inside the bound the request itself carried.
///
/// `None` when any term is unmeasured (her first turn, an unmeasured box): an absence is
/// not a number, and the named constant then governs alone.
///
/// The rates come through the SAME ladder the fill cap reads (fresh → stale → the box's
/// most conservative → none), not the fresh-only read: a stale 63 t/s point was good
/// enough to ADMIT a 1907-token prompt on the IntelMac (card c30a4757) while this
/// function, asking for fresh only, returned `None` and left the 300 s floor to govern
/// the wait for the prompt that rate had sized. One rate, one meaning: what sizes the
/// prompt sizes its wait.
pub(crate) fn expected_occupancy_for(persona_id: uuid::Uuid) -> Option<std::time::Duration> {
    let shape = LAST_TURN_SHAPE.get(&persona_id).map(|s| *s)?;
    let model = crate::inference::llama_server::current_serving().active_model?;
    let prefill_tps = crate::inference::prefill_rate::measured_rate_for(&model).tps?;
    let decode_tps = crate::inference::decode_knee::rate_for(&model).tps?;
    occupancy_of(shape, prefill_tps, decode_tps)
}

/// PURE: the wall time one turn of `shape` is expected to occupy the lane at this box's
/// measured rates — the UNCACHED prompt at `prefill_tps` plus the output at `decode_tps`.
/// `None` unless both rates are positive numbers: an absence (or a NaN) is not a number.
pub(super) fn occupancy_of(shape: TurnShape, prefill_tps: f64, decode_tps: f64) -> Option<std::time::Duration> {
    if !(prefill_tps > 0.0 && decode_tps > 0.0) {
        return None;
    }
    let uncached = shape.input.saturating_sub(shape.cached) as f64;
    let secs = uncached / prefill_tps + shape.output as f64 / decode_tps;
    Some(std::time::Duration::from_secs_f64(secs))
}

pub(super) fn metrics_from(
    persona: &str,
    resp: &TextGenerationResponse,
) -> crate::cognition::workspace::TurnMetrics {
    // The lane's PREFILL-vs-DECODE split (llama-server `timings`), when present:
    // cache_n/prompt_n is the KV-cache hit/miss, prompt_ms/predicted_ms the
    // wall-clock split that lets the harness see where Metal time actually goes.
    // Absent (cloud / older endpoints) → 0, and the breakdown rows read "n/a".
    let t = resp.timing.as_ref();
    let m = crate::cognition::workspace::TurnMetrics {
        input_tokens: resp.usage.input_tokens,
        output_tokens: resp.usage.output_tokens,
        latency_ms: resp.response_time_ms,
        cached_tokens: t.map(|t| t.cached_tokens).unwrap_or(0),
        prefill_tokens: t.map(|t| t.prefill_tokens).unwrap_or(0),
        prefill_ms: t.map(|t| t.prefill_ms.round() as u64).unwrap_or(0),
        decode_ms: t.map(|t| t.decode_ms.round() as u64).unwrap_or(0),
    };

    // THE CACHE-REUSE GLASS BOX — the row whose ABSENCE cost two weeks.
    //
    // Every generation funnels through here, so one probe covers every turn of every
    // citizen with no caller change. Emitted only when the lane actually reported
    // timings (cloud/older endpoints omit them); a fabricated 0.0 hit-rate for a
    // provider that never measured one is a lying receipt, so those stay silent.
    //
    // Why this did not exist and why that mattered: `delib` carried `turn.demand` and
    // `context.render` — how big the prompt WAS — and nothing about what the lane did
    // with it. So a prompt whose prefix we were destroying every turn looked identical
    // in the probe stream to one served entirely from cache. The only way to see it on
    // 2026-08-21 was hand-diffing a citizen's raw prompt captures, which is exactly the
    // manual step a probe exists to delete.
    //
    // What it proves, in one row: `hit_rate` near 0 with a large `prefill_tokens` means
    // the prefix is being invalidated upstream — measured that day at ~306s of wasted
    // re-prefill PER ACT while `--cache-reuse 256` was set and working. After the
    // canonical stable-tier ordering above, the same row is the verification: hit_rate
    // should climb off the floor from the second act of a task onward, WITHOUT anyone
    // re-reading a capture by hand.
    if t.is_some() {
        crate::probe!(
            class = "delib.generate.cache",
            persona = %persona,
            cached_tokens = m.cached_tokens,
            prefill_tokens = m.prefill_tokens,
            // The ratio the humans and the citizens both read. 1.0 = fully warm prefix;
            // near 0 = re-encoding the prompt every act, the inefficiency to attack.
            hit_rate = m.cache_hit_rate(),
            prefill_ms = m.prefill_ms,
            decode_ms = m.decode_ms,
            input_tokens = m.input_tokens,
            output_tokens = m.output_tokens,
            latency_ms = m.latency_ms,
            "prompt cache reuse for this generation"
        );
    }
    m
}

/// Append one request's exact message list to `<dir>/<persona>.wire.jsonl` (the wire capture,
/// sink 4 of PRIVACY-OF-THOUGHT.md). A turn in her mind room writes nothing.
pub(super) fn append_wire_capture(dir: &std::path::Path, persona_id: Uuid, persona_name: &str, room_id: Uuid, request: &TextGenerationRequest) {
    if crate::persona::mind_room::is_private_room(persona_id, room_id) {
        crate::persona::mind_room::note_withheld(persona_id, "wire_capture");
        return;
    }
    let row = serde_json::json!({
        "ts_ms": crate::time::EpochMs::now().millis(),
        "persona": persona_name,
        "messages": request
            .messages
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "text": m.content_text()}))
            .collect::<Vec<_>>(),
    });
    let _ = std::fs::create_dir_all(dir);
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{persona_name}.wire.jsonl")))
    {
        let _ = writeln!(f, "{row}");
    }
}
