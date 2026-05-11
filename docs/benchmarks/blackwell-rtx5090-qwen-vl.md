# Blackwell RTX 5090 sm_120 — Qwen-VL baseline bench

First-pass perf and correctness validation of the local multimodal path
required by the `#1072` sensory persona alpha contract, measured on the
Blackwell tier (RTX 5090, compute capability 12.0, sm_120, FP4 tensor
cores).

Reproducer: [`scripts/bench-blackwell-vl.sh`](../../scripts/bench-blackwell-vl.sh).
Runs in a `nvidia/cuda:12.8.0-devel-ubuntu22.04` container with
`--gpus all`, builds llama.cpp upstream HEAD from source targeting
`sm_120`, downloads Qwen2-VL-7B Q4_K_M + mmproj-f16, runs `llama-bench`
(text-only) and `llama-mtmd-cli` (vision smoke).

## Hardware

| Field            | Value                                |
| ---------------- | ------------------------------------ |
| GPU              | NVIDIA GeForce RTX 5090              |
| Compute cap      | 12.0 (sm_120, Blackwell)             |
| VRAM total       | 32 606 MiB                           |
| Driver           | 591.55                               |
| CUDA toolkit     | 12.8.0                               |
| Host             | Windows 11 Pro, WSL2, Docker Desktop |

## llama.cpp build

Upstream `ggerganov/llama.cpp` at `e936660` (2026-05-11,
"Ggml/cuda snake fusion hardening #22912"). Built with
`-DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=120-real`. Continuum's
vendored llama.cpp is at `e21cdc11a` (2026-04-13) — 28 days older;
refresh would pick up the snake-fusion-hardening and any Qwen patches
landed in the interval.

## Results

### Text-only (`llama-bench`, `-ngl 99 -p 512 -n 128 -r 3`)

| Test  | Tokens/sec       |
| ----- | ---------------- |
| pp512 | 12 345.58 ± 1 674.49 |
| tg128 | 214.61 ± 28.74   |

Model size: 4.36 GiB on disk (`Qwen2-VL-7B-Instruct-Q4_K_M.gguf`),
7.62 B parameters, full 99-layer offload, CUDA backend. VRAM
footprint residual after bench: ~1.4 GiB (model + KV cache cleared
between repeats).

Context for the numbers: a 7B Q4_K_M model on RTX 4090 (Ada, sm_89)
typically lands at ~120–150 t/s tg128 and ~6 000–8 000 t/s pp512
with the same llama.cpp config. Blackwell sm_120 is roughly
30–40 % faster on this workload here, consistent with the higher
SM count and FP4 tensor core availability.

### Vision (`llama-mtmd-cli`, Qwen2-VL + mmproj-f16, single image)

Input image: a 1288×1288 JPEG of a tabby cat (Wikipedia commons).
Prompt: `"Describe this image in one sentence."`.

| Phase               | Value                                              |
| ------------------- | -------------------------------------------------- |
| mmproj load         | 1 289.95 MiB on CUDA                               |
| Image slice encode  | 733 ms                                             |
| Image decode batch 1 | 148 ms (2 048 tokens)                             |
| Image decode batch 2 | 143 ms (1 967 tokens)                             |
| Prompt eval         | 3 186.26 t/s across 4 032 tokens (1 265 ms)        |
| Text generation     | 200.96 t/s across 28 tokens (139 ms)               |
| Total end-to-end    | 2 595 ms (image + prompt + 28 tokens of response)  |
| Wall clock incl load | 8.594 s                                           |

Model output for the cat photo:

> A tabby cat with green eyes and a striped coat is sitting on a ledge with a blurred background of bare branches and a blue sky.

`graphs_reused=27` — kernel cache warmed inside the run. Flash
attention enabled. Vision-conditioned generation (201 t/s) is within
6 % of text-only generation (215 t/s), so the mmproj +
cross-attention path is not bottlenecking gen on Blackwell.

## The actual forge gap

