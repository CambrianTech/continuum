# Sensory Model And Experiential Plasticity Plan

**Status**: active alpha plan
**Updated**: 2026-05-11
**Owner split**: Codex/Mac owns literature and candidate metadata; Windows/RTX
owns empirical build, forge, CUDA/Vulkan VDD.
**Parent**: [Alpha Gap Analysis](../planning/ALPHA-GAP-ANALYSIS.md)
**Related**: [Persona-as-Rust-Library](PERSONA-AS-RUST-LIBRARY-PLAN.md),
[Restore Full Sensory Parity](../infrastructure/RESTORE-FULL-PARITY-PLAN.md),
[Genome Architecture](../genome/GENOME-ARCHITECTURE.md)

## Thesis

Continuum personas are sensory entities, not text bots. The standard local
persona contract requires text, vision/image/video perception, audio input,
voice/audio output, avatar/control output, WebRTC presence, and traceable
runtime behavior. The model layer must therefore select or forge models by
capability and hardware budget, not by scattered hardcoded model names.

The target architecture is:

```text
Persona sensory requirement
  -> Rust ModelRequirement
  -> Rust registry/admission resolver
  -> vetted model artifact or forge task
  -> llama.cpp local runtime path
  -> VDD timing/resource report
  -> canary promotion
```

No runtime code should know a specific model name because a persona wants
sensory cognition. Runtime code asks for capabilities, context, intelligence,
license/runtime constraints, and hardware budgets. The registry resolves the
best vetted artifact on the current machine.

## Current Public Model Read

This section is a candidate scout, not the runtime source of truth. Runtime
truth belongs in the Rust registry once artifacts are validated.

### Qwen2.5-Omni-7B

