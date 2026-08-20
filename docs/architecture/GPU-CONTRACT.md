# GPU Contract — zero-compromise GPU for every compute component

**Status:** spec (spine doc). Referenced by the Carl installer (card 54e0d729), the
Windows/GPU build, and the `carl-install-smoke` acceptance gate.

## The rule (non-negotiable)

**Every GPU-capable compute component runs on the GPU. There is NO silent CPU
fallback anywhere.** A component that cannot obtain its GPU backend **fails LOUD**
at startup — it does not degrade to CPU. CPU-only operation of any of these is a
defect, not a fallback.

This extends the existing substrate rule (`gpu/memory_manager.rs::detect_gpu`
panics when no GPU is present — `#964` / `#980` GPU-fallback audit). This doc
generalizes it from "the LLM" to **every** subsystem below.

## Component → backend → install must provision

| Component | GPU backend | Install must provision | CPU = |
|---|---|---|---|
| llama server (LLM inference) | llama.cpp **CUDA** (ggml-cuda) | CUDA toolkit (nvcc, cuBLAS) | red |
| training / genome forge | candle-**cuda** (Linux/Win) · MLX (Mac) | CUDA toolkit | red |
| inference (general) | candle-**cuda** | CUDA toolkit | red |
| STT (Whisper / Moonshine / Silero VAD) | **ORT CUDA EP** or whisper.cpp CUDA | ORT-CUDA provider + **cuDNN** | red |
| TTS (Piper / Kokoro / Orpheus) | **ORT CUDA EP** | ORT-CUDA + cuDNN | red |
| classifiers / YOLO / ML CNNs | **ORT CUDA EP** / candle-cuda | ORT-CUDA + cuDNN | red |
| bevy renderer (live persona) | **wgpu → D3D12 / Vulkan** | Vulkan/D3D12 runtime (GPU driver) | red |
| livekit / webrtc | **NVENC / NVDEC** hardware codec | GPU driver codec + libwebrtc | red |

"CPU = red" means: if that component initializes on CPU, the install acceptance
gate fails and the persona substrate refuses to serve.

## Build implication (Windows/GPU lane)

No-compromise means the binary keeps **livekit + bevy** AND adds **CUDA
everywhere** — i.e. FULL cargo features + `cuda,load-dynamic-ort`, never
`--no-default-features` (that drops livekit off the GPU, a compromise).

livekit links a **prebuilt `webrtc.lib` built `/MT`** (static CRT — Google/LiveKit
ship it that way; it cannot be recompiled `/MD` short of building libwebrtc from
source). Therefore **the entire native Windows build is `/MT` (`crt-static`)** so
every from-source dep (Rust, candle, llama.cpp, basis-universal, esaxx, ort shim)
matches libwebrtc. This is the coherent fix — libwebrtc's `/MT` stops being the
outlier. The earlier `/MD` pins were only valid for the (rejected) livekit-dropped
build.

- `core/llama/build.rs` → `CMAKE_MSVC_RUNTIME_LIBRARY` honors `crt-static`
  (`MultiThreaded` when set, `MultiThreadedDLL` otherwise).
- `.cargo/config.toml` → `+crt-static` for `x86_64-pc-windows-msvc`; the esaxx
  `/MD` override is removed (esaxx's own `static_crt(true)` is correct under `/MT`).
- cc-built deps (basis, esaxx, ring, sqlite, …) honor `crt-static` automatically.
- MKL (if pulled) is the one prebuilt to watch — a `/MD` static MKL collides with
  `/MT`; a CUDA build should not need candle's `mkl` (CPU BLAS) feature.

## Per-subsystem acceptance gate (carl-install-smoke)

The install is not "done" until each subsystem is verified **on GPU**. Any CPU
path turns the gate red:

- **LLM**: llama offload layers on-device; tok/s above the native floor (not the
  ~10 tok/s CPU signature).
- **STT / TTS**: ORT session created with the CUDA execution provider (assert the
  provider list contains `CUDAExecutionProvider`, not just `CPUExecutionProvider`).
- **YOLO / CNN**: one inference pass on the CUDA EP.
- **bevy**: wgpu adapter is a hardware device (D3D12/Vulkan), fps above floor — not
  the `llvmpipe` software adapter.
- **livekit**: an NVENC encode succeeds (hardware encoder, not the software x264
  path).

## Install ergonomics (Joel's addendum — extends elegance to installs)

Two hard properties the installer MUST have:

1. **Idempotent repeat = the universal repair action.** Re-running the installer
   is ALWAYS safe and is how you fix a machine: it converges to healthy — skips
   what's already correct, reinstalls/upgrades only what's missing or drifted, and
   **never makes things worse**. "It's broken" → "run the installer again" is the
   whole support playbook. (This is the `module_skip` / `module_fail` contract in
   `tools/scripts/lib/install-common.sh` — guard first, act only on drift.)

2. **The installer IS the update/repair mechanism — one verb.** There is exactly
   ONE user action for everything post-install: **re-run the installer** (rustup
   pattern, identical on Mac and Windows). The product detects its own staleness
   and tells the user to re-run the installer; the installer then converges
   everything — self-update, repair, GPU (re)provision, config — in one pass.
   There is no separate `update` or `repair` command to learn: update = repair =
   provision = "run the installer again." No manual dep-chasing, no Docker on the
   compute hot path; native binaries updated in place.

Both properties are GPU-aware: every installer run (first install, update, or
repair) re-verifies the per-subsystem GPU gate above, so drifting a component onto
CPU is caught and repaired on the next run, never silently tolerated.
