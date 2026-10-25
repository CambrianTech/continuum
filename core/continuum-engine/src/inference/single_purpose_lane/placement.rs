//! Where a single-purpose lane runs: GPU-first under a governed VRAM lease, spilling to the
//! CPU only when the governor refuses, with the decision recorded in the placement glass box.

use crate::inference::llama_server::LanePlacement;
use continuum_base::time::EpochMs;

/// Who holds a single-purpose lane's governed slot, and for how long before the governor may
/// reclaim it. Each kind of lane names its own (a gene eval, a training lane).
#[derive(Debug, Clone, Copy)]
pub struct LaneLeaseTerms {
    pub consumer: &'static str,
    pub ttl_ms: u64,
}

/// Small headroom (bytes) kept free on the GPU so a lane placed right at the edge
/// can't trip Metal's decode-time command-buffer OOM. Deliberately SMALL: the
/// policy is GPU-FIRST — fill the accelerator, aim for ~100% GPU utilization
/// ([[optimization-is-always-first]]; Joel: "near 100% GPU utilization if you're
/// doing it right. Fill in GPU lanes first"). This keeps us off the exact cliff;
/// it is NOT a conservative reserve that idles the GPU.
pub const GPU_PLACEMENT_MARGIN_BYTES: u64 = 256 * 1024 * 1024;


/// Where a coexisting single-purpose lane runs + WHY. Rides out on the eval result so a CPU
/// placement is VISIBLE in the harness (a CPU-pinned lane is ~10× slower; surfacing
/// the device + the headroom numbers means a slow run is explained by data, never a
/// silent degrade). The benchmark self-reports which device it measured on.
#[derive(Debug, Clone)]
pub struct PlacementEvidence {
    pub placement: LanePlacement,
    pub device: String,
    pub reason: String,
    pub free_vram_bytes: Option<u64>,
    pub footprint_bytes: Option<u64>,
}

/// GPU-FIRST placement policy: pack the accelerator, spill to CPU ONLY when the GPU
/// genuinely can't hold this lane alongside what's already resident. Pure (no I/O)
/// so the policy is unit-testable; the caller supplies the live `free` VRAM (from
/// the GPU monitor — already net of the living persona's resident lane) and this
/// lane's estimated `footprint`.
///
/// - no GPU on the node (`free=None`)   → CPU is the only device (the honest truth,
///   not a fallback — there is nothing to fall back FROM)
/// - footprint unknown (`free=Some`)    → GPU (optimistic: pack it; llama.cpp
///   offloads what fits — never idle the accelerator on a sizing miss)
/// - footprint fits in free (− margin)  → GPU
/// - footprint exceeds free             → CPU spill (the GPU is full; SAY so, loud)
///
/// The seam #56's `ResourceGovernor` will later own this — and extend it to PARTIAL
/// `--n-gpu-layers N` offload (fill the GPU to the brim, spill only the residue to
/// CPU) for true ~100% utilization. Today's enum is binary GPU/CPU; this is the
/// GPU-first bias on that binary.
pub fn choose_lane_placement(
    free: Option<u64>,
    footprint: Option<u64>,
) -> (LanePlacement, &'static str) {
    match (free, footprint) {
        (None, _) => (
            LanePlacement::Cpu,
            "no GPU backend on this node — CPU is the only device",
        ),
        (Some(_), None) => (
            LanePlacement::Gpu,
            "GPU-first: lane footprint unknown, offloading to GPU",
        ),
        (Some(f), Some(fp)) if f >= fp.saturating_add(GPU_PLACEMENT_MARGIN_BYTES) => (
            LanePlacement::Gpu,
            "GPU-first: lane fits in free VRAM alongside resident lanes",
        ),
        (Some(_), Some(_)) => (
            LanePlacement::Cpu,
            "GPU full: free VRAM below lane footprint + margin — spilling this lane to CPU",
        ),
    }
}

