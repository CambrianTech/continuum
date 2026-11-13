//! Lane send — POST a generation to the local lane and ride out a mid-relaunch
//! (connection refused / 503 loading) with a bounded retry, then classify a rejected
//! status at the seam. Carved out of `openai_adapter::generate_stream` (pure code-motion,
//! 2026-09-03, the S3b decompose). Behaviour-identical to the inline block.

use std::time::Instant;

use crate::ai::inference_error::InferenceError;
use crate::ai::openai_adapter::OpenAICompatibleConfig;

/// FLOOR on the wait for response HEADERS after POSTing a generation — the
/// pre-stream twin of [`STREAM_IDLE_TIMEOUT_SECS`]. Covers the hung-prefill /
/// poisoned-backend case where the server accepts and never answers; sized for
/// a worst-case full-window prefill queued behind co-tenants (minutes), because
/// its job is releasing ETERNAL holds, not policing slow ones.
///
/// A floor, not the bound (card ba82d0a0): a request that carries its own
/// `turn_bound` — the mind's measured expectation with headroom
/// (`inference::turn_bound`) — waits `max(this, turn_bound)`. Measured 2026-09-20 on
/// the M5: 331–389 s to the first byte for a 27–30k cold prompt; the IntelMac at
/// ~25 tok/s needs ~1200 s. A constant here can only ever be wrong for one box.
///
/// On a local lane this is a CHECKPOINT interval, not a deadline (card 6f3218ed): the
/// wait continues while the engine's `/slots` work moves, so the bound decides how often
/// the engine is asked, and only an engine that stops moving is released.
// derived-or-floor: a floor under the request's own measured turn_bound (max of the two), and on a local lane only how often /slots progress is checked
pub const PRE_STREAM_HEADER_TIMEOUT_SECS: u64 = 300;

/// A local single-resident lane can be RELAUNCHED out from under an in-flight POST —
/// grow-back (#214), a genome page-in, or memory pressure all bounce the llama-server
/// process, and the published serving snapshot can lag at `ready=true` for the ~seconds
/// the socket is actually refused (the pre-flight guard trusts the `watch` snapshot; the
/// socket is the ground truth, and a watch channel is inherently slightly behind the
/// process). A `connect` error is therefore "the lane is mid-relaunch", not "the lane is
/// gone": the connection never opened, so nothing was streamed to the sink, and
/// re-sending the SAME lane/model is idempotent — resilience, NOT a fallback
/// ([[fallbacks-are-illegal-fail-loud]]). Retry the connect with linear backoff
/// (1s, 2s, … ≈ 21s total) to ride out a relaunch, then fail loud if it never returns.
/// Scoped to the local resident lane — remote endpoints don't relaunch under us.
/// Glass-boxed 2026-07-20: one legitimate grow-back relaunch zeroed hard-rs 0/8, every
/// task `Connection refused (os error 61)` to :58057 mid-eval.
const LANE_RELAUNCH_CONNECT_RETRIES: u32 = 6;
const LANE_RELAUNCH_RETRY_BASE: std::time::Duration = std::time::Duration::from_secs(1);

/// How many quiet checkpoints (no slot counter moved AND no slot processing) a header
/// wait allows before it calls the lane dead. One, not the page switch's eight: a
/// save/restore is invisible to the counters, but a generation is not, and an engine with
/// nothing processing while our POST waits has not picked the request up.
// derived-or-floor: a floor of one checkpoint of grace for the probe landing between a request's accept and its first ubatch; it only ever ENDS a wait that shows no progress.
const HEADER_WAIT_QUIET_CHECKPOINTS: u32 = 1;

/// The engine root (`http://127.0.0.1:<port>`) whose `/slots` a header wait may read:
/// only a local single-resident lane, the one kind that serves `/slots` and that the
/// probe's reading describes.
fn engine_root(cfg: &OpenAICompatibleConfig) -> Option<String> {
    if !cfg.single_resident_model {
        return None;
    }
    let trimmed = cfg.base_url.trim_end_matches('/');
    Some(trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()) // a base without /v1 is already the engine root: a form, not a missing value
}