Update 2026-05-11: the first Omni bench closed the "no single local model"
question for the Blackwell full tier. `ggml-org/Qwen2.5-Omni-7B-GGUF`
Q4_K_M plus mmproj-f16 ran successfully through upstream llama.cpp `1ec7ba0`
on RTX 5090 sm_120 with CUDA 12.8. Text bench reached pp512 13,659 t/s and
tg128 220 t/s; the vision smoke described the cat image correctly at 212 t/s
generation; the audio smoke transcribed the JFK WAV correctly at 216 t/s
generation. This makes Qwen2.5-Omni-7B the recommended full-tier sensory-input
candidate for RTX/Blackwell while Qwen3-Omni-30B-A3B remains the next MoE
candidate to bench.

That result also surfaced the next real kernel gap: upstream llama.cpp reports
CUDA `POOL_1D` unsupported in the CLIP/mmproj graph, so that operator falls
back from CUDA to CPU. Decode remains CUDA/full-offload, and performance is
still usable, but Continuum should treat this as a VDD failure to eliminate,
not an accepted architecture. Position 3 follow-up should either patch the
CUDA `POOL_1D` kernel upstream or keep the candidate marked with an explicit
`mmproj_pool_1d_cpu_fallback` warning in the Rust registry.

The headline `#1072` alpha-bar miss is **not** Qwen 3.5/3.6-VL upstream
availability — though that is real (only three files in vendored
`llama.cpp` mention `qwen3_vl`: `test-backend-ops.cpp`,
`convert_hf_to_gguf.py`, `clip-model.h`; and `bartowski/Qwen2.5-VL-7B-Instruct-GGUF`
returns "Invalid username or password" against an anonymous fetch).

The original headline gap was that **no single local model in `models.toml` has
all four `standard_persona` capabilities** `{Chat, Vision, AudioInput, AudioOutput}`:

| Model entry                          | Chat | Vision | AudioIn | AudioOut |
| ------------------------------------ | :--: | :----: | :-----: | :------: |
| qwen2-vl-7b-instruct                 |  ✓   |   ✓    |    —    |    —     |
| qwen2-audio-7b-instruct *(disabled)* |  ✓   |   —    |    ✓    |    —     |

`qwen2-audio-7b-instruct` is commented out at
`src/workers/continuum-core/config/models.toml` line 309+ — disabled
2026-04-22 because registering both `qwen2-vl-7b` and `qwen2-audio-7b`
at boot spawned a second `LlamaCppAdapter` whose eager
`initialize()` pushed Apple Metal over `kIOGPUCommandBufferCallback​ErrorOutOfMemory`.
That OOM is a Mac/Metal constraint at 8–16 GB unified memory; on RTX
5090 (32 GB VRAM) both adapters fit with substantial headroom (each
model ≈ 5 GB + KV).

This is why `cognition::model_resolver::tests::current_registry_state_fails_alpha_bar_naming_the_forge_gap`
ships as a passing test that *asserts* the failure: the resolver fires
`NoMultimodalBase` on every host because no entry in the registry has
the full sensory bundle.

The 2026-05-11 Omni bench changes the next action: the hardware/runtime path is
viable, but `models.toml` and the Rust registry still need a vetted
Qwen2.5-Omni row before the resolver can select it. The candidate should be
admitted for `{Chat, Vision, AudioInput}` first, with a separate typed
voice-output adapter or forge task for `AudioOutput`.

## Three paths forward

1. **Admit Qwen2.5-Omni-7B as the first full-tier sensory-input GGUF.**
   The ggml-org Qwen2.5-Omni-7B GGUF path is verified on RTX 5090 for
   text/image/audio input. This is now the immediate Rust registry work:
   add a candidate row with hardware tier, artifact paths, measured VDD,
   and an explicit `mmproj_pool_1d_cpu_fallback` warning until the CUDA
   kernel gap is fixed.

