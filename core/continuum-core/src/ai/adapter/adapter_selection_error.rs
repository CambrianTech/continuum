use super::*;

impl std::fmt::Display for AdapterSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEligibleProductionAdapter {
                requested_model,
                requested_device,
                preferred_provider,
                registered_providers,
                non_production_adapters_present,
            } => {
                write!(f, "no production-capable adapter found for ")?;
                if let Some(p) = preferred_provider {
                    write!(f, "preferred_provider='{}' ", p)?;
                }
                if let Some(m) = requested_model {
                    write!(f, "model='{}' ", m)?;
                }
                write!(f, "device={:?}. ", requested_device)?;
                if registered_providers.is_empty() {
                    write!(f, "No adapters are registered. ")?;
                } else {
                    write!(
                        f,
                        "Registered production adapters: {:?}. ",
                        registered_providers
                    )?;
                }
                if *non_production_adapters_present {
                    write!(
                        f,
                        "A non-production adapter (heuristic / canned) IS registered \
                         but the substrate refuses to substitute it for a real model \
                         (per no-fallbacks doctrine). "
                    )?;
                }
                write!(
                    f,
                    "Remediation: install/configure a real-model adapter that supports \
                     this model+device, or route this request through `select()` if \
                     it's a CI/debug context that legitimately wants a non-production \
                     adapter."
                )?;
                Ok(())
            }
        }
    }
}

impl std::error::Error for AdapterSelectionError {}

