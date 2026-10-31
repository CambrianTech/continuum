//! External responsibility of the llama-server process owner.
use super::*;
/// The operator-pinned EXTERNAL OpenAI-compatible serving endpoint ROOT
/// (`http://host:port`, no `/v1`), if `LLAMA_SERVER_BASE_URL` is set in the
/// config (read via [`continuum_base::config_env::read`] — the `~/.continuum/config.env`
/// FILE, not a process env var). When set, this node does not own a local GPU
/// serving lane; persona hosting adopts the pinned endpoint. `serving_root()`
/// already resolves to this same value — this predicate just answers "is one
/// pinned", the seam the hosting path branches on.
pub fn external_serving_pin() -> Option<String> {
    let raw = continuum_base::config_env::read("LLAMA_SERVER_BASE_URL")?;
    let trimmed = raw.trim().trim_end_matches('/');
    let root = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    (!root.is_empty()).then(|| root.to_string())
}


/// Read the resident model id an external endpoint reports at `/v1/models`,
/// accepting BOTH the canonical OpenAI shape (`data[].id`) and the
/// llama.cpp/Ollama-compat shape some builds answer (`models[].model|name` — a
/// K3 `llama-server` does exactly this). Returns `None` only if neither shape
/// names a model; the caller substitutes a stable sentinel (a single-resident
/// endpoint ignores the request `model` field anyway).
/// Reachability gate for a pinned EXTERNAL endpoint: `/health` answers 200.
///
/// Why NOT the local-lane [`decode_smoke_ok`] (a 5+ token generation): that bar
/// exists to catch an *untrusted orphan* whose GPU compute path is wedged. It
/// doubles as a speed test — a legitimately slow external lane (a CPU-offloaded
/// MoE like K3 at ~0.03 tok/s under load, or a distant grid peer) cannot finish a
/// multi-token generation inside any sane probe deadline, and worse, a
/// client-side timeout does NOT cancel the server-side generation, so a probe
/// storm monopolizes the endpoint's slot and NOTHING ever hosts. The operator
/// DELIBERATELY pinned this endpoint, so the adopt question is "is it reachable +
/// serving a model" (proved here + by the `/props` window read + the `/v1/models`
/// model read in [`probe_external_serving`]); the decode path is exercised by the
/// persona's real turns, where a genuine wedge (every turn 500s) surfaces LOUD on
/// the first turn — not silently faked. Cheap, off the HTTP layer, so no slot
/// contention and no probe storm.
pub(super) async fn external_health_ok(root: &str, client: &reqwest::Client) -> bool {
    let url = format!("{root}/health");
    matches!(
        client.get(&url).timeout(PROBE_TIMEOUT).send().await,
        Ok(resp) if resp.status().is_success()
    )
}