/// What one send carried, for the receipt a tripped wait leaves behind: the model the
/// lane was asked for and the prompt it was asked to prefill (chars/4 —
/// `serving_guard::approx_prompt_tokens`, the same estimate the overflow guard uses).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FillReceipt<'a> {
    pub model: &'a str,
    pub prompt_tokens: usize,
    pub prompt_quote: Option<PromptQuote<'a>>,
    /// The admission the request was rendered under: at dispatch, after the wait for the lane, its
    /// lease on her grant is RESERVED (`window_grant::reserve`) against her grant now, with the
    /// engine's exact count, or the send is refused typed.
    pub admission: Option<DispatchAdmission>,
}

/// A request's admission and whose grant it was rendered against.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DispatchAdmission {
    pub persona: uuid::Uuid,
    pub admission: crate::ai::types::RequestAdmission,
    /// The engine incarnation (pid, OS start time) the request's endpoint admission is bound to:
    /// the lease must name it, or the attempt is refused (the published lease is only a watch value).
    pub engine: Option<(u32, u64)>,
}

/// Request-specific authority for the engine's finalized-prompt counter. The adapter
/// supplies this only for its managed local engine, not every OpenAI-compatible
/// provider or every gateway that happens to serve one resident model.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PromptQuote<'a> {
    pub client: &'a reqwest::Client,
    pub dedicated_lane: bool,
    pub caller: &'a str,
    pub reply: u32,
    pub request_id: &'a str,
}