/// Registry of AI provider adapters.
///
/// Stores `Arc<dyn AIProviderAdapter>` so the substrate's shared
/// adapter ownership (supervisor + service_loop + cognition layer
/// + future shared-base + LoRA paging #122 all see the same
/// instance) maps cleanly into the registry.
///
/// **The registry is storage + lookup. It is NOT lifecycle.** The
/// caller initializes adapters BEFORE registering them
/// (init-then-register pattern). The adapter is "ready" the moment
/// it reaches the registry. Shutdown happens when the Arc's last
/// holder drops it; no registry-side `shutdown_all` is needed.
/// This is the elegant intentional architecture Joel called for
/// 2026-06-03 — no Box→Arc shim hacks, no &mut self lifecycle
/// methods accessed through shared handles.
pub struct AdapterRegistry {
    pub(super) adapters: std::collections::HashMap<String, Arc<dyn AIProviderAdapter>>,
    pub(super) priority_order: Vec<String>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: std::collections::HashMap::new(),
            priority_order: Vec::new(),
        }
    }

    /// Register an already-initialized adapter with a priority (lower
    /// = higher priority). The caller is responsible for calling
    /// `adapter.initialize()` BEFORE wrapping in `Arc::new` and
    /// passing here; the registry trusts that registered adapters
    /// are ready to serve.
    pub fn register(&mut self, adapter: Arc<dyn AIProviderAdapter>, priority: usize) {
        let id = self.registration_key(adapter.provider_id());

        // Insert into priority order
        if priority >= self.priority_order.len() {
            self.priority_order.push(id.clone());
        } else {
            self.priority_order.insert(priority, id.clone());
        }

        self.adapters.insert(id, adapter);
    }

    pub(super) fn registration_key(&self, provider_id: &str) -> String {
        if !self.adapters.contains_key(provider_id) {
            return provider_id.to_string();
        }
        let mut i = 2;
        loop {
            let candidate = format!("{provider_id}#{i}");
            if !self.adapters.contains_key(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    /// Drop an adapter from the registry. Mirror of `register`. The
    /// hot-swap lever for adapters whose health is dynamic (e.g. DMR
    /// when Docker Desktop crashes — see `DmrWatchdog`). Returns true
    /// if the adapter was registered, false if it wasn't present.
    /// Removes from both the adapters map AND the priority_order vec
    /// so a subsequent `available()` / `select()` reflects reality.
    /// Caller is responsible for invoking `adapter.shutdown()` first
    /// if there's per-adapter cleanup to do; this method drops the
    /// boxed adapter (Drop impl runs).
    pub fn deregister(&mut self, provider_id: &str) -> bool {
        let keys: Vec<String> = self
            .adapters
            .iter()
            .filter_map(|(key, adapter)| {
                if key == provider_id || adapter.provider_id() == provider_id {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();
        let removed = !keys.is_empty();
        if removed {
            for key in &keys {
                self.adapters.remove(key);
            }
            self.priority_order.retain(|id| !keys.contains(id));
        }
        removed
    }

    /// True if the given provider_id is currently registered. Cheap
    /// HashMap lookup. Used by health-watchdogs to decide whether they
    /// need to register or deregister on a probe state change.
    pub fn is_registered(&self, provider_id: &str) -> bool {
        self.adapters
            .iter()
            .any(|(key, adapter)| key == provider_id || adapter.provider_id() == provider_id)
    }

    /// Get adapter by provider ID.
    pub fn get(&self, provider_id: &str) -> Option<&dyn AIProviderAdapter> {
        self.get_shared(provider_id).map(|adapter| adapter.as_ref())
    }

    /// Lease an adapter past the registry lock's scope.
    pub fn get_arc(&self, provider_id: &str) -> Option<Arc<dyn AIProviderAdapter>> {
        self.get_shared(provider_id).cloned()
    }

    pub(super) fn get_shared(&self, provider_id: &str) -> Option<&Arc<dyn AIProviderAdapter>> {
        self.adapters.get(provider_id).or_else(|| {
            self.priority_order.iter().find_map(|key| {
                self.adapters.get(key).filter(|adapter| adapter.provider_id() == provider_id)
            })
        })
    }

    /// Get available adapters (those that initialized successfully)
    pub fn available(&self) -> Vec<&str> {
        self.priority_order
            .iter()
            .filter_map(|id| self.adapters.get(id).map(|_| id.as_str()))
            .collect()
    }

    /// What each available adapter actually serves: `(provider_id, model_id)`
    /// pairs in priority order, deduplicated, from each adapter's
    /// [`AIProviderAdapter::served_model_ids`] (live runtime set where the
    /// adapter has one, declared default otherwise). This is the list a refusal
    /// must show a caller who named a model the registry can't route —
    /// `available()` is provider ids, which a grid consumer with no local
    /// registry cannot turn into a valid `model` (card a466fdd4: IntelMac → 5090
    /// was refused with a list it couldn't act on).
    pub fn served_models(&self) -> Vec<(&str, String)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for id in &self.priority_order {
            let Some(adapter) = self.adapters.get(id) else {
                continue;
            };
            for model in adapter.served_model_ids() {
                if seen.insert((id.as_str(), model.clone())) {
                    out.push((id.as_str(), model));
                }
            }
        }
        out
    }

    /// Select best adapter based on request.
    ///
    /// Per [[no-fallbacks-ever]] (Joel, 2026-06-01: "No fallbacks ever
    /// it's forbidden."): if the caller specifies neither `model` nor
    /// `preferred_provider`, this is auto-discovery without any specifier
    /// — the textbook leak path that lets fake adapters silently serve
    /// production traffic. We refuse it and return `None` with a warning.
    /// Callers MUST specify at least one of: which provider, or which
    /// model. The substrate's role is to honor that intent precisely,
    /// not to guess.
    ///
    /// Device-aware routing (like PyTorch device='cuda' / Android MediaCodec):
    /// - `device = Gpu`: only GPU-capable adapters (DMR, llama-metal, llama-vulkan).
    ///   Hard error if no GPU adapter supports the model. DEFAULT.
    /// - `device = Cpu`: only CPU-capable adapters (Candle). Explicit opt-in for
    ///   training/LoRA. Never auto-selected for chat.
    /// - `device = Auto`: try GPU first, CPU fallback WITH warning. RESERVED —
    ///   not implemented yet, behaves as Gpu until we trust the CPU path.
    ///
    /// Explicit `preferred_provider` always wins regardless of device.
    /// Cloud providers (Anthropic, OpenAI, etc.) are always eligible — they're
    /// not local compute, so device preference doesn't apply.
    pub fn select<'a>(
        &'a self,
        preferred_provider: Option<&str>,
        model: Option<&str>,
        device: InferenceDevice,
    ) -> Option<(&'a str, &'a dyn AIProviderAdapter)> {
        self.select_shared(preferred_provider, model, device)
            .map(|(provider, adapter)| (provider, adapter.as_ref()))
    }

    /// Lease the exact selected adapter so inference does not retain a registry lock.
    /// Provider IDs may be shared by adapters serving different models: clone the
    /// selected Arc rather than looking it up again by provider ID.
    pub fn select_arc(
        &self,
        preferred_provider: Option<&str>,
        model: Option<&str>,
        device: InferenceDevice,
    ) -> Option<(String, Arc<dyn AIProviderAdapter>)> {
        self.select_shared(preferred_provider, model, device)
            .map(|(provider, adapter)| (provider.to_owned(), Arc::clone(adapter)))
    }

    pub(super) fn select_shared<'a>(
        &'a self,
        preferred_provider: Option<&str>,
        model: Option<&str>,
        device: InferenceDevice,
    ) -> Option<(&'a str, &'a Arc<dyn AIProviderAdapter>)> {
        // 0. No-specifier guard. Auto-discovery without ANY specifier is
        // the silent-substitution path forbidden by [[no-fallbacks-ever]].
        // Caller must say what they want.
        if preferred_provider.is_none() && model.is_none() {
            clog_warn!(
                "AdapterRegistry::select called with no preferred_provider AND no model. \
                 Auto-discovery without a specifier is forbidden per the no-fallbacks doctrine \
                 — caller MUST specify which provider or which model they want. \
                 Registered: {:?}.",
                self.available()
            );
            return None;
        }

        // 1. Explicit provider — bypass routing for NAMED adapters.
        //    Special case: "local" means "best available local GPU adapter"
        //    — NOT a specific adapter name. Drops through to device-filtered
        //    auto-selection (tier 3) with the requested model. This is how
        //    local personas get DMR when available, Vulkan when not, and
        //    hard-error when neither can serve the model.
        if let Some(pref) = preferred_provider {
            if pref != "local" {
                for key in &self.priority_order {
                    if let Some(adapter) = self.adapters.get(key) {
                        if key == pref || adapter.provider_id() == pref {
                            if model.map_or(true, |m| adapter.supports_model(m)) {
                                return Some((adapter.provider_id(), adapter));
                            }
                        }
                    }
                }
                clog_warn!(
                    "Provider '{}' explicitly requested but not available. Available: {:?}",
                    pref,
                    self.available()
                );
                return None;
            }
            // "local" — fall through to device-filtered auto-selection below
        }

        // 2. Registry-driven cloud routing (#70). A model's provider is a
        // registry FACT — look it up instead of re-guessing it from the name
        // prefix. Only CLOUD providers short-circuit here: they're always
        // eligible (no local device cost), whereas local models must fall
        // through to tier-3 device-filtered selection. An UNREGISTERED model
        // name resolves to nothing and falls through too — we never route by
        // guessing a provider from an unmodeled name (that was the smell).
        // `try_global()` (not `global()`): consult the registry IF it's up.
        // This is not a fallback — tier 2 asks "is this a registered CLOUD
        // model?", and "registry not yet initialized" is materially the same
        // answer as "not a registered cloud model": proceed to tier 3. In
        // production the registry is always booted in backend_init before any
        // adapter selects, so the None branch here is exclusively the bare
        // unit-test case; panicking via global() would couple selection to
        // global boot for no gain.
        if let Some(model_name) = model {
            if let Some(reg) = crate::model_registry::try_global() {
                if let Some(spec) = reg.model(model_name) {
                    let is_cloud =
                        reg.provider(&spec.provider).map(|p| p.kind) == Some(ProviderKind::Cloud);
                    if is_cloud {
                        if let Some(adapter) = self.get_shared(&spec.provider) {
                            return Some((spec.provider.as_str(), adapter));
                        }
                    }
                }
            }
        }

        // 3. Device-filtered local adapter selection.
        // Walk priority order; only consider adapters whose device_type
        // matches the request. GPU adapter that honestly supports the model
        // wins. No silent cross-device fallback.
        let device_matches = |adapter_device: InferenceDevice| -> bool {
            match device {
                InferenceDevice::Gpu => adapter_device == InferenceDevice::Gpu,
                InferenceDevice::Cpu => adapter_device == InferenceDevice::Cpu,
                InferenceDevice::Auto => true, // future: GPU-first then CPU
            }
        };

        for id in &self.priority_order {
            if let Some(adapter) = self.adapters.get(id) {
                if !device_matches(adapter.device_type()) {
                    continue; // wrong device class — skip, don't fallback
                }
                // If model specified, adapter must honestly support it.
                // If no model specified, any adapter on the right device works.
                if model.map_or(true, |m| adapter.supports_model(m)) {
                    return Some((adapter.provider_id(), adapter));
                }
            }
        }

        // No adapter matched. Fail loud.
        if let Some(model_name) = model {
            clog_warn!(
                "No {:?}-device adapter supports model '{}'. Registered: {:?}. Pull model into DMR: `docker model pull {}`, or install the right GPU backend.",
                device,
                model_name,
                self.available(),
                model_name
            );
        } else {
            clog_warn!(
                "No {:?}-device adapter available. Registered: {:?}.",
                device,
                self.available()
            );
        }
        None
    }

    // Note: `initialize_all` and `shutdown_all` were removed in task
    // #162 alongside the Box→Arc registry migration. The registry's
    // job is storage + lookup, NOT lifecycle. Callers initialize
    // adapters before registering (init-then-register pattern) and
    // adapter cleanup happens when the Arc's last holder drops the
    // adapter. Per [[init-once-handle-then-lease-zero-copy-refs]]:
    // init at boot, lease per turn, drop at end-of-life.
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