pub(super) async fn external_active_model(v1_url: &str, client: &reqwest::Client) -> Option<String> {
    let url = format!("{v1_url}/models");
    let body: serde_json::Value = client
        .get(&url)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    // OpenAI: { "data": [ { "id": "..." } ] }
    if let Some(id) = body
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|m| m.get("id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    // llama.cpp/Ollama-compat: { "models": [ { "model": "...", "name": "..." } ] }
    body.get("models")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|m| m.get("model").or_else(|| m.get("name")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}


/// Probe a pinned EXTERNAL OpenAI-compatible endpoint for decode-readiness and,
/// on success, synthesize the ready [`ServingSnapshot`] persona hosting binds
/// against — the SAME shape the local serving daemon publishes, so every reader
/// ([`await_ready_serving`], the persona adapter factory) stays source-agnostic.
///
/// The endpoint is held to the IDENTICAL bar as a local lane: [`decode_smoke_ok`]
/// runs a real multi-token generation, so an endpoint that answers `/v1/models`
/// 200 but 500s (or dribbles ~2 tokens on) every decode is rejected, never
/// adopted ([[fallbacks-are-illegal-fail-loud]]). `served_context_window` comes
/// from the endpoint's own `/props` (`n_ctx`) so personas budget prompts to the
/// truth. Returns `None` if unset, unreachable, wedged, or `/props` won't name a
/// window (a decode-ready endpoint that can't report its context is not safely
/// adoptable — never a guessed window).
pub async fn probe_external_serving(timeout: Duration) -> Option<ServingSnapshot> {
    let _ = timeout; // reachability gate is control-plane (fast); no generation to bound.
    let root = external_serving_pin()?;
    let v1_url = format!("{root}/v1");
    let client = reqwest::Client::new();

    // Reachability gate (see `external_health_ok` for why not a generation probe
    // against a deliberately-pinned, possibly-very-slow endpoint).
    if !external_health_ok(&root, &client).await {
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.external_probe",
            endpoint = root.as_str(),
            "pinned external serving endpoint is unreachable (/health) — \
             not adopting; will retry on the next serving edge",
        );
        return None;
    }

    // The authoritative per-slot window from the endpoint's own `/props`. A
    // decode-ready endpoint that won't name its window is not safely adoptable.
    let served_context_window = match LlamaServerProcess::with_root(root.clone())
        .served_context_window()
        .await
    {
        Ok(w) if w > 0 => w,
        _ => {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.external_probe",
                endpoint = root.as_str(),
                "pinned external endpoint decodes but `/props` did not report an n_ctx window — \
                 refusing to adopt with a guessed window",
            );
            return None;
        }
    };

    let active_model = external_active_model(&v1_url, &client)
        .await
        .unwrap_or_else(|| "external-serving-lane".to_string());

    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.external_adopt",
        endpoint = root.as_str(),
        model = active_model.as_str(),
        window = served_context_window,
        "external serving endpoint adopted as the persona lane — this node hosts personas \
         without owning a local GPU lane (misfit / grid serving)",
    );

    Some(ServingSnapshot {
        // A lane with a verified window is serving, not loading.
        loading_model: None,
        held_for: None,
        active_model: Some(active_model),
        ready: true,
        // Personas point their inference adapter here; `serving_v1_url()` already
        // resolves to the pinned endpoint (operator-honored verbatim).
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window,
        // One shared lane. A future refinement can read `/props.total_slots`.
        lanes: 1,
        // An adopted endpoint's cache grant is not ours to know.
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        // NOT `Some(now)`. This path ADOPTS an endpoint the operator pinned; we
        // have confirmed it answers, not that it has ever delivered a token to
        // us. `ready_verified_at_ms` means exactly the latter, and stamping it
        // here would tell every downstream reader that a lane we have never
        // pulled a token from was confirmed this instant — which is the wedged-
        // slot failure the field was added to expose, manufactured by hand.
        // `None` = never confirmed, which is true and which readers already
        // handle.
        ready_verified_at_ms: None,
        // An external OpenAI-compatible endpoint does not tell us whether it can
        // see, and we have not asked. Absence of knowledge, reported as absence
        // — not as `false` meaning "we checked and it cannot".
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
    })
}


/// Wait until the served window has SETTLED after a serving-mode change that may have triggered
/// a grow-relaunch (e.g. an exam declaring Ludicrous/Performance). The caller must be able to
/// pin the lane WITHOUT bouncing in-flight work, so it must know the relaunch is DONE — not
/// merely that the lane was ready a moment ago (it may be about to relaunch on the next 5s plan
/// tick). Returns the settled per-slot window, or `None` on timeout / no serving state.
///
/// Two phases, watch-driven (no polling, no time-based stability guessing):
///  1. Within `settle_grace` (must exceed the daemon's `TICK`, so a re-plan can fire), watch for
///     the lane to go NOT-ready — the signature of a relaunch. If it never does, no relaunch was
///     needed (the window was already at target) → return the current window immediately.
///  2. A relaunch happened → wait (bounded by `timeout`) for the lane to come back decode-ready
///     at its new window.
pub async fn wait_for_serving_window_settle(
    settle_grace: Duration,
    timeout: Duration,
) -> Option<u32> {
    let mut rx = SERVING_STATE.get()?.clone();
    let start = std::time::Instant::now();
    // Phase 1: did a relaunch start (lane went not-ready) within the grace window?
    let relaunched = tokio::time::timeout(settle_grace, rx.wait_for(|s| !s.ready))
        .await
        .is_ok();
    if !relaunched {
        // No relaunch — the window was already at target. Return it if ready.
        let s = rx.borrow_and_update();
        return s.ready.then_some(s.served_context_window);
    }
    // Phase 2: wait for the relaunched lane to come back decode-ready. Bind the timeout result
    // before matching so its `watch::Ref` temporary drops before `rx` does (else E0597).
    let remaining = timeout.saturating_sub(start.elapsed());
    let waited = tokio::time::timeout(remaining, rx.wait_for(|s| s.is_live())).await;
    match waited {
        Ok(Ok(guard)) => Some(guard.served_context_window),
        _ => None,
    }
}
