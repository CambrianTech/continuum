//! LoRA catalog probe and name-to-server-index resolution.

use super::*;

impl OpenAICompatibleAdapter {
    pub async fn probe_lora_catalog(&self) -> Result<(), String> {
        let url = self.endpoints().lora_adapters();
        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("GET {} failed: {}", url, e))?;

        // 404/501 → the backend genuinely has no runtime-LoRA surface. Cache it.
        if resp.status() == reqwest::StatusCode::NOT_FOUND
            || resp.status() == reqwest::StatusCode::NOT_IMPLEMENTED
        {
            *self.lora_support.write().unwrap() = LoraSupport::Unsupported;
            return Ok(());
        }
        if !resp.status().is_success() {
            return Err(format!("GET {} returned {}", url, resp.status()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse {} body: {}", url, e))?;
        // Accept a top-level array (llama-server) or `{ "data": [...] }`.
        let arr = body
            .as_array()
            .or_else(|| body.get("data").and_then(|v| v.as_array()))
            .ok_or_else(|| format!("{} returned non-array LoRA catalog", url))?;
        let catalog: Vec<(i64, String)> = arr
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                // The server's own `id` is authoritative; fall back to array
                // position only if it omits one (older builds).
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(idx as i64);
                let path = entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                (id, path)
            })
            .collect();
        *self.lora_support.write().unwrap() = LoraSupport::Supported(catalog);
        Ok(())
    }

    /// Pure matcher: given a discovered catalog of `(server-id, path)`, find the
    /// server load-index for a requested adapter. Exact path match wins; else a
    /// trivial substring match on path or name (same forgiving rule as
    /// `lookup_runtime_model`, for the "short name vs full path" case). No I/O.
    pub(super) fn match_lora_index(catalog: &[(i64, String)], name: &str, path: &str) -> Option<i64> {
        let want_path = path.to_lowercase();
        let want_name = name.to_lowercase();
        // 1. exact path
        if !want_path.is_empty() {
            if let Some((id, _)) = catalog.iter().find(|(_, p)| p.to_lowercase() == want_path) {
                return Some(*id);
            }
        }
        // 2. a gene staged into the engine's --train-dir for a runtime load (charter S1):
        //    the catalog carries the staged file, named from the genome path
        if !path.is_empty() {
            if let Some(stage) = crate::inference::llama_server::gene_stage_name(std::path::Path::new(path)) {
                if let Some((id, _)) = catalog.iter().find(|(_, p)| {
                    std::path::Path::new(p).file_name().and_then(|n| n.to_str()) == Some(stage.as_str())
                }) {
                    return Some(*id);
                }
            }
        }
        // 3. substring on path or name
        catalog
            .iter()
            .find(|(_, p)| {
                let hay = p.to_lowercase();
                (!want_path.is_empty() && hay.contains(&want_path))
                    || (!want_name.is_empty() && hay.contains(&want_name))
            })
            .map(|(id, _)| *id)
    }

