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

## Table build (consume arm, `ggml-backend.cpp` D2D block) — BUILT (fork, 2026-08-03)

Replaces the per-expert copy into `input_cpy`. As landed:

1. **Opt-in:** `GGML_MOE_GATHER=1` (`moe_config().gather`) — off means every path
   is byte-for-byte the pre-gather serve.
2. **Per-backend repr behind ONE seam:** the proc-address extension
   `"ggml_backend_moe_gather_entry"` (`ggml_backend_moe_gather_entry_t` in
   `ggml-backend.h`) builds one table entry from `(src0_cpy, slot_buf, slot_off)`.
   Metal returns a **GPU-VA byte delta relative to src0's tensor start** computed
   from `MTLBuffer.gpuAddress` (`ggml_metal_buffer_gpu_va`) — CPU-pointer deltas
   do NOT transfer across MTLBuffers, so host arithmetic is never used. CUDA
   (BigMama's half) returns an absolute device pointer. A backend without the
   proc, or a slot the repr can't address → per-expert fallback to the copy path.
3. **Table lifecycle:** `MoeGatherTables` — process-global pool (mirrors
   `moe_expert_cache()` ownership), one I64 tensor + device buffer + host staging
   per MUL_MAT_ID node per call, claimed in order, reset each compute call,
   sub-pooled by `sched->cur_copy` so pipeline-parallel copies never clobber a
   table an in-flight graph still reads. Every entry defaults to the expert's
   natural offset inside `input_cpy` (built through the same proc), so unused
   experts stay valid and per-expert fallback needs no bookkeeping. ONE
   `tensor_set_async` upload per node per ubatch, enqueued inside the same
   event-guarded region as the `input_cpy` writes (inherits their ordering
   contract). The node's `src[3]` is reset to NULL before each build; a
   `supports_op` probe with `src[3]` set gates engagement per backend.
4. **Eviction fence (landed shape):** `ResidencyCache::set_gather_fence(true)` is
   armed sticky on first gather engagement; `reserve_slot` then refuses ANY
   victim with `slot_gen == token_gen` (returns `NO_SLOT` → caller falls back to
   copy/mmap for that expert). Rationale: a slot touched this generation may sit
   in an already-published table whose kernel runs later; the `[RETAIN] evict>0`
   probe proved intra-token eviction really happens when a token's working set
   exceeds the pool. Flag-gated, not unconditional — container mode (no mmap
   fallback) relies on admission and would turn pool-too-small into an abort.
5. #43 lesson pinned: whatever tensor/pointer shapes the backend path derefs must
   be real — the table carries backend-repr addresses, so the kernel never touches
   `tensor->buffer/context` for per-expert bases at all. Host-side `memset`/writes
   against device pointers are forbidden (the #43 crash: prefetch pad-zero
   host-memset on a VRAM pointer; guard landed in `fa7e0d8e9`).

Observability: `[MOE-PAGER]` line carries `gather=N` (experts aliased, zero bytes
moved) and the capture JSONL carries `"gathered"`.

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

### First live A/B — Metal, M5, 2026-08-03 (MEASURED)

OLMoE-1B-7B Q4 (real 64-expert MoE), `--device MTL0 -ngl 99 --no-repack
-ot "exps=CPU"`, `GGML_MOE_VRAM_CACHE_GB=2`, `GGML_OP_OFFLOAD_MIN_BATCH=1`,
temp-0 coherence verified both arms (identical output prefix):

| identical config, 100% hit | copy path | gather path |
|---|---|---|
| decode tok/s | 12.23 | **49.09 (4.0×)** |
| bytes moved / token | 465.2 MiB | **0.0 MiB** |
| per-token expert servicing | ~80 ms | ~20 ms |

`[MOE-PAGER] experts=384 gather=384 bytes=0.0 MiB hit_rate=100.0%` — every
expert of every token serviced in place through the table. The 5090/CUDA A/B on
V4-Flash (BigMama's lane) measures the same mechanism at scale.

### Serving-config traps (cost a kernel panic + two dead smokes to learn)

1. **CPU repack silently disables the whole path.** Expert tensors placed by
   `-ot exps=CPU` get captured into `CPU_REPACK` buffers (transformed layout);
   ops on repacked weights are never offloaded, so the expert-streaming path
   no-ops with ZERO diagnostics — no `[MOE-PAGER]`, no error, just slow CPU
   decode. `--no-repack` is REQUIRED for offloaded-expert serving on Metal.
2. **Decode never offloads by default.** Metal's `offload_op` min-batch
   threshold (default 32, `GGML_OP_OFFLOAD_MIN_BATCH`) keeps batch-1 decode on
   CPU. Forcing it to 1 is what the gather makes AFFORDABLE (zero-copy hits) —
   but never force it when the expert working set exceeds free RAM: that exact
   combination (K3 49GB on 64GB UMA) fault-stormed against wired Metal memory
   and watchdog-panicked the M5. Headroom precheck first, always.
3. **Verify the device engaged.** One fork llama-server launch came up
   CPU-only silently despite `-ngl 99`; explicit `--device MTL0` fixed it.
   Check `offloaded N/N layers` in the load log before trusting any number.

## Why this is a "fit" feature, not just a speed feature

The copy-free path makes SMALL cache budgets viable: today a 6GB budget pays the
same per-token consume copies as a 12GB one; with gather, a hit costs nothing but
the address. That moves the whole GPU-tier table up a row (12/16GB cards) and is
the same mechanism the M5-class UMA path uses when a model DOESN'T fully fit —
the difference between "fits at 128GB" and "fits well at 64GB".
