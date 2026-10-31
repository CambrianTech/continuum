//! SSE stream — consume a chat-completions stream: every token to the sink the instant
//! it arrives, the two watchdogs (queue silence vs decode silence), prefill progress as
//! liveness, and the accumulation of content / reasoning / tool calls / usage / timings.
//! Carved out of `openai_adapter::generate_stream` (pure code-motion, 2026-09-03, the S3b
//! decompose) together with the wire types it parses. Behaviour-identical.

use std::time::Instant;

use serde::Deserialize;

use crate::ai::adapter::GenerationChunk;
use crate::ai::openai_adapter::OpenAICompatibleConfig;
use crate::inference::lane_send::PRE_STREAM_HEADER_TIMEOUT_SECS;

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIUsage {
    pub(crate) prompt_tokens: u32,
    pub(crate) completion_tokens: u32,
    pub(crate) total_tokens: Option<u32>,
}

/// llama-server's per-request `timings` object, present on the final stream frame.
/// These are the fields we surface into [`GenerationTiming`] so the harness can
/// separate PREFILL cost from DECODE cost; llama emits more (`*_per_token_ms`
/// variants) we don't need. All `#[serde(default)]` so a provider that omits any
/// field (or the whole object) degrades to zeros, never a parse failure.
#[derive(Debug, Deserialize)]
pub(crate) struct OpenAITimings {
    /// Prefix tokens served from KV cache (no recompute).
    #[serde(default)]
    pub(crate) cache_n: u32,
    /// NEW tokens prefilled this call (the re-rasterization tax).
    #[serde(default)]
    pub(crate) prompt_n: u32,
    #[serde(default)]
    pub(crate) prompt_ms: f64,
    #[serde(default)]
    pub(crate) prompt_per_second: f64,
    #[serde(default)]
    pub(crate) predicted_n: u32,
    #[serde(default)]
    pub(crate) predicted_ms: f64,
    #[serde(default)]
    pub(crate) predicted_per_second: f64,
}

/// How long the inference lane may stay SILENT mid-stream before we declare it
/// dead. This is a LIVENESS watchdog, not a deadline: a slow-but-producing decode
/// (a 4B model on CPU emitting a token every few hundred ms) stays alive
/// indefinitely as long as it keeps streaming. Only true silence — the backend
/// stuck, crashed, or the socket wedged — trips it, and then we fail loud naming
/// the cause ([[fallbacks-are-illegal-fail-loud]]). Replaces the old wall-clock
/// total-request timeout that killed legitimately-long generations.
pub(crate) const STREAM_IDLE_TIMEOUT_SECS: u64 = 90;

// Lane-send constants live in `crate::inference::lane_send` (S3b decompose).


/// Warn LOUD when a call's measured decode rate collapses far below the catalog
/// row's expectation (#441, Joel 2026-08-15: "we need very good tok/sec or it's a
/// failure. We need warnings when shit hits the fan").
///
/// The comparison is decode-only (`predicted_per_second` — undiluted by prefill,
/// so a long prompt can't false-positive this) against the row's
/// `tokens_per_second`. Both thresholds are deliberately coarse: this is a
/// shit-hit-the-fan alarm for order-of-magnitude collapse (CPU-fallback lane,
/// thrashing pager, contended GPU), not a perf regression tracker — a row whose
/// estimate is merely 2× optimistic must stay silent.
///
/// The classification itself is delegated to the canonical
/// [`crate::inference::throughput_expectation::classify_throughput`] — this
/// call site only supplies policy: a sample-size gate (a rate computed over a
/// handful of tokens is noise, and warmup's first few decodes read slow) and a
/// collapse floor (below a quarter of expectation is a different MACHINE
/// STATE, not variance — a row whose estimate is merely 2× optimistic must
/// stay silent).
///
/// Stateless by design — one warn per breaching call. During a genuinely
/// degraded period that is one line per turn, which is the correct volume for
/// "every consumer of this lane is currently waiting an eternity". Unknown model
/// rows and rows without an expectation stay silent (no registry = no contract
/// to breach — external/cloud adapters are not governed lanes).
pub(crate) fn warn_if_decode_collapsed(model_id: &str, decode_tokens: u32, measured_tps: f64) {
    if decode_tokens < 16 || measured_tps <= 0.0 {
        return;
    }
    let Some(expected) = crate::model_registry::try_global()
        .and_then(|r| r.model(model_id).map(|m| m.tokens_per_second as f64))
        .filter(|e| *e > 0.0)
    else {
        return;
    };
    // EVERY measured decode feeds the decode curve (not only collapsed ones): the
    // planner's lane clamp is derived from it (`inference::decode_knee`).
    crate::inference::decode_knee::observe(
        model_id,
        crate::inference::resource_admission::inflight_model_calls() as u32,
        measured_tps,
        expected,
    );
    // Collapse alarm only: floor 0.25 of catalog rate, no above-par ceiling
    // (this seam never celebrates over-delivery — it screams on collapse).
    let verdict = crate::inference::throughput_expectation::classify_throughput(
        measured_tps,
        expected,
        0.25,
        f64::INFINITY,
    );
    if verdict.is_degraded() {
        // CONCURRENCY AT THE MOMENT OF MEASUREMENT (#441). The expectation is a
        // SINGLE-STREAM rate; this decode may have shared the box with N-1 other
        // model calls, and a shared decode is legitimately slower with NOTHING
        // wrong. Measured 2026-08-20 on the 27B: 6.56 t/s median against a 17.2
        // pinned expectation — ratio 0.38, which is real contention, not a defect.
        //
        // ANNOTATE, DO NOT GATE. This alarm's own contract names "contended GPU"
        // as a thing it exists to catch, so suppressing on concurrency would
        // defeat it. Reporting the count lets a reader attribute the ratio
        // instead of hunting a CPU fallback that isn't there — the failure mode
        // this line previously invited by listing three suspects and no evidence.
        //
        // Reuses the existing gauge (`resource_admission::inflight_model_calls`,
        // "lane-queue + prefill + decode") rather than counting again — the
        // concurrency sibling of the window axis on `ThroughputBaseline`.
        let inflight = crate::inference::resource_admission::inflight_model_calls();
        tracing::warn!(
            probe_class = "serving.throughput.degraded",
            model = model_id,
            measured_tps = measured_tps,
            expected_tps = expected,
            ratio = verdict.ratio(),
            decode_tokens = decode_tokens,
            inflight_model_calls = inflight,
            "THROUGHPUT COLLAPSE: decode {measured_tps:.1} t/s vs expected {expected:.0} t/s \
             (single-stream) with {inflight} model call(s) in flight — this lane is serving at \
             a fraction of its catalog rate. With >1 in flight the expectation is not \
             like-for-like and contention alone may explain it; at 1 in flight suspect CPU \
             fallback (see serving.placement.cpu_fallback) or pager thrash (#441)."
        );
    }
}

