# Gather-capable MUL_MAT_ID — zero-copy expert residency (Cut-2)

**Status:** design agreed in k3-serving 2026-08-03 (M5 + BigMama). Owners: M5 = ggml
op plumbing + consume-arm table build + Metal kernel; BigMama = CUDA kernel + 5090
measure. Fork branch: `CambrianTech/llama.cpp#k3-adopt`.

## Why (measured motivation)

With the device-resident expert cache proven (V4-Flash on one 5090: 100% hit rate,
762/762, 3.05 tok/s = 2.19× the all-stream baseline at tip `fa7e0d8e9`), the
remaining per-token cost is the **consume copy**: every routed expert D2D-copies
from its cache slot into the contiguous `input_cpy` staging tensor that
`MUL_MAT_ID` indexes by expert id. `fault_wait` measures **150–330ms of the 327ms
per-token eval** — removing the copy is a potential second ~2×.

A plain "alias the src view at the slot" does NOT work: `input_cpy` is ONE
contiguous 3-D tensor (`as`, one matrix per expert, indexed `id * nb[2]`), while
cache slots are recency-scattered across the pool. The indirection must live in
the kernel.

## Op contract (minimal diff)

`GGML_OP_MUL_MAT_ID` today: `src[0]=as` (3-D experts), `src[1]=b`, `src[2]=ids`.
`src[3]` is unused → it becomes the **optional expert pointer table**:

- `src[3] = expert_ptrs`: I64 tensor, `ne[0] = n_expert`, device-resident.
  `expert_ptrs[id]` = base address of expert `id`'s weights (same layout/type as a
  row of `as`).
- `src[3] == NULL` → exactly today's contiguous path (`as->data + id*as->nb[2]`).
  Every non-cached arch, every other backend, and all existing tests are
  untouched.
- A backend whose MUL_MAT_ID does not implement the gather **must reject the op in
  `supports_op` when `src[3] != NULL`** so the scheduler falls back (never silent
  garbage). Rollout: Metal first (UMA proving ground), CUDA second; CPU rejects
  initially.

## Table build (consume arm, `ggml-backend.cpp` D2D block)

Replaces the per-expert `tensor_copy` into `input_cpy`:

1. Per ubatch, build the host-side `uint64_t ptrs[n_expert]`:
   - device-resident slot hit → slot base pointer (`buffer_get_base + offset`);
   - miss → fetch into `input_cpy` region as today, table entry points there.
     (Mixed hit/miss ubatches work — the table unifies both cases.)
2. One small H2D upload of the table (`n_expert * 8` bytes — noise), then the
   graph's MUL_MAT_ID consumes it via `src[3]`.
3. **Eviction fence:** each `ExpertSlot` carries a generation stamp; the table
   build records `(slot, gen)` and the pager MUST NOT recycle a slot whose gen is
   pinned by an in-flight graph. Stamp-check at table-build time, release at graph
   completion — no locks on the hot path. (This is the same eviction-race class
   suspected in #43; here it becomes load-bearing.)
4. #43 lesson pinned: whatever tensor/pointer shapes the backend path derefs must
   be real — the table carries raw device addresses, so the kernel never touches
   `tensor->buffer/context` for per-expert bases at all. Host-side `memset`/writes
   against device pointers are forbidden (the #43 crash: prefetch pad-zero
   host-memset on a VRAM pointer; guard landed in `fa7e0d8e9`).

## Kernel change (per backend)

Where the kernel computes the expert base as `src0 + i02*nb02`, it instead reads
`base = ptrs[i02]` when the table is present. Everything else (tiling, quant
dequant, dst indexing) is unchanged — this is an address-generation change, not a
math change.

- **Metal:** UMA makes hit-pointers and miss-pointers the same address space;
  simplest correct first implementation. Also directly attacks the known
  Metal-MoE expert-gather bottleneck (K3's 2 tok/s slot-contention class).
- **CUDA:** same shape; measure against the 150–330ms fault_wait on the 5090.

## Measurement plan

Same probes as tonight: `[MOE-PAGER]` hit line + `fault_wait` split + decode
tok/s, A/B `src[3]` on/off at identical config. Success = fault_wait collapses to
the table upload (< 1ms) and decode approaches the residency-only ceiling
(BigMama's non-monotonic budget sweep supplies the per-budget expectation).

## Why this is a "fit" feature, not just a speed feature

The copy-free path makes SMALL cache budgets viable: today a 6GB budget pays the
same per-token consume copies as a 12GB one; with gather, a hit costs nothing but
the address. That moves the whole GPU-tier table up a row (12/16GB cards) and is
the same mechanism the M5-class UMA path uses when a model DOESN'T fully fit —
the difference between "fits at 128GB" and "fits well at 64GB".

## Metal implementation map (recon 2026-08-03, fork @ b4be42ff8)

- **Dispatch:** `ggml-metal-ops.cpp:2292 ggml_metal_op_mul_mat_id` — binds ONE
  `bid_src0` (buffer id + offset) for the whole expert tensor; shaders compute
  per-expert bases via `nb02` strides. Two kernel families by batch size:
  matrix-matrix (`kernel_mul_mm_id_*`, ne21 ≥ 32, with a `map0` id-mapping
  pre-pass) and matrix-vector (`mul_mv_id`), shaders in `ggml-metal.metal:10308+`.
- **Addressing decision (Metal-specific):** MSL addresses through BOUND buffers,
  not raw device addresses. On Metal the `src[3]` table therefore carries
  **byte OFFSETS into a bound expert-pool buffer** (one extra buffer binding =
  the cache pool; every ExpertSlot already lives in it), not raw 64-bit
  addresses. CUDA kernels deref raw device addresses directly, so its table
  carries absolute pointers. The op contract stays one I64 tensor; each
  backend's consume-arm table build writes the representation its kernel reads
  (documented per-backend, asserted at build time).
- **Kernel change per family:** where the shader computes
  `src0 + i02*args.nb02`, read `pool_base + ptrs[i02]` (Metal) /
  `(const char *) ptrs[i02]` (CUDA) when the table binding is present. The
  `map0` pre-pass is address-agnostic (it maps token→expert rows) — unchanged.
- **Fallback path already safe:** supports_op rejects `src[3] != NULL` on all
  backends as of b4be42ff8, so partial rollout can never compute a wrong
  contiguous-stride result.