/// Send `body` through `request_builder` (headers already set). `Ok(response)` is a 2xx response ready to stream; every failure is the
/// typed turn-facing failure, already probed. `body` owns one prepared encoding;
/// transport retries share its allocation except when an admitted engine quote
/// reprices the explicit reply allowance; the prompt remains unchanged.
/// `turn_bound` is the request's own bound (`TextGenerationRequest::turn_bound`); the
/// header wait is `max(PRE_STREAM_HEADER_TIMEOUT_SECS, turn_bound)`. `fill` is what the
/// turn sent, so a tripped header wait can be recorded as the prefill measurement it is
/// (`prefill_rate::observe_bound`, card c30a4757) — the one seam that knows both the
/// prompt and the wait.
pub(crate) async fn send_with_lane_retry(
    cfg: &OpenAICompatibleConfig,
    request_builder: reqwest::RequestBuilder,
    body: Vec<u8>,
    turn_bound: Option<std::time::Duration>,
    mut fill: FillReceipt<'_>,
    // Where the request's reservation of her grant is kept until its terminal receipt (the caller
    // drops it when the stream ends).
    reservation: &mut Option<crate::capacity::window_grant::GrantReservation>,
    // Told at each attempt's POST, never before: a request refused before it (a quote past the
    // window, a changed grant) reached no engine, and its turn admission must know that.
    mut dispatching: impl FnMut() + Send,
) -> Result<reqwest::Response, InferenceError> {
    // The header wait in force for THIS turn: the floor, or the request's measured bound
    // above it. Computed once — the relaunch retries below re-arm the same wait.
    let (header_bound, header_source) = crate::inference::turn_bound::effective_bound_with_source(
        std::time::Duration::from_secs(PRE_STREAM_HEADER_TIMEOUT_SECS),
        turn_bound,
    );
    // reqwest moves this Vec into its shared byte body. Cloning the prepared
    // builder below shares that allocation, including across transport retries.
    let mut request_builder = request_builder.body(body);
    // A CONNECT error to a local resident lane means the lane is mid-relaunch, not
    // gone (see LANE_RELAUNCH_CONNECT_RETRIES) — the connection never opened so
    // nothing streamed, and re-sending the same lane is idempotent. Ride it out with
    // bounded linear backoff, then fail loud.
    // The prepared byte body is cloneable; no stream or serializer is replayed.
    // Shared budget for BOTH mid-relaunch signatures (connection refused = nothing
    // listening yet; 503 = listening but still loading). One counter, so the total
    // time this call can spend waiting on a relaunching lane stays bounded.
    let mut relaunch_retries: u32 = 0;
    // Prompt counting is pre-stream wire work, governed by the same measured
    // bound as headers. Queue admission and KV paging have already completed;
    // closed-endpoint readiness patience must not debit those waits from a quote
    // that has not started (fe7960bf: healthy paging left a zero quote timeout).
    // Keep one cumulative quote allowance across retries and their backoff; do not
    // rearm a fresh quote allowance for each replacement engine.
    let quote_started = Instant::now();
    let response = loop {
        let mut attempt_reply = fill.prompt_quote.map_or(0, |quote| quote.reply);
        if let Some(quote) = fill.prompt_quote {
            // A transport retry may reach a replacement engine. Re-count the
            // same prepared body on EVERY attempt; never reuse its predecessor's
            // template/projector count. A failed quote refuses this attempt.
            super::serving_guard::guard_resident_model(
                cfg, quote.dedicated_lane, fill.model, super::serving_guard::PromptCount::Estimated(0), quote.caller,
            ).await?;
            let count_started = Instant::now();
            fill.prompt_tokens = super::serving_guard::quote_prompt(
                quote.client,
                &request_builder,
                header_bound.saturating_sub(quote_started.elapsed()),
            ).await?;
            let count_elapsed_ms = count_started.elapsed().as_millis() as u64;
            // ONE bound per request (CONTINUUM-ENGINE.md, serving engine: admission (4.8)). An admitted request is bounded
            // by its grant: admission sized it against the engine's own geometry and holds
            // that much of the pool for this act, which may be more than an equal share. The
            // lane's published share is the bound only for a caller with no grant. Checking an
            // admitted request against the share too refused every act whose grant passed it:
            // the M5, 2026-11-11 16:13Z, a 22,949-token prompt refused at a 136,517 grant
            // against the 104,857 share, then landing on the retry sized to the share.
            match fill.admission {
                Some(DispatchAdmission { admission, .. }) => {
                    // The quote awaited the engine, which may have been replaced meanwhile:
                    // residency again (Codex on #5947), then the size against her grant.
                    super::serving_guard::guard_resident_model(
                        cfg, quote.dedicated_lane, fill.model, super::serving_guard::PromptCount::Estimated(0), quote.caller,
                    ).await?;
                    (request_builder, attempt_reply) = measured_reply(
                        request_builder, fill.prompt_tokens, admission.window, quote.reply,
                    )?;
                    super::serving_guard::PromptCount::Measured { prompt: fill.prompt_tokens, reply: attempt_reply }
                        .check(admission.window, &cfg.name, quote.caller)?;
                }
                None => super::serving_guard::guard_resident_model(cfg, quote.dedicated_lane, fill.model,
                    super::serving_guard::PromptCount::Measured { prompt: fill.prompt_tokens, reply: quote.reply }, quote.caller).await?,
            }
            continuum_base::probe!(
                class = "inference.prompt.prompt_quote",
                request_id = quote.request_id,
                model = fill.model,
                input_tokens = fill.prompt_tokens as u64,
                elapsed_ms = count_elapsed_ms,
                attempt = relaunch_retries,
                "selected engine counted the finalized prompt"
            );
        }
        // Her lease on her grant, re-reserved before EVERY attempt's POST (after the lane wait, with
        // the count just taken on this attempt's engine): a grant that moved since render, a
        // replacement engine, or a count past it refuses typed and nothing is sent; a reserved
        // lease holds every regrant off until the engine's terminal receipt.
        if let Some(DispatchAdmission { persona, admission, engine }) = fill.admission {
            if let Some(leased) = admission.lease {
                // Bound to the engine this request's endpoint admission holds, never inferred from
                // the published grants: a lease on any other incarnation (or none known) is refused.
                let leased_on = leased.engine.map(|e| (e.pid, e.started_s));
                if engine.is_none() || leased_on != engine {
                    *reservation = None;
                    return Err(InferenceError::GrantChanged { rendered: leased.tokens, current: None });
                }
                let needed = (fill.prompt_tokens as u64)
                    .saturating_add(u64::from(attempt_reply))
                    .min(u64::from(u32::MAX)) as u32;
                crate::capacity::window_grant::reserve_attempt(reservation, persona, leased, needed).map_err(|refused| match refused {
                    crate::capacity::window_grant::ReserveRefused::Changed { leased, current } => {
                        InferenceError::GrantChanged { rendered: leased.tokens, current: current.map(|lease| lease.tokens) }
                    }
                    crate::capacity::window_grant::ReserveRefused::Retiring { leased } => {
                        InferenceError::GrantChanged { rendered: leased.tokens, current: None }
                    }
                    crate::capacity::window_grant::ReserveRefused::Over { count, grant } => {
                        InferenceError::ContextExceeded { requested: count, available: grant }
                    }
                })?;
            }
        }
        let send_start = Instant::now();
        let attempt_builder = request_builder
            .try_clone()
            .expect("prepared byte request is cloneable"); // JUSTIFIED: body is owned bytes, never a stream

        // BOUNDED pre-first-byte wait: a poisoned lane can accept the request
        // and never return headers (hung prefill) — with no bound here, the
        // caller's ServingLanePermit is held FOREVER and one wedged call
        // starves the whole roster's admission (glass-boxed 2026-07-23: the
        // eternal `nondirected_waiting` park). The stream idle-watchdog only
        // arms AFTER headers; this is its pre-stream twin. Generous (prefill
        // of a full window on a busy co-tenant lane is minutes, not seconds)
        // but FINITE — RTOS rule: every hold is bounded — and sized from the
        // turn's MEASURED expectation when it carries one, never a constant
        // shorter than this box's prefill (card ba82d0a0).
        //
        // BUSY IS NOT DEAD (card 6f3218ed). On a local lane the bound is a CHECKPOINT,
        // not a verdict: at each one the engine's `/slots` work fingerprint is read, and
        // while it moves (this request prefilling, or queued behind a slot that is) the
        // wait goes on. Measured on the IntelMac, 2026-09-27, with no build running: six
        // slots, prefill one 2,048-token ubatch per ~45 s, and 42 of 45 generations
        // killed here at 300 s while every one of them was queued or advancing. Only an
        // engine that stops moving, stays quiet past one checkpoint, or stops answering
        // ends the wait. A remote endpoint has no `/slots` to read: elapsed, as before.
        let send_started = Instant::now();
        dispatching();
        let sent = match engine_root(cfg) {
            Some(root) => crate::inference::llama_server::wait_while_engine_progresses(
                attempt_builder.send(),
                // processing and unmoved is judged over the measured batch window, as every engine wait is
                crate::inference::llama_server::WaitBounds::for_model(header_bound, Some(fill.model)),
                HEADER_WAIT_QUIET_CHECKPOINTS,
                || crate::inference::llama_server::engine_probe(&root, crate::inference::engine_client()),
                |checkpoints| {
                    continuum_base::probe!(
                        class = "inference.header_wait.busy_not_dead",
                        provider = %cfg.name,
                        checkpoints,
                        waited_ms = send_started.elapsed().as_millis() as u64,
                        bound_ms = header_bound.as_millis() as u64,
                        prompt_tokens = fill.prompt_tokens as u64,
                        "no response headers yet, but the engine's work moved since the last checkpoint — busy, not dead; waiting on"
                    );
                },
            )
            .await
            .ok_or(()),
            None => tokio::time::timeout(header_bound, attempt_builder.send()).await.map_err(|_| ()),
        };
        let sent = sent
            .map_err(|_| {
                crate::inference::turn_bound::probe_tripped(
                    "pre_stream_headers",
                    &cfg.name,
                    header_bound,
                    header_source,
                    turn_bound,
                );
                // The trip IS a measurement: `prompt_tokens` sent, no headers inside
                // `header_bound` — the lane prefilled at most that fast. Recorded so the
                // next fill cap is derived from this failure, not from the last success
                // (card c30a4757: 25 of 29 turns tripped on a stale rate that no trip
                // could move).
                let waited = send_started.elapsed();
                crate::inference::prefill_rate::observe_bound(fill.model, fill.prompt_tokens, waited);
                format!(
                    "{}: no response headers after {}s, and the engine's work did not move \
                     through a {}s checkpoint ({} bound) — hung prefill / poisoned backend; \
                     releasing the lane instead of holding it forever",
                    cfg.name,
                    waited.as_secs(),
                    header_bound.as_secs(),
                    header_source.as_str()
                )
            })?;
        match sent {
            // A relaunching lane refuses the connection only while nothing is
            // LISTENING. Once the new process binds, it accepts and answers
            // 503 while it mmaps weights and warms the backend — the SAME
            // mid-relaunch state one layer up, with a completely different
            // signature. Observed live 2026-08-07: a re-home grew the window
            // 16384 → 27136 → 32768, and during the respawn three citizens
            // took `503 {"error":{"message":"Loading model..."}}` as a hard
            // `selftick.inference_failed` while the published snapshot still
            // said `ready` (it is a cached claim — see ServingSnapshot::ready).
            //
            // 503 from a SINGLE-RESIDENT local lane means "not available yet"
            // by definition, so the status alone is the signal — no sniffing
            // the body text for "Loading model"
            // ([[a-string-matcher-for-a-semantic-judgement-means-a-channel-is-missing]]:
            // the HTTP status IS the structured channel). Shares the connect
            // arm's retry budget, because both are the same wait for the same
            // lane and the total hold must stay bounded.
            Ok(resp)
                if resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
                    && cfg.single_resident_model
                    && relaunch_retries < LANE_RELAUNCH_CONNECT_RETRIES =>
            {
                relaunch_retries += 1;
                let backoff = LANE_RELAUNCH_RETRY_BASE * relaunch_retries;
                continuum_base::probe!(
                    class = "inference.lane_relaunch_retry",
                    provider = cfg.provider_id.as_str(),
                    attempt = relaunch_retries,
                    backoff_ms = backoff.as_millis() as u64,
                    reason = "503_loading",
                    "local lane is up but still loading (503, mid-relaunch) — retrying the \
                     same lane",
                );
                tokio::time::sleep(backoff).await;
                continue;
            }
            Ok(resp) => break resp,
            Err(e)
                if e.is_connect()
                    && cfg.single_resident_model
                    && relaunch_retries < LANE_RELAUNCH_CONNECT_RETRIES =>
            {
                relaunch_retries += 1;
                let backoff = LANE_RELAUNCH_RETRY_BASE * relaunch_retries;
                continuum_base::probe!(
                    class = "inference.lane_relaunch_retry",
                    provider = cfg.provider_id.as_str(),
                    attempt = relaunch_retries,
                    backoff_ms = backoff.as_millis() as u64,
                    reason = "connect_refused",
                    "local lane refused the connection (mid-relaunch) — retrying the same lane",
                );
                tokio::time::sleep(backoff).await;
                continue;
            }
            Err(e) => {
                return Err(InferenceError::Unavailable(format!(
                    "{} POST failed after {}ms{}: {}",
                    cfg.name,
                    send_start.elapsed().as_millis(),
                    if relaunch_retries > 0 {
                        format!(
                            " ({relaunch_retries} mid-relaunch retries exhausted — lane never came back)"
                        )
                    } else {
                        String::new()
                    },
                    crate::ai::inference_error::transport_diagnostic(&e)
                )));
            }
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = match response.text().await {
            Ok(body) => body,
            Err(read) => format!("<error body unreadable: {read}>"),
        };
        // Classify ONCE, here, where the status code and the raw body still
        // exist. Preserve the same classification for the caller's admission
        // decision; prose is only an operator-facing representation.
        let classified = InferenceError::from_http(status.as_u16(), &body);
        let (requested, available) = match &classified {
            crate::ai::inference_error::InferenceError::ContextExceeded {
                requested,
                available,
            } => (*requested, *available),
            _ => (0, 0),
        };
        continuum_base::probe!(
            class = "ai.request.rejected",
            provider = %cfg.name,
            status = status.as_u16(),
            retryable_unchanged = classified.is_retryable_unchanged(),
            requested_tokens = requested,
            available_tokens = available,
            "backend rejected the request — classified at the seam"
        );
        return Err(classified);
    }

    Ok(response)
}

/// The engine measured the prompt on this attempt. Reprice the explicit reply against
/// its remaining grant, retaining the caller's original limit across transport retries.
/// Both the wire body and the reservation use this same allowance. Never turn an empty
/// remainder into a zero/unlimited output sentinel.
fn measured_reply(
    builder: reqwest::RequestBuilder,
    prompt: usize,
    grant: u32,
    requested: u32,
) -> Result<(reqwest::RequestBuilder, u32), InferenceError> {
    let prompt = u32::try_from(prompt)
        .map_err(|_| InferenceError::Protocol("measured prompt exceeds the capacity receipt's token range".into()))?;
    if requested == 0 {
        return Ok((builder, requested));
    }
    // PromptCount::check and the engine reject totals >= the window; one below is the exact edge.
    let remaining = grant.checked_sub(prompt).and_then(|n| n.checked_sub(1))
        .filter(|n| *n > 0)
        .ok_or(InferenceError::ContextExceeded { requested: prompt, available: grant })?;
    let reply = requested.min(remaining);
    let request = builder.try_clone()
        .ok_or_else(|| InferenceError::Protocol("reply pricing requires a replayable request".into()))?
        .build().map_err(|error| InferenceError::Protocol(format!("reply pricing request: {error}")))?;
    let bytes = request.body().and_then(reqwest::Body::as_bytes)
        .ok_or_else(|| InferenceError::Protocol("reply pricing requires a prepared byte body".into()))?;
    let mut value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| InferenceError::Protocol(format!("reply pricing JSON: {error}")))?;
    let output = value.as_object_mut().and_then(|object| object.get_mut("max_tokens"))
        .ok_or_else(|| InferenceError::Protocol("explicit reply allowance has no max_tokens field".into()))?;
    *output = serde_json::Value::from(reply);
    let bytes = serde_json::to_vec(&value) // boundary: OpenAI-compatible HTTP POST body sent to the engine
        .map_err(|error| InferenceError::Protocol(format!("reply pricing encoding: {error}")))?;
    Ok((builder.body(bytes), reply))
}

