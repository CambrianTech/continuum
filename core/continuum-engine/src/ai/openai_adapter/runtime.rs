//! Construction, endpoint resolution and runtime model catalog. Slot-pool discovery is the slots
//! directory's (`SlotDirectory::discover_pool`).

use super::*;

impl OpenAICompatibleAdapter {
    /// Build the reqwest client for a STREAMING inference transport. There is
    /// deliberately NO total-request timeout: generation is a long-running job
    /// whose liveness is "is it still producing tokens?", not "did it finish
    /// within N seconds." A wall-clock cap is the wrong model — it kills a
    /// healthy-but-slow decode (a CPU-placed 4B mid-answer) at an arbitrary
    /// cliff. Liveness is enforced per-token by the idle watchdog in
    /// [`stream_completion`] (no token for [`STREAM_IDLE_TIMEOUT_SECS`] = the
    /// backend went silent → fail loud). `connect_timeout` still bounds the
    /// handshake to a dead loopback backend; `pool_idle_timeout` prevents stale
    /// pooled sockets across backend restarts.
    fn build_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client") // rustls-tls bundles webpki roots and the system resolver loads no host config, so these options cannot fail per machine: an error is a build misconfiguration every adapter test hits
    }

    pub fn new(config: OpenAICompatibleConfig) -> Self {
        // 120s total timeout bounds long generations (qwen3.5 reasoning
        // can take ~60s to emit a full response). Connect timeout bounds
        // the local-loopback DMR case specifically: when Docker Desktop
        // restarts or DMR isn't listening, we want the fast explicit
        // "connect refused" instead of a 120s stall. Idle timeout keeps
        // the reqwest pool from holding onto dead sockets across DMR
        // restarts — a stale pooled connection to a killed server was
        // the reproducing cause of 120s "error sending request" stalls.
        let client = Self::build_http_client();

        // Per-provider concurrency gate. A single-resident-model gateway
        // (DMR, llama-server) must not take more concurrent POSTs than it has
        // SLOTS — excess queues here instead of piling onto a busy forward
        // pass (the pre-streaming "error sending request -> operation timed
        // out" failure mode). Multi-model / cloud endpoints are effectively
        // unbounded. Keyed on the TYPED capability (#55), never the provider id.
        //
        // SIZED TO THE SERVED SLOTS, NOT 1 (B3, Joel 2026-08-30: "not cost a
        // lane, it's the same model"). llama-server continuous-batches its
        // `--parallel` slots — a short conversational reply slipstreams
        // between a solve's decode steps. The hardcoded 1 was the artificial
        // serializer every scarcity policy downstream compensated for: with 4
        // served slots, citizen speech queued behind a 40k-prefill solve.
        // A POOLED endpoint's admissions take that endpoint's own permits,
        // sized from its generation and resized on relaunch, shared by every
        // adapter and the learner (`slots::EndpointConcurrency`, card 23069702).
        // This semaphore bounds only traffic to an endpoint with no slot pool.
        let slots = if config.single_resident_model {
            crate::inference::llama_server::current_serving()
                .lanes
                .max(1) as usize
        } else {
            64
        };
        let concurrency = std::sync::Arc::new(tokio::sync::Semaphore::new(slots));

        Self {
            config,
            api_key: None,
            dedicated_lane: false,
            client,
            initialized: false,
            runtime_models: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            lora_support: std::sync::Arc::new(parking_lot::RwLock::new(LoraSupport::Unknown)),
            concurrency,
        }
    }

    /// Override the base URL at runtime (e.g. when running inside a Docker
    /// container on Windows/Linux where DMR is at model-runner.docker.internal
    /// instead of localhost:12434). Called post-construction, before init; the adapter
    /// owns its config, so the override IS its base URL from here on.
    pub fn with_runtime_base_url(mut self, url: String) -> Self {
        self.config.base_url = url;
        self
    }

    /// Mark this adapter as owning a dedicated, single-purpose serving lane (an
    /// `EphemeralServingLane`) rather than sharing the global gateway. The
    /// single-resident pre-flight guard then trusts THIS lane (readiness
    /// guaranteed at spawn) instead of the global serving snapshot. See
    /// [`OpenAICompatibleAdapter::dedicated_lane`]. Called post-construction,
    /// before init, paired with [`with_runtime_base_url`] pointing at the lane.
    pub fn with_dedicated_lane(mut self) -> Self {
        self.dedicated_lane = true;
        self
    }

    /// Override the default model id (the one a `request.model: None` resolves to).
    /// Used at persona upstart to bind the adapter to the model the engine ACTUALLY
    /// serves (discovered via `/v1/models`), instead of the the Rust catalog (catalog.rs) default
    /// — which can drift from what's loaded. Called post-construction, before init.
    pub fn with_default_model(mut self, model: String) -> Self {
        self.config.default_model = model;
        self
    }

    /// The typed OpenAI-compatible endpoint base — the runtime override if set,
    /// else the configured base. The ONE place request URLs are built; every
    /// site calls a typed accessor ([`OpenAiBase::chat_completions`] etc.) rather
    /// than concatenating `/v1/...` itself, so the protocol's path layout lives in
    /// exactly one place and the snapshot's `/v1` shape can no longer double to
    /// `/v1/v1/...` (THE Asha-mute bug). See [`crate::ai::openai_endpoints`].
    pub(super) fn endpoints(&self) -> OpenAiBase {
        OpenAiBase::new(&self.config.base_url)
    }

    /// Does this adapter's effective base URL target the LOCAL llama-server lane
    /// published in the serving snapshot? Data-driven attribution for the
    /// real-decode success/failure counters (#363): only requests that actually
    /// ride the local lane may stamp its record — a cloud provider's outage must
    /// never smear it, and a local success must never launder a cloud failure.
    /// URL equality against the ONE published snapshot, never a name sniff (#70).
    pub(super) fn targets_local_serving_lane(&self) -> bool {
        let snap = crate::inference::llama_server::current_serving();
        self.targets_serving_endpoint(&snap.base_url)
    }

    /// Does the engine this adapter reaches count each finalized prompt before generating it?
    /// The managed local lane does (`serving_guard::quote_prompt`); the one predicate for the
    /// quote itself and for [`AIProviderAdapter::counts_prompts_exactly`].
    ///
    /// [`AIProviderAdapter::counts_prompts_exactly`]: crate::ai::adapter::AIProviderAdapter::counts_prompts_exactly
    pub(super) fn exact_prompt_admission(&self) -> bool {
        self.config.single_resident_model && self.targets_local_serving_lane()
    }

    pub(super) fn targets_serving_endpoint(&self, serving_base: &str) -> bool {
        if serving_base.is_empty() {
            return false;
        }
        fn norm(s: &str) -> &str {
            s.trim_end_matches('/')
                .trim_end_matches("/v1")
                .trim_end_matches('/')
        }
        norm(&self.config.base_url) == norm(serving_base)
    }

    /// The endpoint base for a SPECIFIC requested model. Same as [`Self::endpoints`]
    /// except for the local serving gateway when the requested model is the
    /// serving snapshot's VISION model but not its main-lane model: that is the
    /// #106 vision SIDECAR (a VL lane beside a text-only mind), and the request
    /// routes to the snapshot's verified `vision_base_url`. Driven entirely by
    /// the ONE published snapshot (id-equality against the gateway's canonical
    /// [`PROVIDER_ID`](crate::inference::llama_server::PROVIDER_ID) const — no
    /// name sniffing), and the address exists only when `/props` confirmed
    /// sight, so pixels can never be aimed at a text lane.
    ///
    /// PURE over `snap`: the caller loads the snapshot once ([`Self::route_now`]) so the URL
    /// and every decision about the destination (its `tail_tools`) read the same route.
    pub(super) fn endpoints_for_model(
        &self,
        model: &str,
        base: &OpenAiBase,
        snap: &crate::inference::llama_server::ServingSnapshot,
    ) -> OpenAiBase {
        if self.config.provider_id == crate::inference::llama_server::PROVIDER_ID {
            if snap.vision_ready
                && snap.vision_model.as_deref() == Some(model)
                && snap.active_model.as_deref() != Some(model)
            {
                if let Some(url) = snap.vision_base_url.as_deref() {
                    return OpenAiBase::new(url);
                }
            }
        }
        base.clone()
    }

    /// The route a request for `model` takes NOW, and whether it may carry `tail_tools`, from ONE
    /// serving snapshot and one evidence read: the head (`accepts_tail_tools`) and the send
    /// (`stream_checked`) both take this pair, so the wire URL and the tail gate never come from
    /// two different snapshots (Codex on #5491).
    pub(super) fn route_now(&self, model: &str, base: &OpenAiBase) -> RequestRoute {
        self.route_for(
            model,
            base,
            &crate::inference::llama_server::current_serving(),
            crate::inference::llama_server::serving_engine_takes_tail_tools(),
        )
    }

    /// PURE: [`Self::route_now`] over one `snap`. The tail gate holds only where the route
    /// ([`Self::endpoints_for_model`]) reaches the serving lane whose own reported build takes
    /// `tail_tools` (`engine_takes`). That evidence is about the main lane only, so a request
    /// routed to the vision sidecar, a dedicated lane, or any other base gets `false`, as does a
    /// provider outside the fork-gateway family.
    pub(super) fn route_for(
        &self,
        model: &str,
        base: &OpenAiBase,
        snap: &crate::inference::llama_server::ServingSnapshot,
        engine_takes: bool,
    ) -> RequestRoute {
        let route = self.endpoints_for_model(model, base, snap);
        let tail_tools = self.config.tail_tools
            && !self.dedicated_lane
            && engine_takes
            && !snap.base_url.is_empty()
            && route.root() == OpenAiBase::new(&snap.base_url).root();
        RequestRoute { base: route, tail_tools }
    }

    /// Fetch the live model list from the provider's /v1/models endpoint.
    /// Used by adapters that have dynamic catalogs (DMR above all — the list
    /// changes every time the user runs `docker model pull`). Populates
    /// `runtime_models` on success; leaves it unchanged on failure so stale
    /// data is preferred over empty data. Never silently succeeds with an
    /// empty set — returns Err if the endpoint responds with nothing.
    pub(super) async fn refresh_runtime_models(&self) -> Result<usize, String> {
        let url = self.endpoints().models();

        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("GET {} failed: {}", url, e))?;
        if !resp.status().is_success() {
            return Err(format!("GET {} returned {}", url, resp.status()));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse {} body: {}", url, e))?;
        let ids: std::collections::HashSet<String> = body
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("{} returned no `data` array", url))?
            .iter()
            .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
            .collect();
        if ids.is_empty() {
            return Err(format!("{} returned no models", url));
        }
        let count = ids.len();
        *self.runtime_models.write() = Some(ids);
        Ok(count)
    }

    /// Resolve a logical model name to the actual DMR model ID stored in
    /// the runtime catalog. Returns the owned resolved ID on match, or an
    /// Err describing what the caller asked for vs what DMR actually has
    /// — no fallback to the raw name (DMR would just 404 on it).
    ///
    /// On cache miss (either an empty cache or a populated cache that
    /// doesn't contain the needle) this forces a single
    /// `refresh_runtime_models` and retries the lookup once. That covers
    /// the common case: the user ran `docker model pull` after the
    /// adapter initialized, so the forged model exists in DMR but not in
    /// our stale in-memory set.
    pub(super) async fn resolve_dmr_model_name(&self, model_name: &str) -> Result<String, String> {
        if let Some(hit) = self.lookup_runtime_model(model_name) {
            return Ok(hit);
        }
        // Cache miss — refresh once, then retry. If refresh itself fails
        // we surface that error; if the needle still isn't there we
        // hard-error with the full available set so the log makes the
        // mismatch obvious (e.g. persona asked for "-GGUF" but DMR stores
        // "...-gguf:latest").
        self.refresh_runtime_models().await?;
        if let Some(hit) = self.lookup_runtime_model(model_name) {
            return Ok(hit);
        }
        let available: Vec<String> = self
            .runtime_models
            .read()
            .as_ref()
            .map(|ids| ids.iter().cloned().collect())
            .ok_or_else(|| "DMR runtime_models still empty after refresh".to_string())?;
        Err(format!(
            "DMR does not have model '{}'. Available: {:?}. Pull it with: docker model pull <id>",
            model_name, available
        ))
    }

    /// Pure lookup against the cached runtime_models set. Same matching
    /// rules as `runtime_models_contain`: case-insensitive exact or
    /// trivial contains in either direction. No I/O, no refresh — callers
    /// own the refresh decision.
    fn lookup_runtime_model(&self, model_name: &str) -> Option<String> {
        let guard = self.runtime_models.read();
        let ids = guard.as_ref()?;
        let needle = model_name.to_lowercase();
        ids.iter()
            .find(|id| {
                let hay = id.to_lowercase();
                hay == needle || hay.contains(&needle) || needle.contains(&hay)
            })
            .cloned()
    }

    /// Returns true if model_name matches any live runtime model.
    /// Match is exact OR a trivial contains in either direction to
    /// handle the common "persona says short name, DMR stores full
    /// hf.co/…-GGUF ID" pattern. No fuzzy magic beyond that — if neither
    /// contains the other, the adapter honestly does not have the model.
    /// Ensure `id` is present in the runtime catalog. The llama-server gateway
    /// calls this with the ServingSnapshot's `active_model` after initialize —
    /// the SNAPSHOT is the authority on what the lane serves; the `/v1/models`
    /// catalog is DERIVED, and it can lie about identity: on Windows a mangled
    /// spawn `--alias` put the GGUF file PATH in `data[].id`, so the served
    /// model matched nothing and `select()` refused a healthy lane (5090 repro
    /// 2026-07-24). Not a fallback: this records a fact the daemon's reconcile
    /// already verified against the live process.
    pub fn ensure_runtime_model(&self, id: &str) {
        let mut guard = self.runtime_models.write();
        match guard.as_mut() {
            Some(set) => {
                set.insert(id.to_string());
            }
            None => {
                *guard = Some(std::collections::HashSet::from([id.to_string()]));
            }
        }
    }

    pub(super) fn runtime_models_contain(&self, model_name: &str) -> bool {
        let guard = self.runtime_models.read();
        match guard.as_ref() {
            None => false, // not populated — can't lie, return false
            Some(ids) => {
                let needle = model_name.to_lowercase();
                ids.iter().any(|id| {
                    let hay = id.to_lowercase();
                    hay == needle || hay.contains(&needle) || needle.contains(&hay)
                })
            }
        }
    }
}

/// Where one request goes and whether it may carry `tail_tools`, decided together from one
/// snapshot ([`OpenAICompatibleAdapter::route_now`]).
#[derive(Debug, Clone)]
pub(super) struct RequestRoute {
    pub(super) base: OpenAiBase,
    pub(super) tail_tools: bool,
}