/// GPU-FIRST placement decision for a coexisting single-purpose lane: probe live free VRAM
/// (net of the living persona's resident lane) + size this base from its GGUF on
/// disk, then [`choose_lane_placement`]. Emits the decision into the placement glass
/// box (`~/.continuum/fixtures/placement-decisions/decisions.jsonl`) and packages the
/// evidence for the harness. `context` labels what asked for the lane; `served_ctx`
/// is the KV-sizing window — both land in the structured log.
pub fn decide_lane_placement(
    base: &crate::model_registry::Model,
    served_ctx: u32,
    context: &str,
    terms: LaneLeaseTerms,
) -> (PlacementEvidence, Option<crate::resources::LeaseGuard>) {
    // ONE sizing, shared with the pre-spawn RAM gate (compression). None → couldn't
    // size (treated optimistically as GPU-first below).
    let footprint = super::lane_footprint(base);

    // #56/G1 — ASK THE ONE GOVERNOR FOR THE SLOT, don't read raw free and race.
    // The eval lane is the top concurrent-OOM source: it stands up a SECOND
    // llama-server, and it used to place GPU-vs-CPU off `gpu::monitor::detect()`'s
    // raw free bytes — which count serving's RESERVED-but-not-yet-allocated prefill
    // buffer as "free", so the eval placed on GPU and serving's next prefill OOM'd.
    // Now it `acquire`s a real VRAM lease of its footprint from the resource
    // authority: an atomic check-and-reserve against the GOVERNED available (physical
    // free minus every other consumer's guarantee, now peak-accurate after G5).
    //   granted  → GPU, and HOLD the lease so a concurrent serving tick sees these
    //              bytes as taken and won't tier up into them
    //   refused  → the governed GPU is full → spill to CPU (VISIBLE, ~10× slower,
    //              never an OOM) — "pressure, not OOM"
    //   ungoverned node (no daemon) → the ORIGINAL raw-free probe, unchanged
    let (placement, reason, lease, free_vram) = acquire_lane_slot(terms, footprint);
    let device = match placement {
        LanePlacement::Gpu => "gpu",
        LanePlacement::Cpu => "cpu",
    };
    // Formal capture — a load-bearing decision gets a structured record, not just a
    // log line ([[observability-is-half-the-architecture]]). Best-effort: a sink
    // open/write failure never blocks the lane.
    use crate::inference::placement_capture::{
        JsonlPlacementCaptureSink, PlacementDecisionRecord, SCHEMA_VERSION,
    };
    JsonlPlacementCaptureSink::glass_box().record(&PlacementDecisionRecord {
        schema_version: SCHEMA_VERSION,
        captured_at_ms: EpochMs::now().millis(),
        context: context.to_string(),
        model: base.id.clone(),
        served_ctx,
        device: device.to_string(),
        reason: reason.to_string(),
        free_vram_bytes: free_vram,
        footprint_bytes: footprint,
        margin_bytes: GPU_PLACEMENT_MARGIN_BYTES,
    });
    // One concise operator line for live tailing; the JSONL above is the audit trail.
    tracing::info!(
        target: "continuum_core::inference::single_purpose_lane",
        device, reason, context,
        free_vram_bytes = ?free_vram,
        footprint_bytes = ?footprint,
        "single-purpose lane placement"
    );
    (
        PlacementEvidence {
            placement,
            device: device.to_string(),
            reason: reason.clone(),
            free_vram_bytes: free_vram,
            footprint_bytes: footprint,
        },
        lease,
    )
}

pub fn lane_request(terms: LaneLeaseTerms, bytes: u64) -> crate::resources::LeaseRequest {
    crate::resources::LeaseRequest {
        consumer_id: terms.consumer.to_string(),
        kind: crate::resources::ResourceKind::Vram,
        bytes,
        ttl_ms: terms.ttl_ms,
        // The actual process owner retains this grant through observed child exit.
        reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
    }
}

/// Ask the ONE resource authority for this lane's VRAM slot (#56/G1) and
/// decide GPU/CPU from the answer. Returns `(placement, reason, held-lease,
/// observed-free-vram)`. Split out so the acquire/refuse/ungoverned branching is
/// unit-testable and `decide_lane_placement` stays about capturing evidence.
///
/// - governed + sized → `acquire_guarded`: Ok ⇒ GPU + hold the lease; refused ⇒ CPU
/// - governed + unsized (GGUF unstatable) → optimistic GPU, ungoverned (rare; the
///   model likely won't load anyway) — surfaced in the reason
/// - ungoverned node (no `ResourceDaemon::global()`) → the original raw-free probe
pub fn acquire_lane_slot(
    terms: LaneLeaseTerms,
    footprint: Option<u64>,
) -> (
    LanePlacement,
    String,
    Option<crate::resources::LeaseGuard>,
    Option<u64>,
) {
    use crate::resources::{LeaseError, ResourceDaemon};
    match (ResourceDaemon::global(), footprint) {
        (Some(daemon), Some(fp)) => {
            let req = lane_request(terms, fp);
            match daemon.acquire_guarded(&req) {
                Ok(guard) => {
                    let remaining = governed_vram_available();
                    (
                        LanePlacement::Gpu,
                        "GPU-first: governor granted a VRAM lease for this lane".to_string(),
                        Some(guard),
                        remaining,
                    )
                }
                Err(LeaseError::InsufficientCapacity { available, .. }) => (
                    LanePlacement::Cpu,
                    format!(
                        "GPU full (governed): available {available}B < lane footprint {fp}B \
                         — spilling this lane to CPU",
                    ),
                    None,
                    Some(available),
                ),
                Err(e) => (
                    // Any non-capacity lease error is unexpected. Fail SAFE to CPU
                    // (never place on GPU ungoverned behind the governor's back) and
                    // say why — a slow CPU lane is recoverable, a blind GPU OOM is not.
                    LanePlacement::Cpu,
                    format!("governor lease error ({e:?}) — spilling to CPU rather than placing ungoverned"),
                    None,
                    None,
                ),
            }
        }
        (Some(_), None) => (
            LanePlacement::Gpu,
            "GPU-first: lane footprint unknown, offloading to GPU (ungoverned — could not size to lease)"
                .to_string(),
            None,
            None,
        ),
        (None, _) => {
            // Ungoverned node: the ORIGINAL raw-free probe, behavior unchanged.
            // `and_then`, not `map`: "no GPU" and "GPU with no reading yet" are
            // the same answer to a placement question — unknown — and
            // `choose_lane_placement` already models unknown as `None`.
            let free_vram = crate::gpu::monitor::detect().and_then(|m| m.free_bytes());
            let (placement, reason) = choose_lane_placement(free_vram, footprint);
            (placement, reason.to_string(), None, free_vram)
        }
    }
}

/// The governed VRAM `available` right now (the board's Vram lease-headroom), or
/// `None` when VRAM is ungoverned. Local read for placement evidence; mirrors the
/// serving daemon's `governed_vram_ceiling` (same board field, read-only).
pub fn governed_vram_available() -> Option<u64> {
    crate::resources::ResourceDaemon::global().and_then(|d| {
        d.board()
            .kinds
            .iter()
            .find(|k| k.kind == crate::resources::ResourceKind::Vram)
            .map(|k| k.available_bytes)
    })
}
