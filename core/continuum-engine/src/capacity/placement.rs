//! `PlacementRequest` — the SLICE-1 buft-override seam between the K3 pager (which owns the
//! VRAM fit) and the llama-server launcher (which owns the `-ot` args).
//!
//! Why layer-granular: llama.cpp's `-ot`/`--override-tensor` places WHOLE stacked tensors,
//! and a MoE layer's experts are ONE tensor (`blk.N.ffn_*_exps` = `{n_embd, n_ff, n_expert}`,
//! all of layer N's experts in a single blob). So a load-time relaunch pins whole LAYERS to
//! VRAM, not individual experts. The pager decides which layers fit
//! ([`plan_layer_residency`](super::expert_residency::plan_layer_residency)); the launcher
//! turns the complement into one `-ot` regex (`blk\.(c1|c2|…)\.ffn_.*_exps=CPU`).
//!
//! Per-EXPERT placement is the slice-2 granularity (the vendored-llama upload fork); it rides
//! a different path and is deliberately NOT in this struct.

/// One serving lane's expert-layer placement for a (re)launch. Produced by the pager,
/// consumed by `llama_server.rs` when it builds the llama-server command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementRequest {
    /// The model these layers belong to — matches the served lane's GGUF.
    pub gguf_id: String,
    /// TOTAL transformer block count (the `-ot` iteration ceiling). The launcher computes the
    /// cold set as `(0..n_layers)` MINUS `hot_layers`.
    pub n_layers: u32,
    /// The real `blk.N` indices whose full expert blob should be GPU-resident, ascending.
    /// Every other block's expert tensor is `-ot`'d to CPU (RAM-faulted per token). Sized by
    /// the pager to fit the serving VRAM budget, so the launcher never overflows.
    pub hot_layers: Vec<u32>,
}

/// The result of one layer-placement pass: the request to (re)launch with, and whether the
/// hot-layer set changed enough vs what's currently served to justify the (expensive) process
/// respawn. The serving loop relaunches only when `needs_relaunch`, then calls
/// [`mark_layer_relaunched`](super::serving_pager::ServingExpertPager::mark_layer_relaunched).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPlacementOutcome {
    pub request: PlacementRequest,
    pub needs_relaunch: bool,
}
