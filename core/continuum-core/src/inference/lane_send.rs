//! Lane send — POST a generation to the local lane and ride out a mid-relaunch
//! (connection refused / 503 loading) with a bounded retry, then classify a rejected
//! status at the seam. Carved out of `openai_adapter::generate_stream` (pure code-motion,
//! 2026-09-03, the S3b decompose). Behaviour-identical to the inline block.

use std::time::Instant;

use serde_json::Value;

use crate::ai::openai_adapter::OpenAICompatibleConfig;

/// Bound on the wait for response HEADERS after POSTing a generation — the
/// pre-stream twin of [`STREAM_IDLE_TIMEOUT_SECS`]. Covers the hung-prefill /
/// poisoned-backend case where the server accepts and never answers; sized for
/// a worst-case full-window prefill queued behind co-tenants (minutes), because
/// its job is releasing ETERNAL holds, not policing slow ones.
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

/// Send `body` through `request_builder` (headers already set, bodyless so it clones per
/// attempt). `Ok(response)` is a 2xx response ready to stream; every failure is the
/// turn-facing error text, already probed.
pub(crate) async fn send_with_lane_retry(
    cfg: &OpenAICompatibleConfig,
    request_builder: reqwest::RequestBuilder,
    body: &Value,
) -> Result<reqwest::Response, String> {
    // A CONNECT error to a local resident lane means the lane is mid-relaunch, not
    // gone (see LANE_RELAUNCH_CONNECT_RETRIES) — the connection never opened so
    // nothing streamed, and re-sending the same lane is idempotent. Ride it out with
    // bounded linear backoff, then fail loud. `request_builder` carries no body yet
    // (`.json` is applied per attempt below), so `try_clone` always succeeds.
    // Shared budget for BOTH mid-relaunch signatures (connection refused = nothing
    // listening yet; 503 = listening but still loading). One counter, so the total
    // time this call can spend waiting on a relaunching lane stays bounded.
    let mut relaunch_retries: u32 = 0;
    let response = loop {
        let send_start = Instant::now();
        let attempt_builder = request_builder
            .try_clone()
            .expect("bodyless request builder is always cloneable");
        // BOUNDED pre-first-byte wait: a poisoned lane can accept the request
        // and never return headers (hung prefill) — with no bound here, the
        // caller's ServingLanePermit is held FOREVER and one wedged call
        // starves the whole roster's admission (glass-boxed 2026-07-23: the
        // eternal `nondirected_waiting` park). The stream idle-watchdog only
        // arms AFTER headers; this is its pre-stream twin. Generous (prefill
        // of a full window on a busy co-tenant lane is minutes, not seconds)
        // but FINITE — RTOS rule: every hold is bounded.
        let sent = tokio::time::timeout(
            std::time::Duration::from_secs(PRE_STREAM_HEADER_TIMEOUT_SECS),
            attempt_builder.json(body).send(),
        )
        .await
        .map_err(|_| {
            format!(
                "{}: no response headers for {}s after POST — lane accepted the                      request and went silent (hung prefill / poisoned backend);                      releasing the lane instead of holding it forever",
                cfg.name, PRE_STREAM_HEADER_TIMEOUT_SECS
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
                return Err(format!(
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
                ));
            }
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        // Classify ONCE, here, where the status code and the raw body still
        // exist. Downstream this is still carried as a String (the trait's
        // error type has not moved yet — that is the threading commit), so
        // the CLASSIFICATION is emitted as a probe rather than lost: an
        // operator reading the receipt now sees WHICH kind of failure this
        // was, and for an overflow, both token counts.
        //
        // Why that matters: settle.rs retries every fault blind, which is
        // right for a transient wedge (#386, ~2/3 recover) and useless for
        // ContextExceeded — same prompt, same slot, same 400, forever.
        // Until the type reaches settle, this probe is the only place the
        // difference is visible at all.
        let classified = crate::ai::inference_error::InferenceError::from_http(
            status.as_u16(),
            &body,
        );
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
        return Err(format!("{} returned {}: {}", cfg.name, status, body));
    }

    Ok(response)
}
