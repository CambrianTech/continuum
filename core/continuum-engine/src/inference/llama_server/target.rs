//! Target responsibility of the llama-server process owner.
use super::*;
/// Everything the launcher needs to bring a model up, GROUPED so adding a new
/// serving knob is a field here — never a new param threaded down the call chain
/// ([[pass-the-model-struct-no-param-hell]]). The [`Model`] carries its own id,
/// gguf path, chat template, and trained `context_window` (resolved ONCE at
/// registry load — never re-fetched downstream). `context_window` here is the
/// HOST-FIT served window the serving daemon derived for THIS host from
/// `(model.context_window ∩ budget/lanes/kv)` — a computed property of the
/// serving plan, NOT a hardcoded constant cap. It is the single source of truth
/// for the effective serving window (task #50): it becomes llama-server's `-c`,
/// the persona's `context_length`, and the deliberation prompt budget.
#[derive(Debug, Clone)]
pub struct ServingTarget {
    /// The chosen base model — the grouped model info, resolved once.
    pub model: Model,
    /// The host-RAM prompt cache (`--cache-ram`, MiB) DERIVED for this serve —
    /// per-citizen measured demand × the model's own kv/token, clamped by what
    /// the host affords after the resident working set (restore-economy 1.b).
    /// Computed ONCE at target construction; the governor's host-cache lease
    /// reads the SAME number, so the expert cache can never be sized as if
    /// this RAM were free. Single-purpose lanes (eval, vision sidecar) pass
    /// the declared cold-start prior.
    pub host_prompt_cache_mib: u32,
    /// The host-fit served window the planner computed for this host — the
    /// PER-LANE window. Sized to fit the working set (tool schemas + framing +
    /// room burst + recalled memory + completion reserve) within the budget,
    /// capped only by the model's own trained ceiling. The deliberation faculty
    /// keeps its prompt inside this so llama-server never 500s ("Context size
    /// has been exceeded").
    ///
    /// CRUCIAL: this is per-lane, NOT the `-c` total. llama-server's `-c` is the
    /// WHOLE KV cache, split evenly across `--parallel` slots — each request
    /// only gets `-c / n_parallel` tokens. So to hand each of `lanes` slots a
    /// full `context_window`, the spawn launches `-c (context_window * lanes)
    /// --parallel lanes`. Launching `-c context_window` without `--parallel`
    /// (the prior bug) let llama.cpp default to 4 slots and silently quartered
    /// the per-request window, 500-ing every deliberation whose prompt fit the
    /// planned window but overflowed the unplanned per-slot share.
    pub context_window: u32,
    /// Continuous-batching lanes (`n_seq_max`) from the plan (≥ 1). Drives
    /// llama-server's `--parallel`. The plan budgeted `kv_at(context_window) *
    /// lanes` of KV against the host budget, so `-c (context_window * lanes)`
    /// is memory-safe by the planner's own arithmetic.
    pub lanes: u32,
    /// The trained LoRA genome layers to load into the serving catalog at spawn
    /// (`llama-server --lora <path>` per entry). This is the SET — which genes
    /// are *loadable*; the per-request `"lora":[{id,scale}]` body field decides
    /// which page IN for a given turn. llama.cpp has no hot-load API, so a change
    /// to this set is a relaunch (rare — genes are produced post-training);
    /// page-in/out within a loaded set never relaunches. Empty = base model only
    /// (the legitimate no-genes-trained state, NOT a fallback).
    pub adapters: Vec<AdapterEntry>,
    /// Where this lane's weights run. The live persona lane runs on the GPU
    /// (`Gpu` — every offloadable layer) for throughput; a throwaway measurement
    /// lane that must COEXIST with the living GPU lane runs on the CPU (`Cpu` —
    /// `--n-gpu-layers 0`) so it never contends for the VRAM the living persona
    /// already holds. On a single GPU two resident models OOM the Metal command
    /// buffer at decode time (`kIOGPUCommandBufferCallbackErrorOutOfMemory`),
    /// which surfaced as a silent all-empty eval; pinning the eval lane to CPU
    /// honors humane-eval (#59 — never degrade the living lane) AND the
    /// single-GPU budget (#56) by using the free CPU RAM (the misfit-toy
    /// resource). This is an explicit placement the CALLER chooses, NOT a silent
    /// fallback — when the `ResourceGovernor` (#56) lands it owns this decision
    /// (tier the live lane down, route to a grid peer with free GPU, or pin CPU).
    pub placement: LanePlacement,
    /// K3 slice-1 expert placement: the layer-granular residency the pager computed
    /// (`PlacementRequest.hot_layers` = the real `blk.N` whose stacked expert block fits
    /// GPU). `None` = no expert paging (the default — the whole model places normally).
    /// When `Some`, the COLD layers' `ffn_*_exps` tensors are offloaded to CPU via `-ot` at
    /// spawn (see [`ServingTarget::expert_ot_value`]); the hot layers stay GPU-resident.
    /// llama.cpp places tensors at LOAD time only, so a change to the hot set is honored by
    /// a relaunch — that relaunch is DECIDED by the pager (`serving_pager`'s
    /// `relaunch_needed`), not by the probe-based reconcile here, since there is no API to
    /// read a running server's `-ot`. Per-expert paging (no relaunch) is slice-2.
    pub expert_placement: Option<crate::capacity::placement::PlacementRequest>,
    /// Device-fit resident-override: when the model's RESIDENT (non-expert) tier
    /// overflows the governed VRAM budget as-shipped, the governor's
    /// [`device_fit`](crate::capacity::device_fit) plan resolves a precision-shrunk
    /// resident-override GGUF that fits. The launcher exports it as
    /// `LLAMA_RESIDENT_OVERRIDE` so llama.cpp sources the resident tensors from it
    /// (all offloaded to GPU) while the primary GGUF streams experts. `None` =
    /// resident fits as-shipped (Native), served with no override — the default and
    /// the only shape for a dense or small-MoE model. [[device-fit-repeatable-primitive]] / #29.
    pub resident_override: Option<std::path::PathBuf>,
    /// This lane IS the vision sidecar (its whole purpose is the projector).
    /// The `mmproj_on_main_lane` withhold — written so multimodal never disables
    /// `--cache-reuse` on the MAIN persona lane — must not strip the sidecar's own
    /// eyes: measured 2026-08-26, every sidecar spawn flowed through the same
    /// withhold, launched sightless, honestly failed its /props sight check, and
    /// churned. The guard was scoped too broadly; the lane's ROLE decides.
    pub vision_sidecar: bool,
}