    /// Project the genome onto the wire as the complete llama.cpp `lora`
    /// request-body field: an `{ "id": N, "scale": S }` entry for EVERY loaded
    /// adapter — the requested scale when the genome names it, else `0.0`.
    ///
    /// Why an entry per LOADED adapter and not just per REQUESTED one: llama.cpp
    /// applies every loaded adapter at scale 1.0 for any request that OMITS the
    /// `lora` field. So once the custodian has loaded an adapter, an empty genome
    /// can NOT be expressed by omission — omitting silently serves the adapter at
    /// full strength. That was the no-op behind the LIFT=0 A/B: the base arm sent
    /// no `lora` field and unknowingly ran WITH the gene, so base==gene, lift 0.
    /// The genome handle is the single source of truth for what's active; emitting
    /// explicit `0.0` for loaded-but-unrequested adapters makes "empty genome ==
    /// base" true at the wire.
    ///
    /// Returns `None` only when nothing is loaded (omitting the field is then
    /// correct, and a non-LoRA backend with an empty request never probes — no
    /// per-request penalty). Capability is DISCOVERED: a non-empty request probes
    /// `/lora-adapters` and FAILS LOUD if the backend can't page LoRA
    /// (`Unsupported`) or the adapter isn't loaded (re-probing once first).
    pub(super) async fn lora_scale_vector(
        &self,
        reqs: &[ActiveAdapterRequest],
    ) -> Result<Option<Vec<Value>>, String> {
        // Resolve each requested adapter to its loaded `(id, scale)` (probe +
        // fail loud on miss). Explicit requested scales override the 0.0 neutral.
        let mut requested: Vec<(i64, f64)> = Vec::with_capacity(reqs.len());
        if !reqs.is_empty() {
            if matches!(&*self.lora_support.read().unwrap(), LoraSupport::Unknown) {
                self.probe_lora_catalog().await?;
            }
            for req in reqs {
                let id = match self.lookup_lora_index(&req.name, &req.path) {
                    Some(id) => id,
                    None => {
                        // Miss — re-probe once (it may have just been registered),
                        // then resolve or fail loud with what IS loaded.
                        self.probe_lora_catalog().await?;
                        self.lookup_lora_index(&req.name, &req.path)
                            .ok_or_else(|| self.lora_miss_error(&req.name, &req.path))?
                    }
                };
                requested.push((id, req.scale));
            }
        }

        // Neutralize every loaded-but-unrequested adapter. Unknown/Unsupported with
        // an EMPTY request → nothing known to neutralize → omit the field (the eval
        // probes at lane spawn so its base arm already has the catalog here).
        let loaded_ids: Vec<i64> = match &*self.lora_support.read().unwrap() {
            LoraSupport::Supported(catalog) => catalog.iter().map(|(id, _)| *id).collect(),
            _ => Vec::new(),
        };
        if loaded_ids.is_empty() {
            return Ok(None);
        }
        let entries = loaded_ids
            .into_iter()
            .map(|id| {
                let scale = requested
                    .iter()
                    .find(|(rid, _)| *rid == id)
                    .map(|(_, s)| *s)
                    .unwrap_or(0.0);
                json!({ "id": id, "scale": scale })
            })
            .collect();
        Ok(Some(entries))
    }

    /// Local lookup against the discovered catalog. `None` when unsupported,
    /// unprobed, or genuinely absent — callers own the probe/fail decision.
    fn lookup_lora_index(&self, name: &str, path: &str) -> Option<i64> {
        match &*self.lora_support.read().unwrap() {
            LoraSupport::Supported(catalog) => Self::match_lora_index(catalog, name, path),
            _ => None,
        }
    }

    /// The fail-loud message for an unresolved page-in, naming the cause so the
    /// log makes the boundary obvious (backend can't, vs custodian hasn't).
    fn lora_miss_error(&self, name: &str, path: &str) -> String {
        match &*self.lora_support.read().unwrap() {
            LoraSupport::Unsupported => format!(
                "LoRA page-in requested (adapter '{}') but provider '{}' exposes no \
                 /lora-adapters — its serving backend can't apply a LoRA per-request \
                 (cloud API, or mlx_lm.server whose --adapter-path no-ops). Serve a \
                 FUSED model or route to a llama.cpp/llama-server backend.",
                name, self.config.provider_id
            ),
            LoraSupport::Supported(catalog) => {
                let loaded: Vec<&str> = catalog.iter().map(|(_, p)| p.as_str()).collect();
                format!(
                    "LoRA adapter '{}' (path '{}') is not loaded on '{}'. Loaded: {:?}. \
                     The custodian must register it with the serving backend first.",
                    name, path, self.config.provider_id, loaded
                )
            }
            LoraSupport::Unknown => format!(
                "LoRA adapter '{}' could not be resolved on '{}' (catalog unprobed)",
                name, self.config.provider_id
            ),
        }
    }

}
