//! MoE serving context — builds a sized, gate-seeded [`ServingExpertPager`] from a model's
//! GGUF, the bridge the serving daemon uses to drive K3 expert placement.
//!
//! It composes three already-tested reads over the GGUF (no forward pass, no serving):
//! [`locate_layer_sets`] (MoE detection + per-layer expert geometry),
//! [`locate_gate_magnitudes`] (the router's baked per-expert preference — the cold-start
//! seed), and [`gguf_keys::block_count`] (the total transformer-block count the `-ot`
//! placement iterates). Dense models (no `*_exps` layers) return `None` — the daemon then
//! serves them with no expert override, exactly as before.
//!
//! Seeding matters: with the gate magnitudes loaded, the pager's FIRST layer-placement pass
//! already pins the model's inherently-preferred expert layers to GPU, before a single live
//! hit — then [`ServingExpertPager::tick_layer_placement`] refines it as real activations
//! arrive. So K3 gets a sensible static residency immediately and an adaptive one over time.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use candle_core::quantized::gguf_file::Content;

use super::expert_residency::ExpertId;
use super::serving_pager::ServingExpertPager;
use crate::genome::expert_layout::locate_layer_sets;
use crate::genome::gate_magnitude::locate_gate_magnitudes;
use crate::inference_capability::gguf_keys;

/// A served MoE model's pager plus the layout dimensions
/// [`tick_layer_placement`](ServingExpertPager::tick_layer_placement) needs — carried
/// together so the daemon resolves them once per model, not per tick.
pub struct MoeServingContext {
    /// The residency driver for this model (owns the observer + decay + gate seed).
    pub pager: ServingExpertPager,
    /// Experts per MoE layer (uniform K3-class geometry) — the `-ot` layer-byte unit.
    pub n_experts_per_layer: u32,
    /// TOTAL transformer block count — the `-ot` iteration ceiling the launcher needs to
    /// compute the cold (CPU) complement of the hot layers.
    pub n_layers: u32,
}

/// Build a [`MoeServingContext`] from a model's GGUF, or `None` if the model is dense (no MoE
/// expert layers) or the GGUF can't be read. Pure over the GGUF. `margin_bytes` is the VRAM
/// headroom the pager holds below the serving budget; `relaunch_threshold` the hot-layer churn
/// that earns a (process-respawning) relaunch.
pub fn moe_serving_context(
    gguf_path: &Path,
    gguf_id: &str,
    margin_bytes: u64,
    relaunch_threshold: usize,
) -> Option<MoeServingContext> {
    let mut file = File::open(gguf_path).ok()?;
    let ct = Content::read(&mut file).ok()?;
    let arch = gguf_keys::architecture(&ct)?;

    // MoE detection + geometry. Empty / NotMoe → dense → no expert placement.
    let sets = match locate_layer_sets(&ct, &arch) {
        Ok(sets) if !sets.is_empty() => sets,
        _ => return None,
    };
    let n_experts_per_layer = sets[0].n_experts;
    if n_experts_per_layer == 0 {
        return None;
    }
    // Per-expert bytes: the layer's stacked expert blob divided by its expert count. K3-class
    // geometry is uniform across MoE layers, so the first set sizes them all.
    let expert_bytes = sets[0].total_bytes() / n_experts_per_layer as u64;
    let n_layers = gguf_keys::block_count(&ct, &arch)?;

    // Cold-start seed: the router's baked per-expert preference. Empty is fine (rides live
    // hits alone); a read error is non-fatal (same — never block serving on the seed).
    let gate: HashMap<ExpertId, f32> = locate_gate_magnitudes(&ct, &mut file, &arch)
        .unwrap_or_default()
        .into_iter()
        .map(|((layer, expert), m)| (ExpertId { layer, expert }, m))
        .collect();

    let pager = ServingExpertPager::new(gguf_id, expert_bytes, margin_bytes, relaunch_threshold, gate);
    Some(MoeServingContext {
        pager,
        n_experts_per_layer,
        n_layers,
    })
}