/// Where a serving lane's model weights are resident — see [`ServingTarget::placement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanePlacement {
    /// Offload every layer llama-server can to the GPU — its default, so we omit
    /// the flag. The throughput placement for the living persona lane.
    #[default]
    Gpu,
    /// Pin all layers to CPU RAM (`--n-gpu-layers 0`). Zero VRAM contention — for
    /// a measurement lane coexisting with a GPU-resident living lane.
    Cpu,
}

impl LanePlacement {
    /// The pool this placement's bytes live in, which is the governed row that admits them:
    /// a CPU-placed lane (`--n-gpu-layers 0`) keeps weights AND KV in system RAM, so it is
    /// sized against `Ram`, never against a card holding none of it (card 7c72c0f0: nine
    /// citizens planned against a 5.7 GB Vram row on a box with 34.4 GB of RAM in use).
    pub fn pool_kind(self) -> crate::resources::ResourceKind {
        match self {
            Self::Cpu => crate::resources::ResourceKind::Ram,
            Self::Gpu => crate::resources::ResourceKind::Vram,
        }
    }
}


/// Where the host's serving plan places MAIN lanes. `CONTINUUM_SERVING_PLACEMENT=cpu` in
/// config.env (the layer the installer writes — #3729's Intel-Mac arm builds the server
/// with Metal OFF and records the decision here) pins every main lane to the CPU; anything
/// else is the GPU. A decision, read from config, never a raw env var (concurrency guide).
pub fn main_lane_placement() -> LanePlacement {
    placement_from_config(continuum_base::config_env::read("CONTINUUM_SERVING_PLACEMENT").as_deref())
}

/// The pool the main lane's bytes live in: the ONE decision every serving site keyed by a
/// resource kind reads through (the plan's ceiling, the replace-myself credit, the
/// credited-once check, the consumer's own attribution). #4169 made the physical bound follow
/// the placement, and this makes the PLAN follow it. It lives beside the placement it maps, so
/// the training trigger reads it without reaching into the core's serving daemon (card 2a69e6ba).
pub fn serving_pool_kind() -> crate::resources::ResourceKind {
    main_lane_placement().pool_kind()
}


pub fn placement_from_config(value: Option<&str>) -> LanePlacement {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "cpu" => LanePlacement::Cpu,
        _ => LanePlacement::Gpu,
    }
}


/// One LoRA genome layer to load into the serving catalog at spawn. The `path`
/// is llama.cpp's load identity (what `--lora` takes and what the per-request
/// resolver matches against); the `alias` is ours, for logs and the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterEntry {
    /// Human label for logs/observability (the gene's name).
    pub alias: String,
    /// Absolute path to the GGUF-lora the server loads via `--lora`.
    pub path: PathBuf,
}