- **Source**: [Qwen/Qwen2.5-Omni-7B](https://huggingface.co/Qwen/Qwen2.5-Omni-7B)
- **GGUF**: [ggml-org/Qwen2.5-Omni-7B-GGUF](https://huggingface.co/ggml-org/Qwen2.5-Omni-7B-GGUF)
- **Current read**: official end-to-end omni model with a working ggml-org
  GGUF path for local text, image, and audio input through upstream llama.cpp.
  RTX 5090 VDD on 2026-05-11 validated Q4_K_M plus mmproj-f16 on CUDA sm_120:
  text bench, image description, and audio transcription all passed.
- **Measured RTX 5090 result**: upstream llama.cpp `1ec7ba0`,
  `-DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=120-real`,
  `Qwen2.5-Omni-7B-Q4_K_M.gguf` 4.36 GiB plus `mmproj` 2.5 GiB. Text bench
  `-ngl 99 -p 512 -n 128 -r 3`: pp512 13,659 t/s, tg128 220 t/s. Vision
  smoke: 1,288 px cat image described correctly, text generation 212 t/s.
  Audio smoke: JFK WAV transcribed correctly, text generation 216 t/s.
- **Known kernel gap**: upstream llama.cpp reported CUDA `POOL_1D` unsupported
  inside the CLIP/mmproj graph, so that operator falls back from CUDA to CPU.
  Decode stayed on CUDA; the fallback is still a VDD failure to track and fix,
  not an acceptable steady-state architecture. Upstream tracking referenced by
  RTX VDD: ggml-org/llama.cpp PR 16837, comment 3461676118.
- **Alpha role**: recommended full-tier local sensory-input candidate for
  Blackwell/RTX-class hosts now. It closes text/image/audio input locally and
  is fast enough to restore real persona perception. It still does not close
  speech output unless llama.cpp support grows, we pair a typed voice-output
  adapter, or we forge the missing output path.
- **Registry action**: add as the first vetted full-tier candidate with a
  `requiresAccelerator=true` profile and a `mmproj_pool_1d_cpu_fallback`
  warning until the upstream kernel is fixed. Mac Metal still requires its own
  VDD because this result is CUDA/Blackwell-specific.

### Qwen2.5-Omni-3B

- **GGUF**: [ggml-org/Qwen2.5-Omni-3B-GGUF](https://huggingface.co/ggml-org/Qwen2.5-Omni-3B-GGUF)
- **Current read**: smaller Qwen2.5-Omni GGUF candidate for low-memory hosts.
  Needs confirmation that llama.cpp support covers the same sensory path as 7B.
- **Alpha role**: MBA/low-memory sensory candidate if it passes audio/vision
  VDD.
- **Registry action**: bench after 7B. If audio output is transformers-only or
  incomplete in llama.cpp, treat as compatibility candidate, not alpha sensory
  default.

### Qwen3-Omni-30B-A3B-Instruct

- **Source**: [Qwen/Qwen3-Omni-30B-A3B-Instruct](https://huggingface.co/Qwen/Qwen3-Omni-30B-A3B-Instruct)
- **GGUF**: [ggml-org/Qwen3-Omni-30B-A3B-Instruct-GGUF](https://huggingface.co/ggml-org/Qwen3-Omni-30B-A3B-Instruct-GGUF)
- **Current read**: official Qwen3-Omni Any-to-Any MoE model. HF marks the
  source model `text-to-audio`, `multimodal`, and `Any-to-Any`. The ggml-org
  GGUF mirror has llama.cpp `-hf` examples.
- **Alpha role**: Blackwell/5090 sensory flagship and future distributed/grid
  target. This is the best current candidate for the complete sensory contract
  if audio output works in local runtime. MoE makes it the best pruning/paging
  target if VDD is viable.
- **Registry action**: bench after Qwen2.5-Omni-7B input path. Validate
  30B/3B-active behavior, speech output, context, VRAM, and whether MoE expert
  paging/pruning can make it practical.

### Qwen3.6-27B

- **Source**: [Qwen/Qwen3.6-27B](https://huggingface.co/Qwen/Qwen3.6-27B)
- **Current read**: official open-weight Qwen3.6 model. HF marks it
  `Image-Text-to-Text`; model card says causal LM with vision encoder, 262K
  native context, vLLM/SGLang/KTransformers support, and explicit image-input
  examples.
- **Alpha role**: high-end dense sensory reasoning target for 5090/3090-class
  hosts if quantized runtime is viable.
- **Registry action**: Windows/RTX must validate CUDA/Vulkan llama.cpp or other
  local adapter path, quant size, projector handling, first-token, tok/s, CPU%,
  GPU%, and VRAM.

### Qwen3.6-35B-A3B

- **Source**: [Qwen/Qwen3.6-35B-A3B](https://huggingface.co/Qwen/Qwen3.6-35B-A3B)
- **GGUF probe**: [bartowski/Qwen_Qwen3.6-35B-A3B-GGUF](https://huggingface.co/bartowski/Qwen_Qwen3.6-35B-A3B-GGUF)
- **Current read**: official open-weight Qwen3.6 sparse MoE/VLM. HF marks it
  `Image-Text-to-Text`; card says 35B total / 3B active and causal LM with
  vision encoder. The community GGUF has Q4_K_M around 21.39GB.
- **Alpha role**: prime MoE pruning/paging target: high capability surface with
  only part of the model active per token.
- **Registry action**: validate the GGUF first, then decide whether to forge
  official Continuum quants with embedded chat template and measured hardware
  profiles.

### Qwen3.5 VLMs

- **Source**: [Qwen/Qwen3.5-9B](https://huggingface.co/Qwen/Qwen3.5-9B)
- **Current read**: official Qwen3.5 models are `Image-Text-to-Text`; model
  card says unified vision-language foundation and causal LM with vision
  encoder.
- **Alpha role**: current mid/full host VLM target if Qwen3.6 is too heavy or
  less stable.
- **Registry action**: existing Continuum forged 4B/code artifacts should be
  rechecked against official Qwen3.5 VLM behavior, projector needs, and
  prompt/template metadata.

### Qwen3.5-Omni

- **Source**: [paper](https://huggingface.co/papers/2604.15804)
- **Current read**: public reports describe text/audio/image/video native omni
  behavior, hundreds of billions of parameters, 256K context, and audio-visual
  capabilities. Official downloadable weights were not confirmed in this pass.
- **Alpha role**: watch item and API/closed-source comparison target.
- **Registry action**: do not add runtime row until exact downloadable artifact
  and license are verified.

### Existing Qwen2-VL Baseline

- **Source**: `Qwen/Qwen2-VL-7B-Instruct-GGUF`
- **Current read**: already in `src/shared/models.json` with GGUF plus mmproj.
- **Alpha role**: known working vision baseline and regression fixture.
- **Registry action**: keep as baseline until Qwen3.5/3.6/Omni artifacts beat
  it in VDD.

Current ranking from AIRC/RTX scout and 2026-05-11 RTX VDD:

1. `Qwen2.5-Omni-7B` official source plus `ggml-org` GGUF is the first full-tier
   local sensory-input candidate. RTX 5090 VDD proved text, image, and audio
   input with high throughput. It still needs speech-output validation or
   forge/voice-adapter work, and the CUDA `POOL_1D` mmproj fallback must be
   tracked as an upstream kernel gap.
2. `Qwen3-Omni-30B-A3B-Instruct` plus `ggml-org` GGUF is the high-end
   Blackwell/grid candidate, the likely complete sensory contract candidate,
   and the best MoE pruning/paging target.
3. `Qwen3.6-27B` and `Qwen3.6-35B-A3B` are valuable VLM/intelligence targets
   but do not satisfy the full audio sensory contract alone. They need a paired
   audio model or a forged Continuum sensory variant.

## Forge-First Policy

If the right sensory model does not exist in a clean, runnable, license-valid
artifact, Continuum forges it. Missing GGUF, missing projector, missing audio
layer, missing chat template, bad quant, bad kernel, or poor packaging is a
foundry task, not an excuse to hardcode a weaker runtime path.

This does not block getting a working model online. The alpha sequence is:

1. admit the best already-working open model through the Rust registry;
2. validate it with TDD/VDD on real hardware;
3. keep the runtime capability-based so it can be replaced without code churn;
4. forge, prune, defrag, quantize, and upstream the Continuum-optimized version;
5. promote the forged model only when it beats the baseline on replay quality
   and resource metrics.

Working first and forging better second is different from accepting a fallback.
The first working model is a measured baseline and service-restoration step.
The forged model is the planned optimization path.

Every forge, pruning, defrag, quantization, or kernel optimization pass must
re-prove the full declared modality set. It is easy to optimize away video,
image, audio-in, audio-out, or projector paths by accident. That is a failed
candidate, even if text quality, size, or tokens/sec improved.

The forge loop is:

```text
select official/open base
  -> add or preserve required modality encoders/projectors
  -> repair llama.cpp/GGUF/runtime support where needed
  -> quantize for target hardware tiers
  -> embed template/license/manifest metadata
  -> publish under continuum-ai or approved registry
  -> run TDD/VDD replay gates
  -> admit through Rust registry
```

For Qwen3.5/3.6 this means we can produce Continuum-owned sensory variants:

- `qwen3.6-35b-a3b-sensory-forged`: MoE/VLM target with measured expert
  pruning and GPU profiles.
- `qwen3.6-27b-sensory-forged`: dense high-quality sensory target.
- `qwen2.5-omni-7b-continuum-gguf`: consumer full-sensory target if existing
  community artifacts fail license/runtime gates.
- `qwen3-omni-30b-a3b-blackwell-forged`: 5090/grid flagship if VDD shows it
  can be made practical.

## Experiential Plasticity

Continuum should treat model selection as the starting point, not the end state.
The `continuum-ai/experiential-plasticity-paper` card already states the core
method: entropy-based pruning plus domain retraining can produce smaller
models that improve on the target domain. Reported examples include Qwen3.5-4B
improving on code and Qwen3.5-27B compressing substantially while improving on
the target task. Source:
[continuum-ai/experiential-plasticity-paper](https://huggingface.co/continuum-ai/experiential-plasticity-paper)

In Continuum terms, experiential plasticity is the model foundry loop:

```text
capture real persona experience
  -> score/replay/label by domain and modality
  -> prune low-value weights/heads/experts
  -> train or distill on the captured domain
  -> defrag the resulting structure
  -> quantize/package
  -> validate against replay and VDD
  -> admit as a new registry candidate
```

This applies to:

- dense model pruning: remove low-utility heads/blocks for the target domain;
- MoE pruning: remove or page cold experts, preserve hot experts, and measure
  active-parameter quality rather than total-parameter marketing size;
- modality pruning: keep every vision, video, audio-in, audio-out, projector,
  tokenizer, and bridge path required by the persona contract; remove only
  conversion paths that VDD proves are unused by that admitted profile;
- LoRA/genome pruning: compact adapters after repeated experiential training;
- KV/context policy: shorten or summarize context based on replay-proven value,
  not arbitrary token limits.

The important rule is that pruning is not "make it smaller and hope." Every
cycle must be replayed against captured persona fixtures and measured against
hardware telemetry. If it gets smaller but loses sensory accuracy, tool
correctness, or persona responsiveness, it is not admitted.

## Hardware Targeting

The resolver must select by capability and pressure:

| Host class | Backend target |
| --- | --- |
| Mac M-series | Metal + unified memory |
| NVIDIA 3090/4090/5090 | CUDA first, Vulkan secondary |
| AMD/Intel | Vulkan |
| Low-memory hosts | GPU path if present; otherwise explicit degraded state |
| Grid | Capability routing across machines |

Default posture:

- Mac M-series: prefer smaller Qwen3.5/3.6 VLM or Qwen2.5-Omni quants with
  strict memory admission. Use unified memory pressure to gate context and
  concurrent personas.
- NVIDIA 3090/4090/5090: validate Qwen3.6-27B, Qwen3.6-35B-A3B, and
  Qwen2.5/Qwen3 Omni. Highest priority for forge/alloy, MoE pruning, and VDD
  timing.
- AMD/Intel: treat Vulkan as a first-class local backend once validated. No CPU
  happy path.
- Low-memory hosts: admit smaller sensory or compatibility models. If sensory
  cannot run, report `Unavailable`/`Degraded`, not fake success.
- Grid: send sensory jobs to the host with the right GPU/artifact/residency
  budget using command/grid contracts.

The registry/admission result should explain:

- selected model and artifact;
- rejected candidates and reasons;
- required files and whether they exist;
- GPU backend and layer/offload plan;
- estimated model, projector, audio, LoRA, KV, and scratch memory;
- whether the result is `Ready`, `NeedsDownload`, `NeedsForge`,
  `Backpressured`, `KernelGap`, `MissingArtifact`, `LicenseBlocked`, or
  `InsufficientMemory`.

## Windows/RTX Build Assignment

Windows/RTX owns empirical proof for this workstream. The deliverable is not
"looked at it"; it is a small VDD table per candidate:

| Field | Required |
| --- | --- |
| HF repo and exact revision | yes |
| Files pulled | yes |
| License | yes |
| Quant and size | yes |
| Backend | CUDA and Vulkan where possible |
| llama.cpp command or adapter path | yes |
| First token latency | yes |
| Decode tok/s | yes |
| CPU utilization | yes |
| GPU utilization | yes |
| VRAM and RSS | yes |
| Context length tested | yes |
| Vision fixture result | yes |
| Audio fixture result | yes for Omni/audio candidates |
| Missing kernel/projector/audio layer | yes, if any |
| Forge/alloy next step | yes, if not directly usable |

Initial Windows/RTX queue:

1. `Qwen/Qwen2.5-Omni-7B` official and `ggml-org` GGUF paths.
2. `Qwen/Qwen3-Omni-30B-A3B-Instruct` feasibility on 5090-class hardware.
3. `Qwen/Qwen3.6-27B` official + best available GGUF quant.
4. `bartowski/Qwen_Qwen3.6-35B-A3B-GGUF` as a fast MoE/VLM probe.
5. Existing `qwen2-vl-7b` as a baseline regression measurement.

## Rust Registry Requirements

The model registry needs typed vocabulary before any candidate becomes runtime
default:

- `ModelFamily`: `Qwen`, `ContinuumForged`, `Cloud`, etc.
- `Architecture`: dense, MoE, omni, VLM, audio, embedding, reranker.
- `Capability`: text, vision input, video input, audio input, audio output,
  tool/control, avatar/control, embedding, LoRA, MoE.
- `RuntimeBackend`: `LlamaCppLocal`, `CloudApi`, `ForgeTraining`,
  `GridRemote`, with hardware backend nested below it.
- `HardwareBackend`: `Metal`, `Cuda`, `Vulkan`, `Dmr`, `CpuDegraded`.
- `ArtifactKind`: base GGUF/safetensors, mmproj, audio projector, tokenizer,
  chat template, LoRA, adapter manifest, license, benchmark report.
- `AdmissionState`: `Ready`, `NeedsDownload`, `NeedsForge`, `Unavailable`,
  `Backpressured`, `KernelGap`, `LicenseBlocked`, `InsufficientMemory`.

Selection must be capability/range based:

```text
needs:
  family ~= qwen
  intelligence >= full
  context >= 64k
  input includes text,image,audio
  output includes text,audio
  backend in cuda|metal|vulkan
  memory <= host budget
  license in allowed set
```

The registry may prefer Qwen, but it should not hardcode one model as the
system truth. The current host and artifact state determine the admitted model.

## TDD And VDD Gates

TDD:

- Rust unit tests for capability/range selection.
- Missing artifact tests return `NeedsDownload` or `MissingArtifact`.
- Missing projector tests reject false vision/audio capability.
- License-blocked artifacts do not become defaults.
- No candidate may be admitted if its chat template is unknown or unembedded.
- No model row can use untyped provider/model strings in persona runtime paths.

VDD:

- `qwen2-vl-7b` baseline image fixture still works.
- Qwen3.5/3.6 VLM candidate passes image/OCR/document fixtures.
- Omni candidate passes text, image/OCR/document, short-video if declared,
  audio-in, and speech-out fixtures.
- Refined, forged, pruned, quantized, or kernel-optimized candidates rerun the
  same modality fixtures before replacing the previous baseline.
- Report first-token latency, tok/s, CPU%, GPU%, VRAM, RSS, context, and queue
  wait for every candidate.
- Run at least one replay-derived persona smoke: multiple messages consolidate
  into one turn and the response does not echo prompt/RAG garbage.
- CPU-only execution on GPU-capable hosts is a failing result unless the test is
  explicitly a degraded-mode test.

## PR Plan

1. `docs/sensory-experiential-plasticity`: this document and alpha-plan link.
2. `feature/rust-model-registry-candidates`: typed candidate metadata and
   ts-rs exports; no runtime default switch yet.
3. `feature/model-vdd-harness`: one Rust/CLI command emits the candidate VDD
   table from structured timing/resource data.
4. `feature/qwen36-vlm-admission`: admit Qwen3.6 VLM only after RTX/Mac
   evidence exists.
5. `feature/qwen-omni-admission`: admit Qwen2.5/Qwen3 Omni only after audio,
   vision, and runtime support are proven.
6. `feature/experiential-plasticity-foundry-loop`: capture -> prune/train ->
   defrag -> quantize -> validate -> registry candidate.

## Deletion Targets

- duplicate model/provider lists outside the Rust registry;
- stale compatibility/fallback code that silently picks another provider;
- runtime references to unsupported local providers;
- TS cognition model-routing logic;
- comments or tombstones for deleted model paths;
- candidate rows without evidence, license, or artifact ownership.
