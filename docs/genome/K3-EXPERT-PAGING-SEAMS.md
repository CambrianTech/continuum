# K3 Dynamic Expert Paging — the 3 seams (BigMama ↔ M5 pairing)

**Status:** design-for-review, 2026-07-25. Owners: BigMama (Seam 1), M5 (Seam 2),
joint (Seam 3). Weights land 2026-07-27; this is the pre-weight design pass.

## Why this exists

Kimi K3 is a 2.8T sparse MoE (896 experts, 16 awake/token, ~50–60B active, native
MXFP4). It cannot be served whole on any single GPU (~594GB). It is the exact artifact
the genome pager was built for: page the active experts, freeze the 896-pool on the
Cold/Frozen tier (D:). See engrams `kimi-k3-architecture`, `kimi-k3-grid-strategy`,
`moe-expert-paging-feasibility`.

**The pager already exists** in `core/continuum-core/src/genome/` — do NOT rebuild it.
`PageKind::MoEExpert` is documented for exactly this case ("the artifact is the full
expert set; offset picks one expert"). `tier.rs` has the 5 roles + `EvictionPolicy`
(Cold = `DemandAlignedWithRefinedPreference`). `bus.rs` auto-publishes page_in/out/
eviction and the sentinel-observer already consumes eviction records = the PGO loop.
M5's parallel `capacity/` residency work folds INTO genome (nothing consumes it yet).

## The 3 seams

### Seam 1 — INGEST (BigMama)
Take a K3 GGUF + its expert layout → register each of the 896 experts (per MoE layer)
as a content-addressed artifact on the **Frozen/Cold** tier (D:).

Real types it builds on:
- `working_set::ArtifactId` — sha256-derived UUID, content-addressed. One per expert
  `(layer, expert_id)` tile.
- `blob::ArtifactBlob { id, bytes }` + `blob::Provenance::minimal(id, created_at_ms)`.
- `tier::TierStore::write(blob, provenance, role)` — the sink.
- `working_set::{PageKind::MoEExpert, PageOffset, PageRef}` — `PageOffset` addresses one
  expert within the full-expert-set artifact.

**Open decision (the fork the splitter forces): `ArtifactBlob` is inline `bytes: Vec<u8>`.**
An MoE expert is hundreds of MB; inlining 896 of them through the value type / bus is a
non-starter. `blob.rs` already anticipates this: "later PRs replace with a tier-aware
handle (mmap, ref-counted Arc, GPU buffer ID) so large artifacts don't round-trip through
the message bus." Seam 1 needs that handle NOW. Proposal: add an `ArtifactSource` enum to
the blob value side — `Inline(Vec<u8>)` (LoRA/engram, unchanged) | `Mapped { path, offset,
len }` (expert tiles: mmap a slice of the GGUF-on-D: without copying). Splitter emits
`Mapped` handles; TierStore::write accounts size from `len`, never reads the bytes. This
keeps small-artifact callers untouched and gives large-artifact paging zero-copy from D:.
**Needs M5 sign-off — it touches the shared `TierStore::write` value contract.**

Splitter interface (sketch):
```
fn split_gguf_into_experts(gguf: &Path, layout: &MoeLayout, tier: &mut dyn TierStore)
    -> Result<Vec<PageRef /* one per expert tile */>, TierError>
```
Layout (n_layers, n_experts, per-expert tensor offsets/lens) comes from GGUF metadata —
llama.cpp's gguf-py already parses `expert_count` / per-expert tensor keys; we read the
same header in Rust (no Python at runtime).

### Seam 2 — SIGNAL / PRIORITY (M5)
Demand-aligned MoE residency priority = gate-magnitude prior + live activation hits,
implemented as the `MoEExpert` branch of `eviction.rs::rank_pages_for_eviction` (fits
Cold's `DemandAlignedWithRefinedPreference`). The missing half is the **hit producer**:
an "expert E just fired" emitter from the live serving loop, published on the existing
bus so the policy + sentinel consume it. This is where the Python `profile_expert_activation`
algorithm lands as live Rust — the offline proof becomes the online signal. M5 posts the
policy + producer design before cutting code.

### Seam 3 — PLACEMENT (joint, blocked on 7/27 + backend)
The residency decision must actually move expert tensors into VRAM/RAM for the serving
backend (llama.cpp `-ot`/`--override-tensor`, or our own kernel). This is the only
genuinely new work. Pre-weight task: pin the `WorkingSet` ↔ engine contract —
- who owns the VRAM budget (governor `WorkingSetCapacity` vs the engine's own allocator),
- how a `page_in(PageRef)` decision maps to an `-ot` tensor placement (or a buffer upload),
- eviction ordering when the engine and the pager disagree.
Impl deferred until weights + backend choice; contract designed now.

## Division
- BigMama: Seam 1 splitter + the `ArtifactSource::Mapped` handle proposal.
- M5: Seam 2 policy + hit-producer.
- Joint: Seam 3 contract doc (this file's §Seam 3 expands into it).
- Parked: M5's `recursion_depth` / MoR planner — a compute-allocation axis, not paging.

## Review protocol (symmetric)
Each owner posts design before code; the other sanity-checks. Seam 1's open decision
(`ArtifactSource::Mapped`) blocks on M5 because it touches the shared value contract.