#[cfg(test)]
mod measured_reply_tests {
    use super::*;

    #[test]
    fn measured_prompt_reprices_wire_reply_and_retry_keeps_original_limit() {
        let original = serde_json::json!({"messages": [{"role": "user", "content": "same prompt"}], "max_tokens": 120000});
        let builder = reqwest::Client::new().post("http://localhost/chat/completions").json(&original);
        let (builder, reply) = measured_reply(builder, 66780, 181826, 120000).expect("prompt fits");
        assert_eq!(reply, 115045);
        let wire = builder.try_clone().expect("replayable").build().expect("request");
        let body: serde_json::Value = serde_json::from_slice(wire.body().expect("body").as_bytes().expect("bytes")).expect("JSON");
        assert_eq!(body["max_tokens"], reply);
        assert_eq!(body["messages"], original["messages"]);
        let (_, retry_reply) = measured_reply(builder, 60000, 181826, 120000).expect("retry quote fits");
        assert_eq!(retry_reply, 120000, "a smaller retry quote may restore the original allowance");
    }

    #[test]
    fn measured_reply_preserves_small_limits_and_refuses_no_output_room() {
        let request = || reqwest::Client::new().post("http://localhost/chat/completions")
            .json(&serde_json::json!({"max_tokens": 12}));
        assert_eq!(measured_reply(request(), 50, 100, 12).expect("small explicit limit").1, 12);
        assert!(matches!(measured_reply(request(), 99, 100, 12), Err(InferenceError::ContextExceeded { .. })));
        assert!(matches!(measured_reply(request(), 100, 100, 12), Err(InferenceError::ContextExceeded { .. })));
    }
}
