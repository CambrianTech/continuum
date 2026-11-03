//! The engine's ONE shared KV pool on a budget (2026-11-03, Joel: "Slot counts should never
//! require shutdown"). Under `--kv-unified` every sequence draws on one pool, so the pool is
//! what the budget holds beside the weights and one compute buffer, not windows laid side by
//! side, and not capped at the trained window: each mind's share inside it is bounded where
//! it is published.

use super::ModelFootprint;

impl ModelFootprint {
    /// Tokens of shared KV `usable_bytes` holds for this model: the one-lane fit without the
    /// trained-window cap. 0 when the model has no measured KV cost, so nothing can be fitted.
    pub fn pool_within(&self, usable_bytes: u64) -> u32 {
        if self.kv_per_token == 0 {
            return 0;
        }
        ModelFootprint {
            context_window: u32::MAX,
            ..self.clone()
        }
        .window_within(usable_bytes, 1)
    }
}