/// One streamed SSE frame from an OpenAI-compatible `/v1/chat/completions` with
/// `stream: true`. Each frame carries an incremental `delta`; `usage` arrives only
/// on the final frame (requires `stream_options.include_usage`).
#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamChunk {
    #[serde(default)]
    pub(crate) choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    pub(crate) usage: Option<OpenAIUsage>,
    /// Per-request lane timings (cache_n / prompt_ms / predicted_per_second …);
    /// arrives on the final frame alongside `usage`.
    #[serde(default)]
    pub(crate) timings: Option<OpenAITimings>,
    #[serde(default)]
    pub(crate) model: String,
    /// PREFILL progress (llama.cpp `return_progress` extension). Present only on
    /// frames emitted while the slot is still ingesting the prompt — before any
    /// token exists. This is the ONLY evidence a client has that a long prefill
    /// is advancing rather than wedged; see the liveness rule in
    /// [`OpenAIAdapter::stream_completion`].
    #[serde(default)]
    pub(crate) prompt_progress: Option<OpenAIPromptProgress>,
    /// The server failed this request mid-stream (`data: {"error":{...}}`). It carries no
    /// choices and no finish reason, so without this field it parsed as an empty frame and
    /// the act ended as an empty `stop`: the M5 logged 32 such "completed" acts while its
    /// Metal backend was wedged (2026-10-23, card 7eef3e05).
    #[serde(default)]
    pub(crate) error: Option<OpenAIStreamError>,
}

/// A server-side failure frame: llama.cpp's `send_error` (`code`, `message`, `type`).
#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamError {
    #[serde(default)]
    pub(crate) code: Option<u16>,
    #[serde(default)]
    pub(crate) message: String,
    #[serde(default, rename = "type")]
    pub(crate) kind: String,
}

