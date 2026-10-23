//! The live model binding a mind deliberates through: the adapter, the requested model and
//! the served window, which must move as one unit. Its own owner, out of the deliberation
//! faculty (12,000+ lines), so binding facts are read in one place.

use std::sync::Arc;

use crate::ai::adapter::AIProviderAdapter;

/// The live model binding for the deliberation faculty: the inference adapter,
/// the model to request, and the served context window — the three facts that
/// MUST move together when the served model re-homes. A `serving/pin` swap (or a
/// grid failover onto another node) means a new adapter (new `base_url` /
/// `default_model`), a new requested model (the single-resident guard checks it),
/// AND a new context window (the new GGUF's `n_ctx`) — as ONE atomic unit. A torn
/// read (new model + stale window, or new adapter + stale model) is exactly the
/// silent wrong-brain that [[fallbacks-are-illegal-fail-loud]] forbids, so all
/// three ride ONE `ArcSwap`, loaded once per turn.
pub struct ModelBinding {
    /// The inference adapter, leased per call. `Arc` so the whole cycle shares the
    /// one served-model backend (one base model, N persona lanes — the
    /// INFERENCE-LANES-REALISTIC shape).
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// Which model to ask for (`None` → the adapter's own default). The gateway's
    /// single-resident guard checks this against the resident model and refuses a
    /// mismatch LOUD, so it must move in lockstep with `adapter`.
    pub model: Option<String>,
    /// The effective served context window in tokens (task #50). The deliberation
    /// prompt + completion reserve MUST fit this or the gateway 500s; it is sized
    /// from the SAME binding so a re-home to a smaller window can never leave the
    /// prompt sized for the old one.
    pub context_window: u32,
}

impl ModelBinding {
    /// The model this binding's requests are actually served by: the bound one, or the
    /// adapter's own default when none is bound. This is the one resolution request
    /// construction uses, so measurements keyed by model (the decode rate) read the same
    /// key the request carries. A spawn binding starts with `model: None`, and the receipt
    /// said "no decode rate" for a lane with a fresh curve (card b406e901).
    pub fn requested_model(&self) -> &str {
        self.model.as_deref().unwrap_or_else(|| self.adapter.default_model()) // unwrap_or_else: an omitted model selects the adapter default, the same one request construction sends
    }
}
