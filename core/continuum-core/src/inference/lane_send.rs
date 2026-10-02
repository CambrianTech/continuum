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
pub(crate) const PRE_STREAM_HEADER_TIMEOUT_SECS: u64 = 300;

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
    Some(trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string())
}

/// One client for the header wait's engine probes, built once per process.
fn probe_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// What one send carried, for the receipt a tripped wait leaves behind: the model the
/// lane was asked for and the prompt it was asked to prefill (chars/4 —
/// `serving_guard::approx_prompt_tokens`, the same estimate the overflow guard uses).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FillReceipt<'a> {
    pub model: &'a str,
    pub prompt_tokens: usize,
    pub image_quote: Option<ImageQuote<'a>>,
}

/// Request-specific authority for the engine's multimodal counter. The adapter
/// supplies this only for its managed local engine, not every OpenAI-compatible
/// provider or every gateway that happens to serve one resident model.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImageQuote<'a> {
    pub client: &'a reqwest::Client,
    pub dedicated_lane: bool,
    pub caller: &'a str,
    pub patience: std::time::Duration,
    pub reply: u32,
    pub request_id: &'a str,
}

/// Send `body` through `request_builder` (headers already set). `Ok(response)` is a 2xx response ready to stream; every failure is the
/// typed turn-facing failure, already probed. `body` owns one prepared encoding;
/// transport retries share its allocation rather than serializing again.
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
) -> Result<reqwest::Response, InferenceError> {
    // The header wait in force for THIS turn: the floor, or the request's measured bound
    // above it. Computed once — the relaunch retries below re-arm the same wait.
    let (header_bound, header_source) = crate::inference::turn_bound::effective_bound_with_source(
        std::time::Duration::from_secs(PRE_STREAM_HEADER_TIMEOUT_SECS),
        turn_bound,
    );
    // reqwest moves this Vec into its shared byte body. Cloning the prepared
    // builder below shares that allocation, including across transport retries.
    let request_builder = request_builder.body(body);
    // A CONNECT error to a local resident lane means the lane is mid-relaunch, not
    // gone (see LANE_RELAUNCH_CONNECT_RETRIES) — the connection never opened so
    // nothing streamed, and re-sending the same lane is idempotent. Ride it out with
    // bounded linear backoff, then fail loud.
    // The prepared byte body is cloneable; no stream or serializer is replayed.
    // Shared budget for BOTH mid-relaunch signatures (connection refused = nothing
    // listening yet; 503 = listening but still loading). One counter, so the total
    // time this call can spend waiting on a relaunching lane stays bounded.
    let mut relaunch_retries: u32 = 0;
    let quote_started = Instant::now();
    let response = loop {
        if let Some(quote) = fill.image_quote {
            // A transport retry may reach a replacement engine. Re-count the
            // same prepared body on EVERY attempt; never reuse its predecessor's
            // template/projector count. A failed quote refuses this attempt.
            super::serving_guard::guard_resident_model(
                cfg, quote.dedicated_lane, fill.model, super::serving_guard::PromptCount::Estimated(0), quote.caller,
            ).await?;
            fill.prompt_tokens = super::serving_guard::quote_image_prompt(
                quote.client,
                &request_builder,
                quote.patience.saturating_sub(quote_started.elapsed()),
            ).await?;
            super::serving_guard::guard_resident_model(
                cfg, quote.dedicated_lane, fill.model, super::serving_guard::PromptCount::Measured { prompt: fill.prompt_tokens, reply: quote.reply }, quote.caller,
            ).await?;
            crate::probe!(
                class = "inference.prompt.image_quote",
                request_id = quote.request_id,
                model = fill.model,
                input_tokens = fill.prompt_tokens as u64,
                attempt = relaunch_retries,
                "selected engine counted the finalized image-bearing prompt"
            );
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
        let sent = match engine_root(cfg) {
            Some(root) => crate::inference::llama_server::wait_while_engine_progresses(
                attempt_builder.send(),
                header_bound,
                HEADER_WAIT_QUIET_CHECKPOINTS,
                || crate::inference::llama_server::engine_probe(&root, probe_client()),
                |checkpoints| {
                    crate::probe!(
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
                crate::probe!(
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
                crate::probe!(
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
                // reqwest::Error's top-level Display often collapses the
                // real cause (timeout vs connect vs body-write) into a
                // generic "error sending request" string. Walk the error
                // source chain so the log shows the actual terminal
                // reason — critical for debugging stalls where the
                // outer message alone is useless.
                let mut chain: Vec<String> = vec![e.to_string()];
                let mut cur: &dyn std::error::Error = &e;
                while let Some(src) = cur.source() {
                    chain.push(src.to_string());
                    cur = src;
                }
                return Err(InferenceError::Unavailable(format!(
                    "{} POST failed after {}ms{}: {} (kind: timeout={}, connect={}, request={}, body={})",
                    cfg.name,
                    send_start.elapsed().as_millis(),
                    if relaunch_retries > 0 {
                        format!(
                            " ({relaunch_retries} mid-relaunch retries exhausted — lane never came back)"
                        )
                    } else {
                        String::new()
                    },
                    chain.join(" -> "),
                    e.is_timeout(),
                    e.is_connect(),
                    e.is_request(),
                    e.is_body()
                )));
            }
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
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
        crate::probe!(
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