2. **Tier-aware load policy that re-enables `qwen2-audio-7b-instruct`
   when memory budget allows.** Adapter-side substrate work: skip on
   Mac 8/16 GB, enable on RTX 5090 32 GB, M3 Max 64 GB, etc. Uses
   `HostCapability.available_memory_mb` from
   [`PR #1075`](https://github.com/CambrianTech/continuum/pull/1075).

3. **Multi-model virtual `StandardPersona`.** Extend Codex's
   `RequirementProfile` shape from [`PR #1074`](https://github.com/CambrianTech/continuum/pull/1074)
   so that `resolve_model` returns a per-capability dispatch table
   (`{vision_model, audio_model, text_model}`) instead of a single
   `ResolvedModel`. The persona runtime then routes each modality
   to its specialist backend. RTX 5090 32 GB holds three 7 B
   Q4_K_M models simultaneously without paging; smaller tiers fall
   back to a tiered subset behind the existing dispatch.

Path 3 maps cleanest to the Rust-first runtime substrate codified in
[`#1070`](https://github.com/CambrianTech/continuum/pull/1070) and the
`adaptive_throughput` planner + `FootprintRegistry` leases from
[`#1062–#1065`](https://github.com/CambrianTech/continuum/pull/1065):
each modality is a typed lane with its own `TargetSilicon` budget,
admission and revocation already covered by the substrate.

## What this PR does (and what it doesn't)

- **Adds** `scripts/bench-blackwell-vl.sh` — reproducer for this tier
  and a template for other tiers (`CUDA_ARCH=native` for auto-detect;
  works on Ampere/Ada/Hopper as well).
- **Adds** this document with the measured numbers.
- **Does not** change `models.toml` (no row-add or row-edit) — the
  Qwen2-VL row is already present; the audio row is already disabled.
- **Does not** alter the resolver or adapter — Path 3 above is a
  follow-up that crosses Position 1 and Position 3 ownership and
  needs Codex's input on the `RequirementProfile` shape change.
- **Does not** unblock `current_registry_state_fails_alpha_bar_naming_the_forge_gap`
  — that test goes green only when a sensory-complete entry lands in
  the registry. This PR establishes the per-tier perf baseline that
  proves the Blackwell side is ready to host one once forged.

## Other tiers — to-do

| Tier              | Expected      | Status                                |
| ----------------- | ------------- | ------------------------------------- |
| RTX 5090 / sm_120 | tg ≥ 150 t/s  | ✓ measured: 215 t/s text, 201 t/s vision |
| RTX 4090 / sm_89  | tg ≥ 120 t/s  | not yet measured                      |
| H100 / sm_90      | tg ≥ 200 t/s  | not yet measured                      |
| A100 / sm_80      | tg ≥ 80 t/s   | not yet measured                      |
| T4  / sm_75       | tg ≥ 25 t/s   | not yet measured                      |
| M3 Max / Metal    | tg ≥ 50 t/s   | not yet measured                      |

`scripts/bench-blackwell-vl.sh` works on any of these — `CUDA_ARCH=native`
auto-detects, and for Apple Metal the equivalent harness uses
`-DGGML_METAL=ON` (separate script, follow-up).

## Known reproduction notes

- Docker Desktop on Windows WSL2 cannot bind-mount `/tmp/*` or
  `/home/user/*` paths from non-`docker-desktop` distros into
  containers; the script uses a named volume `qwen-vl-bench-work`
  instead.
- Vulkan parity testing is currently blocked on this host: the
  NVCT graphics slice in WSL2 Docker Desktop doesn't expose Vulkan
  to containers. A direct Windows host build of llama.cpp + Vulkan
  is the workaround if a Vulkan parity number is needed.
- HF anonymous fetches for `bartowski/Qwen2.5-VL-7B-Instruct-GGUF`
  returned an auth error during this run. The Qwen2-VL repo
  (`bartowski/Qwen2-VL-7B-Instruct-GGUF`) is anonymous-fetchable.
