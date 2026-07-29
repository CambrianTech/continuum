# K3 GPU expert paging — modify the llama.cpp fork so experts stream to VRAM (never CPU compute)

**Status:** design locked 2026-07-29 (Joel: "K3 and yea we need to modify llama to
accommodate" + "never let inference or training EVER run on cpu"). Implementation is
the next focused build. This is tasks #23 (LiveUploadPager) + #28 realized IN the engine.

## The problem stock llama.cpp cannot solve
A 662GB MoE (K3 UD-IQ2_XXS) on a 32GB GPU has exactly two stock modes:
- `-ngl 99` (all experts on GPU) → **271GB cudaMalloc → OOM** (measured 2026-07-29).
- `--n-cpu-moe N` (experts on CPU) → the FFN matmul runs on the **CPU backend** →
  the forbidden slow path (Joel: never CPU).

Stock llama.cpp has **no** "keep experts in host storage, stream the router-selected
ones to VRAM per token, compute on GPU" mode. That mode is the thing we must add — it
IS our ServingExpertPager (#20/#22) realized inside the engine.

## The modification surface (located)
- **`src/llama-graph.cpp:1810` `llm_graph_context::build_moe_ffn`** — the ONE shared
  MoE FFN (every MoE arch + `models/kimi-k3.cpp` route through it). Experts enter as
  `up_exps` / `gate_exps` / `down_exps`; `ggml_mul_mat_id(..., selected_experts, ...)`
  does the compute on whatever backend those tensors live on. This is the intercept.
- **`src/llama-model.cpp` + `common/common.cpp` + `common/arg.cpp`** — where a tensor's
  buffer type is chosen (`--n-cpu-moe` / `-ot` / `tensor_buft_override`). The new
  "paged" buft-class is declared here.
- **`ggml/src/ggml-cuda/`** — where the async host→device stream + VRAM slot cache live.

## The design (never CPU compute)
1. **Host residency, pinned.** Expert weights live in pinned host RAM (cudaHostAlloc)
   — or flash-backed for the cold tail (ties to task #28 read-based residency). Pinned
   so the DMA is fast; NEVER a CPU compute buffer.
2. **VRAM expert cache.** A fixed pool of `S` GPU expert slots (S sized to fit 32GB
   after dense+KV). Holds the hot working set.
3. **Per-MoE-FFN, per token:** after routing picks the top-k experts, for each selected
   expert NOT resident in a VRAM slot: `cudaMemcpyAsync` its up/gate/down rows host→a
   VRAM slot (LRU-evict the coldest), on the compute stream so the copy overlaps the
   previous expert's matmul. Then `ggml_mul_mat_id` runs against the **VRAM** copies →
   GPU compute, always.
4. **LRU + prefetch.** LRU eviction of slots; the cross-layer predictor (already in
   `capacity/expert_predictor.rs`) prefetches layer L+k's likely experts while L
   computes. Residency POLICY is driven by our pager (#20); the engine provides the
   MECHANISM (the slot cache + async stream).
5. **Correctness:** a paged expert computes the bit-identical result of a resident one
   (same weights) — paging is a SPEED lever only, never accuracy ([[K3-PAGING-DIAGNOSIS]]).

## The seam to our substrate
The engine exposes an `upload_expert(layer, expert_id, host_ptr)` / slot-cache API; our
Rust ServingExpertPager (#20/#22) decides WHICH experts are hot (sentinel-PGO from the
`ffn_moe_topk` observer) and drives residency. Engine = mechanism, our pager = policy.
This is the A-path of [[k3-slice2-A-vs-B-decision]] (weight-write into a resident slot)
made real in CUDA, not the harder K-slot router remap (B).

## Build gate (step 0 — unblock BEFORE implementing)
The fork does not build on this box: cmake 3.30.5 rejects the auto-selected
"Visual Studio 18 2026" generator (VS18-2026/cmake drift, [[windows-build-env-drift]]).
Unblock: **Ninja generator + vcvars** — `ninja.exe` is present
(`~/.continuum/tools/ninja/bin`), `cl.exe` needs vcvars sourced. Set
`CMAKE_GENERATOR=Ninja` + enter vcvars so cmake uses Ninja+cl and never touches the VS
generator. This ALSO unblocks the continuum-core build (the reason our pager couldn't
run here). Fix this first — it has blocked every build/test all session.

## Sequencing
0. Unblock the build (Ninja+vcvars) — gate for everything.
1. ggml-cuda: pinned-host expert store + VRAM slot cache + async host→device stream.
2. Wire into `build_moe_ffn`: route selected experts through the slot cache; `mul_mat_id`
   on VRAM copies.
3. LRU eviction + cross-layer prefetch hook.
4. Expose `upload_expert`/slot API; drive residency from our ServingExpertPager.
5. Measure hot-set hit-rate + tok/s on K3 IQ2_XXS; iterate to par ([[K3-PAGING-DIAGNOSIS]]).

## Then: better sharding + tailored quants (Joel's "go from there")
Once GPU expert paging serves K3 on one box: our cross-node expert sharding
(`node_content`→`ArtifactResidency`, experts split across grid nodes) and our
working-set-tailored quant (compaction, not Unsloth's static blob) layer on top.