impl ServingTarget {
    /// The model id the running server should report — used to decide whether a
    /// relaunch is needed.
    pub fn model_id(&self) -> &str {
        &self.model.id
    }

    /// Lane count to hand llama-server's `--parallel`, floored at 1. The plan
    /// guarantees ≥ 1; the floor is a defensive guard so a zero can never
    /// collapse the KV cache to nothing.
    pub fn parallel_lanes(&self) -> u32 {
        self.lanes.max(1)
    }

    /// The `-c` value: the TOTAL KV cache to request = per-lane `context_window`
    /// × `lanes`. llama-server splits this evenly across the `--parallel` slots,
    /// so each slot ends up with one full `context_window`. Launching `-c
    /// context_window` alone (no `--parallel`) was the prior bug: llama.cpp
    /// defaulted to 4 slots and quartered the per-request window, 500-ing
    /// deliberations whose prompt fit the planned window. `saturating_mul`
    /// guards the (unreachable on real plans) overflow.
    pub fn served_total_ctx(&self) -> u32 {
        self.context_window.saturating_mul(self.parallel_lanes())
    }

    /// The adapter SET as a sorted list of path strings — the identity llama.cpp
    /// loads by and the key [`ensure_model_serving`] compares to decide whether a
    /// genome change requires a relaunch. Sorted so order can't spuriously trip
    /// the diff.
    pub fn adapter_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .adapters
            .iter()
            .map(|a| a.path.to_string_lossy().into_owned())
            .collect();
        paths.sort();
        paths
    }

    /// The `-ot`/`--override-tensor` VALUE that offloads the COLD layers' stacked expert
    /// tensors to CPU for this target's [`expert_placement`](Self::expert_placement), or
    /// `None` when there is no placement or nothing is cold (every block hot). Delegates to
    /// the pure [`cold_expert_offload_ot`] builder, keyed on the placement's real `blk.N`
    /// hot set. The spawn path applies this as a `--override-tensor` arg.
    pub fn expert_ot_value(&self) -> Option<String> {
        let p = self.expert_placement.as_ref()?;
        cold_expert_offload_ot(&p.hot_layers, p.n_layers)
    }
}


/// Build the `--override-tensor` (`-ot`) VALUE that offloads every COLD MoE layer's
/// stacked expert tensor to CPU, keeping `hot_layers` on the GPU (the default placement).
/// This is the slice-1 buft-override half of K3 physical expert paging.
///
/// Experts are STACKED per layer — `blk.N.ffn_*_exps` is ONE tensor holding all of layer
/// N's experts — so `-ot`, which places WHOLE tensors, pages at LAYER granularity. The
/// residency planner (the `ServingExpertPager`) owns the VRAM fit and hands us the
/// `PlacementRequest.hot_layers` (the real `blk.N` indices that fit GPU); we emit the
/// inverse: the rest to CPU. (True per-expert paging is slice-2 — a sub-range
/// `ggml_backend_tensor_set` upload — and does not use `-ot`.)
///
/// `n_layers` is the total transformer-block count (the iteration ceiling). Cold =
/// `(0..n_layers)` minus `hot_layers`. A dense (non-MoE) block in that range is HARMLESS:
/// it has no `ffn_*_exps` tensor, so its pattern is inert — which is why we need only the
/// hot set + the block count, never the exact MoE-layer set.
///
/// Returns `None` when nothing is cold (every block is hot, or `n_layers == 0`): there is
/// no override to apply, so the caller omits the flag entirely — never an empty `-ot`,
/// which llama-server rejects. The value targets the `CPU` buffer type
/// (`common/arg.cpp::parse_tensor_buffer_overrides`).
pub fn cold_expert_offload_ot(hot_layers: &[u32], n_layers: u32) -> Option<String> {
    use std::collections::BTreeSet;
    // Ignore any hot index outside the block range (a stale plan can't force a bad pattern).
    let hot: BTreeSet<u32> = hot_layers
        .iter()
        .copied()
        .filter(|&l| l < n_layers)
        .collect();
    let cold: Vec<u32> = (0..n_layers).filter(|l| !hot.contains(l)).collect();
    if cold.is_empty() {
        return None;
    }
    // `\.` on BOTH sides of the block index so `blk.3` never also matches `blk.31`, and
    // `ffn.*_exps` covers all four stacked projections (gate/up/down/gate_up).
    let alternation = cold
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("|");
    Some(format!(r"blk\.({alternation})\.ffn.*_exps=CPU"))
}