/// llama.cpp's `prompt_progress` frame — the slot's ingest counter.
///
/// `processed` climbs toward `total` as prefill proceeds; `cache` is the prefix
/// the KV cache served for free. A slot that is genuinely wedged holds
/// `processed` FROZEN, which is exactly what makes this a liveness signal and
/// not merely a keepalive.
#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIPromptProgress {
    #[serde(default)]
    pub(crate) total: u64,
    #[serde(default)]
    pub(crate) cache: u64,
    #[serde(default)]
    pub(crate) processed: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamChoice {
    #[serde(default)]
    pub(crate) delta: Option<OpenAIStreamDelta>,
    #[serde(default)]
    pub(crate) finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct OpenAIStreamDelta {
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_content: Option<String>,
    #[serde(default)]
    pub(crate) tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamToolCall {
    #[serde(default)]
    pub(crate) index: Option<usize>,
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamFunction {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) arguments: Option<String>,
}

/// A tool call assembled across many streamed `delta.tool_calls` fragments. The
/// model emits the id + name once and then the JSON `arguments` arrive token by
/// token; we accumulate by `index` until the stream ends.
#[derive(Default)]
pub(crate) struct StreamToolAccum {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: String,
}

/// Fold one streamed tool-call fragment into the per-index accumulator.
pub(crate) fn accumulate_stream_tool_call(acc: &mut Vec<StreamToolAccum>, tc: OpenAIStreamToolCall) {
    let idx = tc.index.unwrap_or(0);
    if acc.len() <= idx {
        acc.resize_with(idx + 1, StreamToolAccum::default);
    }
    let slot = &mut acc[idx];
    if let Some(id) = tc.id {
        if !id.is_empty() {
            slot.id = id;
        }
    }
    if let Some(f) = tc.function {
        if let Some(n) = f.name {
            if !n.is_empty() {
                slot.name = n;
            }
        }
        if let Some(a) = f.arguments {
            slot.arguments.push_str(&a);
        }
    }
}

/// Everything the stream accumulated, handed back to the caller's post-processing
/// (reasoning split, usage, timings, the response) exactly as the inline locals were.
pub(crate) struct StreamOutcome {
    pub(crate) acc_content: String,
    pub(crate) acc_reasoning: String,
    pub(crate) acc_tools: Vec<StreamToolAccum>,
    pub(crate) finish_reason_str: Option<String>,
    pub(crate) stream_usage: Option<OpenAIUsage>,
    pub(crate) stream_timings: Option<OpenAITimings>,
    pub(crate) resp_model: Option<String>,
    pub(crate) probe_persona: String,
}

/// Consume `response`'s SSE stream to completion (or a watchdog failure).
/// The longest a stalled stream waits on a lane that is working for others: the act's own
/// ceiling, so a caller without an act bound above it (a probe, an eval) still ends.
const STARVED_WAIT_CEILING: std::time::Duration = crate::inference::llama_server::UNMOVED_CEILING;

/// Whether a stream that has been silent (or stalled) for `idle` sits on a lane that did real
/// work, prefill or decode, for ANYONE within that window, and has been open less than the
/// ceiling. Busy, not dead: the bound becomes a checkpoint (card 115f9a14).
fn lane_class(idle: std::time::Duration, model: Option<&str>) -> crate::inference::llama_server::NeverStartedClass {
    // The one no-movement window (`llama_server::unmoved_window`): two batches at the
    // measured prefill rate, never less than `idle`.
    let window = crate::inference::llama_server::unmoved_window(idle, model);
    crate::inference::llama_server::classify_never_started_timeout(
        crate::inference::llama_server::ms_since_real_work(),
        window.as_millis() as u64,
    )
}

/// PURE: whether a stalled stream waits on, from the ONE classification of the lane taken at
/// this checkpoint (Cormac on #4506: two reads could disagree and strike a working lane) and
/// how long the stream has been open.
fn waits_on(class: crate::inference::llama_server::NeverStartedClass, open: std::time::Duration) -> bool {
    open < STARVED_WAIT_CEILING && class == crate::inference::llama_server::NeverStartedClass::Starved
}

pub(crate) async fn consume_sse_stream(
    cfg: &OpenAICompatibleConfig,
    request: &crate::ai::types::TextGenerationRequest,
    local_lane: bool,
    response: reqwest::Response,
    sink: &tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
) -> Result<StreamOutcome, String> {
    // Consume the SSE stream: every token reaches `sink` the INSTANT it arrives, and its
    // consumer gets the turn before the next is parsed (`stream_sinks::deliver`).
    // Liveness is the per-token idle watchdog ([`STREAM_IDLE_TIMEOUT_SECS`]) —
    // silence means the backend died, NOT that generation is simply long.
    use futures::StreamExt;
    let mut byte_stream = response.bytes_stream();
    // QUEUE WAIT IS NOT SILENCE. Two different things were being policed by one
    // budget. MEASURED 2026-08-13 on the live 1-slot lane: a TINY (2,237-token)
    // request got its first byte of any kind at t=115.2s — not because prefill
    // was slow (it finished within the same second) but because the slot was
    // busy with a co-tenant's turn for 115s first. llama-server says nothing
    // while a task is queued; there is no frame to send until a slot picks it
    // up. So the 90s liveness budget was being spent on CONTENTION, and with
    // total_slots=1 and four citizens it expires routinely on a healthy lane.
    //
    // Split by what the silence MEANS. Before the server shows any sign of
    // working on THIS request, the bound is the same one the header wait
    // already uses and justifies — queue wait is a capacity fact, minutes are
    // legitimate, and the job is releasing eternal holds, not policing slow
    // ones. Once the slot IS working (a prefill-progress frame or a token),
    // the tight liveness budget applies: from then on, silence really is the
    // backend dying, which is what #385 was always about.
    //
    // The queue bound is the header wait's FLOOR or the turn's own measured bound
    // above it (`TextGenerationRequest::turn_bound`, card ba82d0a0): a request queued
    // behind a co-tenant's 30k prefill on a 25 tok/s box hears nothing for ~1200 s,
    // and a constant 300 s read every such turn as dead.
    let (queue_budget, queue_source) = crate::inference::turn_bound::effective_bound_with_source(
        std::time::Duration::from_secs(PRE_STREAM_HEADER_TIMEOUT_SECS),
        request.turn_bound,
    );
    let live_budget = std::time::Duration::from_secs(STREAM_IDLE_TIMEOUT_SECS);
    // #363: real-delivery accounting for the LOCAL lane only. A terminal stream
    // death on the local lane is wedge evidence the smoke probe cannot see (an
    // undersized slot passes a tiny probe while rejecting real prompts); a
    // completed stream is proof of life. Both stamps are gated on this request
    // actually targeting the published serving lane.

    let mut sse_buf: Vec<u8> = Vec::new();
    // Each raw event, kept only when a capture armed this request (card fb6d7912).
    let mut wire = crate::inference::wire_tap::WireTap::open(request.request_id.as_deref());
    let mut acc_content = String::new();
    let mut acc_reasoning = String::new();
    let mut acc_tools: Vec<StreamToolAccum> = Vec::new();
    let mut finish_reason_str: Option<String> = None;
    let mut stream_usage: Option<OpenAIUsage> = None;
    let mut stream_timings: Option<OpenAITimings> = None;
    let mut resp_model: Option<String> = None;

    // #385 (the 5-hour wedge): the timeout below bounds TRANSPORT silence, but
    // any bytes reset it — and a wedged slot that keeps emitting keepalives /
    // comment frames (n_decoded frozen at 1 for HOURS, 2026-08-09) resets it
    // forever. Liveness must be keyed on PROGRESS: `last_progress` advances only
    // when a parsed event yields an actual delta (content / reasoning / tool /
    // finish). Bytes without progress for the same idle budget = the
    // keepalive-masked wedge, failed as loudly as transport silence.
    //
    // PREFILL IS PROGRESS (2026-08-13). The rule above was right about the
    // wedge and wrong about what "progress" means: it counted only DECODED
    // tokens, so a slot legitimately ingesting a long prompt looked identical
    // to a frozen one. It isn't: prefill has a rising counter. We now request
    // `return_progress` (see the body builder) and treat a rising `processed`
    // as progress — the slot is doing the work we asked for. A genuinely
    // wedged slot holds that counter FROZEN and still fails in the same 90s,
    // so the #385 detector keeps its teeth while healthy work stops being
    // executed for the crime of having a big prompt.
    let mut last_prefill_processed: u64 = 0;
    let stream_opened = Instant::now();
    let mut last_progress = stream_opened;
    // PREFILL IS NOT DECODE (2026-08-21, the round-killer). `live_budget` is a
    // DECODE watchdog by its own doc — "a token every few hundred ms". Selecting
    // it the moment ANY progress arrived meant llama.cpp's 0%-ingestion signalling
    // frame dropped us from 300s to 90s seconds into a prefill that measured ~170s
    // for a window-sized prompt, so every big turn died and retried forever. The
    // phase machine keeps the two regimes apart; see `inference::stream_liveness`.
    // Stream attribution for the prefill probe: without it every cached%
    // sample is anonymous, and the 2026-08-23 KV iteration spent a round
    // unable to tell Atlas's task acts from Benchy's ambient turns. One
    // clone per stream open — cold path.
    let probe_persona: String = request
        .persona_id
        .clone()
        .unwrap_or_else(|| "non-persona".into()); // non-persona callers (CLI, probes) are a real class, labeled honestly
    let probe_purpose: String = request.purpose.clone().unwrap_or_default(); // absent purpose renders empty — a label, never a quantity
    let mut phase = crate::inference::stream_liveness::StreamPhase::Queued;
    // One `inference.prefill.rescued` row per stream, not per frame.
    let mut prefill_rescued = false;
    // When the FIRST prompt_progress frame arrived. llama.cpp emits the 0% frame
    // at the moment the slot is ASSIGNED ("signal the client that the request has
    // started processing"), so open→first-frame is QUEUE WAIT and
    // first-frame→complete is INGEST. The first cut of these probes conflated the
    // two into one `elapsed_ms`, and the numbers were absurd in exactly the way a
    // conflation is: a 467-token prompt "prefilling" for 231s at 1 tok/s. It was
    // queued 230 of those seconds. A probe that mixes two regimes measures neither.
    let mut first_prefill_frame: Option<Instant> = None;
    loop {
        let idle = crate::inference::stream_liveness::idle_budget(
            phase,
            queue_budget,
            live_budget,
        );
        let next = match tokio::time::timeout(idle, byte_stream.next()).await {
            Ok(next) => next,
            Err(_) => {
                // BUSY IS NOT DEAD (card 115f9a14). A lane that did real work for ANYONE
                // within the window is working, and our silence is its batch serving another
                // slot (IntelMac 2026-09-28 05:07Z: a turn at 96% prefill dropped at 90 s while
                // slot 4 prefilled a 2,048-token chunk every ~176 s). The bound becomes a
                // checkpoint and the stream waits on; the act bound above it still ends a
                // request whose own slot stops moving (#4455).
                // ONE read of the lane's work and ONE classification decide both whether we
                // wait and, if not, how the failure is recorded.
                let class = lane_class(idle, request.model.as_deref());
                if local_lane && waits_on(class, stream_opened.elapsed()) {
                    continuum_base::probe!(
                        class = "inference.stream.busy_not_dead",
                        provider = cfg.name.as_str(),
                        stalled = "no_bytes",
                        started = phase.has_started(),
                        idle_secs = idle.as_secs(),
                        persona = probe_persona.as_str(),
                        purpose = probe_purpose.as_str(),
                        phase = ?phase,
                        ms_since_lane_work = crate::inference::llama_server::ms_since_real_work().unwrap_or(u64::MAX), // probe field: MAX = the lane never delivered work this process
                        "no bytes for the idle bound, but the lane did real work for others meanwhile: busy, not dead; waiting on"
                    );
                    last_progress = Instant::now();
                    continue;
                }
                let msg: String = {
                let started = phase.has_started();
                if idle == queue_budget {
                    // The bulk bound tripped (queued or prefilling with no progress
                    // frame for the whole budget) — the receipt names the bound and
                    // who sized it, so a busy-not-dead row points at the number.
                    crate::inference::turn_bound::probe_tripped(
                        "stream_queue",
                        &cfg.name,
                        queue_budget,
                        queue_source,
                        request.turn_bound,
                    );
                }
                if local_lane {
                    if started {
                        // Started-then-stopped is per-slot evidence about OUR
                        // generation — always counts toward the relaunch threshold.
                        // Every count gets a row: 2026-09-05 four of these relaunched
                        // a lane (12 citizens re-prefilled cold) with nothing naming
                        // which generation or why (absence-read-as-a-value class).
                        continuum_base::probe!(
                            class = "inference.decode.failed",
                            kind = "started_then_stopped",
                            name = %cfg.name,
                            idle_secs = idle.as_secs(),
                            "generation started then stopped past the idle budget — counted toward relaunch"
                        );
                        crate::inference::llama_server::note_real_decode_failure();
                    } else {
                        // Never-started is ambiguous: dead backend vs oversubscribed
                        // queue. Judge it by the lane's own delivery record — if real
                        // tokens came out for ANYONE while we waited, the lane is
                        // provably alive and this is starvation, not a wedge. Stamping
                        // starvation as wedge evidence relaunched a healthy busy lane
                        // every 2 minutes (bench-hard-rs, 2026-08-15) and killed the
                        // in-flight generations that proved it healthy.
                        use crate::inference::llama_server::NeverStartedClass;
                        match class {
                            NeverStartedClass::WedgeEvidence => {
                                continuum_base::probe!(
                                    class = "inference.decode.failed",
                                    kind = "never_started_wedge_evidence",
                                    name = %cfg.name,
                                    idle_secs = idle.as_secs(),
                                    ms_since_real_work = crate::inference::llama_server::ms_since_real_work(),
                                    "generation never started and the lane delivered nothing to anyone meanwhile — counted toward relaunch"
                                );
                                crate::inference::llama_server::note_real_decode_failure();
                            }
                            NeverStartedClass::Starved => {
                                // The capacity shortfall stays LOUD on its own channel
                                // (#234 QoS reads this) — it just stops masquerading as
                                // lane death.
                                continuum_base::probe!(
                                    class = "inference.queue_starved",
                                    provider = cfg.name.as_str(),
                                    waited_s = idle.as_secs(),
                                    "never-started timeout on a lane that delivered real \
                                     tokens within the wait — oversubscription, not wedge \
                                     evidence; no real-turn failure stamped",
                                );
                            }
                        }
                    }
                }
                format!(
                    "{}: inference lane went silent for {}s (no bytes at all) — {}; \
                     refusing to wait on a dead stream",
                    cfg.name,
                    idle.as_secs(),
                    if started {
                        "the slot HAD started our work and then stopped mid-stream, \
                         so the backend is stuck or dead"
                    } else {
                        "the slot never started our work — either the backend is dead \
                         or the queue is oversubscribed far beyond this budget"
                    }
                )
                };
                return Err(msg);
            }
        };
        if last_progress.elapsed() >= idle {
            // ONE read, ONE classification for this checkpoint (Cormac on #4506).
            let class = lane_class(idle, request.model.as_deref());
            if local_lane && waits_on(class, stream_opened.elapsed()) {
                // BUSY IS NOT DEAD (card 115f9a14): our counters stalled because the batch
                // served another slot; the lane is working, so the bound is a checkpoint,
                // with no failure probe and no relaunch strike.
                continuum_base::probe!(
                    class = "inference.stream.busy_not_dead",
                    provider = cfg.name.as_str(),
                    stalled = "progress",
                    started = phase.has_started(),
                    idle_secs = idle.as_secs(),
                    persona = probe_persona.as_str(),
                    purpose = probe_purpose.as_str(),
                    phase = ?phase,
                    ms_since_lane_work = crate::inference::llama_server::ms_since_real_work().unwrap_or(u64::MAX), // probe field: MAX = the lane never delivered work this process
                    "our stream's progress stalled for the idle bound while the lane did real work for others: busy, not dead; waiting on"
                );
                last_progress = Instant::now();
            } else {
            if idle == queue_budget {
                crate::inference::turn_bound::probe_tripped(
                    "stream_queue",
                    &cfg.name,
                    queue_budget,
                    queue_source,
                    request.turn_bound,
                );
            }
            if local_lane {
                // The same question the never-started path asks (card 2caa0de5): did the lane
                // deliver real work to ANYONE while this request stalled? With 6 CPU slots, one
                // request's prefill can sit behind the others' for the whole idle window while
                // the engine is plainly working. That is starvation, and stamping it as wedge
                // evidence relaunched a fresh, delivering lane on the IntelMac (2026-09-27
                // 21:23Z: two stamps, `real_turn_failures`, while health read ok via real_work
                // and slot_progress seconds earlier). The turn still fails either way; only a
                // lane that delivered nothing to anyone in the window counts toward relaunch.
                use crate::inference::llama_server::NeverStartedClass;
                // WHICH request, HOW FAR it got, and whether the lane was working for anyone
                // else meanwhile (card 18d306f8): a reader must tell "starved behind a
                // neighbour's prefill chunk" from "wedged" off this one row, without
                // cross-reading the engine log (IntelMac 2026-09-28 05:07Z: a turn dropped
                // at 96% of its prefill while slot 4 ingested a 25k prompt at 11 tok/s).
                // A probe value only: the decision is the one `class` read above.
                let since_work = crate::inference::llama_server::ms_since_real_work();
                let (prefill_processed, prefill_total) = match phase {
                    crate::inference::stream_liveness::StreamPhase::Prefilling { processed, total } => (processed, total),
                    _ => (0, 0),
                };
                match class {
                    NeverStartedClass::WedgeEvidence => {
                        continuum_base::probe!(
                            class = "inference.decode.failed",
                            kind = "keepalive_masked_no_progress",
                            name = %cfg.name,
                            idle_secs = idle.as_secs(),
                            persona = probe_persona.as_str(),
                            purpose = probe_purpose.as_str(),
                            phase = ?phase,
                            prefill_processed,
                            prefill_total,
                            ms_since_lane_work = since_work.unwrap_or(u64::MAX), // probe field: MAX = the lane never delivered work this process
                            "stream carried bytes but neither prefill nor decode advanced, and the lane delivered nothing to anyone meanwhile — counted toward relaunch"
                        );
                        crate::inference::llama_server::note_real_decode_failure();
                    }
                    NeverStartedClass::Starved => {
                        continuum_base::probe!(
                            class = "inference.queue_starved",
                            provider = cfg.name.as_str(),
                            waited_s = idle.as_secs(),
                            stalled = "progress",
                            persona = probe_persona.as_str(),
                            purpose = probe_purpose.as_str(),
                            phase = ?phase,
                            prefill_processed,
                            prefill_total,
                            ms_since_lane_work = since_work.unwrap_or(u64::MAX), // probe field: MAX = the lane never delivered work this process
                            "this request's progress stalled on a lane that delivered real work within the wait — oversubscription, not wedge evidence; no real-turn failure stamped",
                        );
                    }
                }
            }
            return Err(format!(
                "{}: no PROGRESS for {}s despite the stream carrying bytes — \
                 keepalive-masked wedge (neither prefill nor decode advanced; stalled in {:?}); \
                 refusing to wait on a stream that is alive but not working (#385)",
                cfg.name,
                idle.as_secs(),
                phase
            ));
            }
        }
        let Some(chunk) = next else {
            break; // server closed the stream (EOF) — generation complete
        };
        let bytes = chunk.map_err(|e| {
            if local_lane {
                continuum_base::probe!(
                    class = "inference.decode.failed",
                    kind = "stream_read_error",
                    name = %cfg.name,
                    error = %e,
                    "stream read error mid-generation — counted toward relaunch"
                );
                crate::inference::llama_server::note_real_decode_failure();
            }
            format!("{}: stream read error: {e}", cfg.name)
        })?;

        // Strip CR (0x0D) so event boundaries normalize to `\n\n`. CR never
        // appears inside a UTF-8 multibyte sequence, so this is decode-safe; we
        // buffer RAW bytes and only decode COMPLETE events (no mid-char split).
        for b in bytes.iter() {
            if *b != b'\r' {
                sse_buf.push(*b);
            }
        }

        while let Some(pos) = sse_buf.windows(2).position(|w| w == b"\n\n") {
            let event_bytes: Vec<u8> = sse_buf.drain(..pos + 2).collect();
            let event = String::from_utf8_lossy(&event_bytes);
            wire.push(&event);
            for line in event.lines() {
                let Some(data) = line.trim_start().strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let parsed: OpenAIStreamChunk = match serde_json::from_str(data) {
                    Ok(p) => p,
                    Err(_) => continue, // keepalive / comment / non-JSON line
                };
                // A failure frame ends the act as a failure, with the server's words. It
                // counts toward the lane's relaunch the same way a broken stream does.
                if let Some(error) = parsed.error {
                    if local_lane {
                        continuum_base::probe!(
                            class = "inference.decode.failed",
                            kind = "server_error_frame",
                            name = %cfg.name,
                            code = error.code.unwrap_or(0), // unwrap_or: a frame with no code still failed; 0 = unstated
                            error_type = %error.kind,
                            error = %error.message,
                            "the server sent an error frame mid-stream: the act failed, never an empty completion"
                        );
                        crate::inference::llama_server::note_real_decode_failure();
                    }
                    return Err(format!(
                        "{}: server error {}{}: {}",
                        cfg.name,
                        error.code.map(|code| code.to_string()).unwrap_or_default(), // unwrap_or_default: no code renders as nothing
                        if error.kind.is_empty() { String::new() } else { format!(" ({})", error.kind) },
                        error.message
                    ));
                }
                if resp_model.is_none() && !parsed.model.is_empty() {
                    resp_model = Some(parsed.model.clone());
                }
                if let Some(u) = parsed.usage {
                    stream_usage = Some(u);
                }
                if let Some(t) = parsed.timings {
                    stream_timings = Some(t);
                }
                // Prefill advanced → the slot is alive and working. Strictly
                // `>` so a REPEATED frame carrying the same count (the wedge
                // signature) cannot hold the watchdog open forever.
                if let Some(p) = parsed.prompt_progress {
                    // Phase advances on EVERY frame (including the 0% signalling
                    // one), because "the slot is assigned and ingesting" is true
                    // from the first frame — that is what picks the bulk budget.
                    // The liveness STAMP below still requires strict advance, so a
                    // frozen counter cannot hold the watchdog open (#385).
                    let was = phase;
                    phase = phase.on_prefill(p.processed, p.total);
                    let first_frame = *first_prefill_frame.get_or_insert_with(Instant::now);

                    // PROVE THE FIX IS LOAD-BEARING. Without this the change is
                    // invisible: "no retries" is an ABSENCE, and an absence cannot
                    // distinguish "prefills now survive" from "nothing is running"
                    // — the exact ambiguity that cost a full day of diagnosis.
                    // This fires ONCE per stream, only for a prefill that has
                    // already outlived the OLD decode budget while still advancing.
                    // Every row is therefore a turn that would previously have been
                    // killed and retried forever.
                    let elapsed = stream_opened.elapsed();
                    if !prefill_rescued
                        && matches!(phase, crate::inference::stream_liveness::StreamPhase::Prefilling { .. })
                        && elapsed > live_budget
                    {
                        prefill_rescued = true;
                        continuum_base::probe!(
                            class = "inference.prefill.rescued",
                            provider = cfg.name.as_str(),
                            elapsed_s = elapsed.as_secs(),
                            queued_s = first_frame.duration_since(stream_opened).as_secs(),
                            old_budget_s = live_budget.as_secs(),
                            processed = p.processed,
                            total = p.total,
                            cached = p.cache,
                            "prefill outlived the OLD decode watchdog and is STILL \
                             advancing — under the previous flat budget this turn \
                             would have been killed here and retried forever",
                        );
                    }
                    // Per-stream ingest receipt, emitted once at the prefill→decode
                    // edge, with QUEUE and INGEST separated: llama.cpp's 0% frame
                    // marks slot ASSIGNMENT, so open→first-frame is time spent
                    // waiting for a slot and first-frame→now is real ingest work.
                    // `ingest_tok_per_s` is the number the 90s constant was
                    // implicitly guessing at and the one a derived budget should
                    // come from per model+device (#441); `queued_ms` is the
                    // admission/oversubscription signal (#234 QoS).
                    // `would_have_died` shadows the bound IN FORCE for this turn
                    // (`queue_budget`: the floor, or the turn's measured bound above
                    // it): 1 = this prefill outlived the bound that any seam waiting
                    // on it flat — a request queued behind it, the remote requester —
                    // would have killed it at. Until 2026-09-20 it shadowed the 90 s
                    // decode watchdog, a bound nothing was waiting on any more; the
                    // M5 read 1 at 331–389 s of ingest, and the number that had
                    // actually killed the turn's queued sibling was the 300 s floor.
                    // A 1 here with a healthy lane means the bound is undersized.
                    if matches!(was, crate::inference::stream_liveness::StreamPhase::Prefilling { .. })
                        && matches!(phase, crate::inference::stream_liveness::StreamPhase::Decoding)
                    {
                        let queued_ms = first_frame.duration_since(stream_opened).as_millis() as u64;
                        let ingest_ms = (first_frame.elapsed().as_millis().max(1)) as u64;
                        let fresh = p.total.saturating_sub(p.cache);
                        continuum_base::probe!(
                            class = "inference.prefill.complete",
                            provider = cfg.name.as_str(),
                            persona = probe_persona.as_str(),
                            purpose = probe_purpose.as_str(),
                            total = p.total,
                            cached = p.cache,
                            fresh = fresh,
                            queued_ms = queued_ms,
                            ingest_ms = ingest_ms,
                            ingest_tok_per_s = (fresh as f64 * 1000.0 / ingest_ms as f64) as u64,
                            bound_secs = queue_budget.as_secs(),
                            bound_source = queue_source.as_str(),
                            would_have_died = u8::from(elapsed > queue_budget) as u64,
                            "prefill complete — queue wait vs real ingest, and the \
                             cache's actual contribution, per stream",
                        );
                    }
                    if p.processed > last_prefill_processed {
                        last_prefill_processed = p.processed;
                        last_progress = Instant::now();
                        // L9: prefill advance is LANE liveness, not just request
                        // liveness — the health heartbeat and the never-started
                        // classifier both read this stamp via ms_since_real_work.
                        if local_lane {
                            crate::inference::llama_server::note_real_prefill_progress();
                        }
                        crate::ai::stream_sinks::deliver(
                            sink,
                            GenerationChunk::Prefill {
                                processed: p.processed,
                                total: p.total,
                                cached: p.cache,
                            },
                        )
                        .await;
                    }
                }
                // Any REAL output (token, reasoning, tool delta, finish) means
                // prefill is definitionally over. Detected once here, by observing
                // whether the sites below moved the liveness stamp, rather than
                // repeating a phase assignment at each of the four — one decision,
                // one place, and a new output kind cannot forget to declare itself.
                let progress_before_output = last_progress;
                if let Some(choice) = parsed.choices.into_iter().next() {
                    if let Some(fr) = choice.finish_reason {
                        finish_reason_str = Some(fr);
                        last_progress = Instant::now();
                    }
                    if let Some(delta) = choice.delta {
                        if let Some(c) = delta.content {
                            if !c.is_empty() {
                                acc_content.push_str(&c);
                                crate::ai::stream_sinks::deliver(sink, GenerationChunk::Token(c)).await;
                                last_progress = Instant::now();
                            }
                        }
                        if let Some(r) = delta.reasoning_content {
                            if !r.is_empty() {
                                acc_reasoning.push_str(&r);
                                crate::ai::stream_sinks::deliver(sink, GenerationChunk::Reasoning(r)).await;
                                last_progress = Instant::now();
                            }
                        }
                        if let Some(tcs) = delta.tool_calls {
                            for tc in tcs {
                                accumulate_stream_tool_call(&mut acc_tools, tc);
                                last_progress = Instant::now();
                            }
                        }
                    }
                }
                if last_progress != progress_before_output {
                    phase = phase.on_output();
                }
            }
        }
    }


    Ok(StreamOutcome {
        acc_content,
        acc_reasoning,
        acc_tools,
        finish_reason_str,
        stream_usage,
        stream_timings,
        resp_model,
        probe_persona,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // what this catches (Astra on #4978): a captured retry replayed as one stream. The
    // accumulator starts fresh on every attempt, so an interrupted attempt's index-0
    // arguments merged with its retry's would replay arguments the response never held.
    // Each captured attempt replays through the real fold on its own; the last is the
    // response's.
    #[test]
    fn a_captured_retry_replays_per_attempt_never_merged() {
        let replay = |events: &[String]| {
            let mut acc = Vec::new();
            for event in events {
                for line in event.lines() {
                    let Some(data) = line.trim_start().strip_prefix("data:") else { continue };
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let chunk: OpenAIStreamChunk = serde_json::from_str(data).expect("test: chunk");
                    for choice in chunk.choices {
                        for tc in choice.delta.and_then(|d| d.tool_calls).unwrap_or_default() {
                            accumulate_stream_tool_call(&mut acc, tc);
                        }
                    }
                }
            }
            acc
        };
        let fragment = |name: Option<&str>, args: &str| {
            let function = match name {
                Some(n) => serde_json::json!({"name": n, "arguments": args}),
                None => serde_json::json!({"arguments": args}),
            };
            format!(
                "data: {}",
                serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":function}]}}]})
            )
        };
        let id = format!("replay-{}", uuid::Uuid::new_v4());
        crate::inference::wire_tap::arm(&id);
        let mut interrupted = crate::inference::wire_tap::WireTap::open(Some(&id));
        interrupted.push(&fragment(Some("work/note"), "{\"note\":\"clo"));
        drop(interrupted);
        let mut retry = crate::inference::wire_tap::WireTap::open(Some(&id));
        retry.push(&fragment(Some("work/note"), "{\"note\":"));
        retry.push(&fragment(None, "\"closed\"}"));
        retry.push("data: [DONE]");
        drop(retry);
        let wire = crate::inference::wire_tap::take(&id).expect("captured");
        let last = replay(wire.attempts.last().expect("the response's attempt"));
        assert_eq!((last[0].name.as_str(), last[0].arguments.as_str()), ("work/note", "{\"note\":\"closed\"}"));
        let merged = replay(&wire.attempts.concat());
        assert_ne!(merged[0].arguments, last[0].arguments, "merging attempts invents arguments");
    }

    // what this catches (card 115f9a14; the IntelMac, 2026-09-28 05:07Z): a turn at 96% prefill
    // failed at the 90 s idle bound while slot 4 prefilled for the whole window. A lane that did
    // real work for anyone within the window is busy, not dead, so the stream waits on; a lane
    // that did nothing is wedge evidence and the stream ends; past the act's own ceiling the
    // stream ends whatever the lane is doing, so every hold stays bounded.
    #[test]
    fn a_stalled_stream_waits_on_a_working_lane_and_only_there() {
        use crate::inference::llama_server::{classify_never_started_timeout, NeverStartedClass};
        let open = Duration::from_secs(400);
        let working = classify_never_started_timeout(Some(5_000), 90_000);
        assert_eq!(working, NeverStartedClass::Starved, "work 5 s ago inside a 90 s window is a working lane");
        assert!(waits_on(working, open), "a working lane: the stream waits on");
        assert!(!waits_on(classify_never_started_timeout(Some(200_000), 90_000), open), "nothing for anyone in the window: ends");
        assert!(!waits_on(classify_never_started_timeout(None, 90_000), open), "never any work observed: ends");
        assert!(!waits_on(working, STARVED_WAIT_CEILING), "past the ceiling: ends, bounded");
    }
}
