# Expert Paging — the governor seam

*M5 (Fable) design note for BigMama's paging lane, 2026-07-25. The governor
machinery is mine (`capacity/lease.rs`, `resources/`, `cognition/prefill_throttle.rs`);
the llama.cpp/CUDA/disk mechanics are BigMama's. This doc is the contract
between them so we build to one shape.*

## The thesis (Joel)

Page MoE experts like we page genome LoRA — hot experts resident in VRAM, cold
experts on the RAM→16TB hierarchy, faulted in per token. "Even a slower paged-in
expert (not super slow) would help greatly, especially if we lose access to
[the cloud]." Target: a K3-class MoE (2.8T, weights 2026-07-27) hosted on the
5090 via expert paging = a near-frontier local brain, the outage insurance.

## What already exists (don't rebuild)

- **`DeviceCapacity`** (`capacity/mod.rs`) — `gpu_total`, `gpu_free_live`,
  `system_ram_free`. The governor already reads live free VRAM
  (`governed_vram_ceiling`) and RAM.
- **`FitPolicy` / `lanes_that_fit`** — the ONE fit rule, sim-proven. Answers
  "does this residency fit the live free axis after a safety margin?"
- **`decide_lane`** (`capacity/lease.rs`) — the leasability→local-fit→peer
  decision. Expert placement is the SAME shape one level down (which experts fit
  THIS device now).
- **Genome paging** (the LoRA slot machinery) — the exact pattern: a bounded
  resident set, LRU eviction under pressure, page-in on demand. Experts are
  genome slots with a different payload.
- **The disk hierarchy** (BigMama's cold-storage: `CONTINUUM_STORAGE_PATH`,
  16TB D:) + `TrackedDir`/`disk_eviction` — where cold experts live.

## The seam: `ExpertResidencyPolicy`

Mirror `FitPolicy`. A pure decision fn the governor owns; llama.cpp is the
mechanism it drives.

```
ExpertResidencyPlan {
    hot:  Vec<ExpertId>,   // pin resident in VRAM (attention+shared always hot)
    warm: Vec<ExpertId>,   // RAM-resident, fault to GPU per token (the --n-cpu-moe set)
    cold: Vec<ExpertId>,   // on disk (16TB), fault to RAM on first route
}

fn plan_expert_residency(
    routing_profile: &ExpertRoutingProfile,  // measured: which experts THIS
                                              // persona's workload routes to (PGO)
    cap: &DeviceCapacity,                     // live free VRAM/RAM (existing)
    expert_bytes: u64,                        // per-expert footprint
    margin: u64,                              // same safety margin FitPolicy uses
) -> ExpertResidencyPlan
```

Rule (the doctrine, one fit everywhere):
1. **Shared + attention weights are always HOT** — GPU-resident, non-negotiable
   (they're on the critical path for every token). This is the `--override-tensor`
   / `--n-cpu-moe` split: keep the dense trunk on GPU, page the expert FFNs.
2. **HOT experts = the routing profile's top-K by measured activation**, as many
   as `lanes_that_fit(gpu_free_live, margin, expert_bytes)` allows. This is PGO
   for residency — a coding persona pins the experts coding routes to; a
   conversational persona pins different ones. **The profile is per (persona,
   TaskKind)** — same measurement the ThroughputLease already scopes by.
3. **WARM = the RAM budget's worth** below the hot set (fault-to-GPU per token,
   the standard consumer-MoE trick — measurable latency cost, bounded).
4. **COLD = everything else on disk**, faulted RAM→GPU on first route, and its
   dir gets a `TrackedDir` + eviction story (the 460GB-incident rule; no
   unbounded cache class without a decided eviction).

## What I (M5) own

- `ExpertRoutingProfile`: capture which experts a persona's turns actually route
  to (the llama.cpp server can emit per-token expert selection; we fold it into
  a per-(persona,TaskKind) histogram — the PGO signal). Lives beside the genome
  profile.
- `plan_expert_residency` + its sim scenarios (same harness `FitPolicy` uses:
  partition/join/pressure all proven before prod).
- Governor wiring: the residency plan flexes on the SAME resource-authority tick
  the prefill throttle flexes on — VRAM pressure shrinks the hot set (evict to
  warm), free VRAM grows it back. Sticky hysteresis so it never thrashes
  (`[[never-thrash-sticky-hysteresis]]`).
- **Grid extension**: an expert set is a service-class offer (card 7382169f) —
  a node gossips which experts it's hot for; `decide_lane` can route a token's
  expert-heavy work to the peer resident-hot for that expert (mesh-MoE affinity,
  `[[mesh-llm-competitive-row-and-moe-affinity-edge]]`). This is the grid win
  over single-node layer splits.

## What BigMama owns

- The llama.cpp launch: `--n-cpu-moe` / `--override-tensor exps=CPU` (or the
  finer per-expert placement if the build supports it) driven by the plan's
  hot/warm split. Prove the K3 (or a smaller MoE first — Qwen3-MoE) serves on the
  5090 with experts paged.
- Per-expert footprint measurement (`expert_bytes`) — feed it to the policy so
  the fit is real, not guessed.
- The disk tier: cold experts on the 16TB, faulted to RAM; the `TrackedDir` +
  eviction registration for that cache class.
- LoRA-on-top: confirm a genome adapter still applies over the paged base
  (weights are weights regardless of residency — should be free, but verify).

## Build order (outlier-validation)

1. **Outlier A (simple)**: a small MoE (Qwen3-30B-A3B) served on the 5090 with
   `--n-cpu-moe` at a STATIC split — prove the mechanism, measure per-token
   latency cost of a warm fault. (BigMama)
2. **Outlier B (extreme)**: K3-class (2.8T) — does the disk tier even hold, does
   a cold fault stay "not super slow"? (BigMama, when weights land)
3. If both serve, the governor-owned dynamic plan (`plan_expert_residency` +
   routing profile) slots in — static split becomes governor-decided. (M5)
4. Grid: expert-affinity service-class offer + `decide_lane` routing. (both)

The interface is proven when a static split works on both the small and the huge
MoE; then the dynamic governor is trivial. Same discipline as every adapter.
