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

### CLOSED on both backends — and the one rule that would have saved a day

**Result:** Metal 4.0x decode with zero bytes moved; CUDA V4-Flash coherent at
51 MiB/token against a 3.3 GiB copy baseline. Cut-2 works.

**Every one of the three real defects was the same shape: a kernel reachable
from a gather-capable `supports_op` that did not honor `src[3]`.** They were
found one at a time, each only after the previous fix failed to clear the fault:

1. `supports_op` accepted `ne2 <= 16` while dispatch only routes to the gathered
   mmvq at `ne2 <= get_mmvq_mmid_max_batch()` (4–6); in-between batches fell
   through to MMQ, which has no gather.
2. The **fused gate+up** path reused the gathered `x` offset for the `vgate`
   dot, so every expert computed its gate from expert 0's weights.
3. `mmvq.cu:914` — `has_ids && ncols_dst > 1` dispatches to
   `mul_mat_vec_q_moe`, a separate MoE-specialised kernel that was never
   gathered and read contiguous staging the consume-arm deliberately leaves
   unpopulated for aliased experts.

> **THE RULE:** every kernel reachable from a gather-capable `supports_op` must
> either honor `src[3]` or be excluded by the gate — and *reachable* must be
> **enumerated from the dispatch code**, not from the kernels you remember
> writing. A gather is a promise made by the gate and kept by every kernel the
> gate admits.

### Why it took eleven eliminations: property probes vs. surface bisection

Ten hypotheses died before the bug was found, and in hindsight they failed as a
group rather than individually. Each asked **what** was wrong with the table —
content, address, alignment, integer width, timing, eviction, weight type, table
shape — and every one of those properties was genuinely correct. Nothing about
the table was ever broken; the table was being *ignored* by kernels nobody had
enumerated.

The probe that cracked it asked **where** instead: `GGML_MOE_GATHER_ONLY=<substr>`
engages the gather for only the tensors matching a name, so a fault can be
attributed to a named tensor in log2(N) runs. Its result — *all three* weight
arms broken — was what proved the fault could not be weight-specific and sent
the search back to dispatch, where it was.

**Heuristic worth keeping: when several probes in a row all report "that property
is fine", the question shape is wrong. Stop enumerating properties and bisect the
surface.**

### The instrument ladder (all shipped, all reusable)

| Instrument | Question it answers |
|---|---|
| `GGML_MOE_GATHER_IDENTITY` | wrong bytes, or wrong kernel? (table over provably identical data) |
| cross-allocation test mode | do entries spanning independent allocations work? (width/repr) |
| quant sweep (F32/Q8_0/Q4_0/Q4_K) | is it type-specific? — **F32-only tests never touch mmvq/MMQ** |
| mixed table mode | can the kernel handle entries straddling two regions? (live-cache shape) |
| `GGML_MOE_GATHER_VERIFY` | are the slot bytes correct at publish time? |
| `GGML_MOE_GATHER_SYNC` | is it ordering? (serialize and see) |
| `GGML_MOE_GATHER_POISON` | does the kernel read past a row? (observation, not inference) |
| `GGML_MOE_GATHER_ONLY` | **WHERE** — attribute the fault to a named tensor/layer |
| multi-consumer warning | does another node read the same staging? |

Each is cheap, each returns a fact rather than an argument, and together they
turn a remote-repro hunt into seconds-per-branch instead of a build cycle per
theory.

### The alignment invariant (a real bug, found cross-backend)

**In-place consumption inherits NOTHING from the tensor allocator.** The copy
path launders alignment: bytes land in a tensor-allocated staging buffer, so
every address the kernel touches carries the allocator's guarantees for free.
Under gather the kernel reads at `pool_base + slot*slot_size` instead, so the
cache's own geometry IS the contract.

`slot_size` was `expert_size + pad`, and `expert_size` is only BLOCK-aligned —
IQ2_XXS is 66 B/block, so it need not be a multiple of 16, and slot bases drift
off the vector-load alignment CUDA's quantized `vec_dot` requires. Result:
garbage reads on rarely-hit slots → NaN logits → `llama-sampler.cpp:1098`
assert after many coherent tokens (5090 V4-Flash, 2026-08-03). Fix (655f183a9):
round `slot_size` up to 256 B so every slot base is congruent to the pool base.

**It was latent on Metal too.** Q4_K is 144 B/block = 9×16, so Metal's slot
bases stayed 16-aligned by luck of the quantization — the 4.0× proof was true
but lucky. A green result on one backend/quant is not evidence an invariant
holds. Test in-place paths across a quant matrix.

### The NaN hunt: what it actually was, and what the hardening bought

The 5090 NaN (coherent for many tokens, then `llama-sampler.cpp:1098`) was NOT
an addressing or lifetime bug. Root cause, found by BigMama: **`supports_op`
accepted shapes that DISPATCH does not route to the gather kernel.** Her gate
allowed `src[3]` for any quantized MUL_MAT_ID with `ne2 <= 16`, but CUDA only
routes to the gathered mmvq at `ne2 <= get_mmvq_mmid_max_batch()` (4–6 for
IQ2/Q4). In-between batches fell through to MMQ — which has no gather and reads
the contiguous `input_cpy` the consume-arm deliberately does not populate for
aliased experts. Stale bytes, rare expert, NaN.

**The rule that generalizes: a backend's `supports_op` must mirror its DISPATCH
predicate exactly — which kernel actually runs for the shape, not which one you
think runs.** A mismatch produces garbage that reads convincingly as an
addressing bug and costs a day.

The hunt still bought two real things, both landed as hardening rather than fix:

- **`GGML_MOE_GATHER_IDENTITY`** — publishes the table with natural offsets AND
  keeps every copy, so the kernel's table path runs over bytes identical to the
  proven-good copy path. One run splits "wrong bytes" from "wrong kernel". This
  is the instrument that ended the theory spiral; keep it.
- **Fence precision + optional event retirement** — the fence now refuses only
  slots a published table actually REFERENCES (copy-served experts stay
  evictable, preserving headroom under pressure). Event-proven retirement exists
  behind `GGML_MOE_GATHER_RETIRE=1`: measured, its ring depth must equal the true
  in-flight set (depth 4 proved completion late and took OLMoE full-fit from 100%
  to 67.7% hit) and even at depth 2 the per-call event sync costs ~9% decode on
  Metal — so the conservative window stays the default, sound wherever the
  frontend syncs per token. Rigor available, cost not imposed.

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
