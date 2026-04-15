# Acceleration Architecture

**Status:** committed (2026-04-15). Enforced by PR891 and successors.

## The non-negotiable

Every Continuum user runs inference, rendering, STT/TTS, and WebRTC media on the GPU native to their machine. **There is no CPU-fallback form.** The only tolerated exception is the Ares bootloader mini-persona that runs on constrained bootstrap hardware before the full stack is online — and even that is a temporary self-upgrading mode, not a shipped product state.

This applies to **both audiences**:

- **Carl** — the curl-install user. Runs `curl install.sh | bash` and expects Qwen3.5-4B to stream tokens as fast as his hardware can push them. He never sees a compile, never picks a backend, never ends up on CPU by accident.
- **Dev** — the git-clone user. Iterates on code, runs `npm start` / `cargo run`. Same acceleration contract: whatever backend the machine can do natively, the dev build uses. Different install mechanism, identical inference performance.

(See `MEMORY.md` → `feedback_carl_and_dev_paradigm.md` for the audience definition; `feedback_support_all_features_no_cheating.md` for the "never cheat" directive.)

## The machine × audience matrix

|  | **Carl (curl)** | **Dev (git clone)** |
|---|---|---|
| **Mac (M1–M5)** | Podman + krunkit + `continuum-core-vulkan` container → Vulkan → MoltenVK → Metal. One command, ~60s to widget open. | `cargo run --features=metal` → native Metal. Zero container overhead, peak throughput. Support services (postgres, widget, livekit-bridge) optionally via Podman or Docker. |
| **BigMama (RTX 5090 / WSL2)** | Docker + `continuum-core-cuda` container → `runtime: nvidia` binds the 5090. `curl install.sh` handles Docker install + compose. | `cargo run --features=cuda` in WSL2 → native nvcc. Support services via Docker. |
| **Generic Linux + GPU** | Docker + `continuum-core-cuda` (Nvidia) or `continuum-core-vulkan` (AMD/Intel) depending on detected GPU. | `cargo run --features=cuda` or `--features=vulkan` based on what's installed. |
| **Generic Linux / no GPU** | No accelerated path exists → refuse to install with a clear "this machine has no GPU Continuum can use" message. CPU fallback is banned. | Same: build refuses unless `--allow-cpu-only` (Ares bootloader exception flag). |

The exception row is hard: we explicitly refuse to ship a silently-degraded experience. A user without a supported GPU gets a clear refusal, not a working-but-slow install.

## Building — per cell

The premise (PR891): CI is not the primary builder. Dev machines are ~10× faster; they own assembly, CI validates.

**Build command (any cell):** `scripts/push-image.sh <variant>` — runs 4 phases:

1. **Phase 0** — native `cargo test -p llama --features=<backend>` against the host's natural backend. Catches Rust regressions in seconds.
2. **Phase 1** — local `docker buildx --load` (single native arch). Catches Dockerfile regressions in minutes.
3. **Phase 2** — `scripts/test-slices.sh` against the loaded image. Proves boot + device visibility + runtime linkage.
4. **Phase 3** — multi-arch `docker buildx --push` to ghcr.io. Only runs when Phases 0–2 passed.

**Who builds what natively:**

| Variant | Fastest host | Why |
|---|---|---|
| `continuum-core-cuda` | BigMama (WSL2, RTX 5090) | Native nvcc, no qemu, real CUDA driver. ~20m vs ~1h42m on GHA. |
| `continuum-core-vulkan` | Any M-series Mac OR BigMama | Cross-platform: glslc compiles SPIR-V anywhere. Mac native arm64 build skips qemu; BigMama has Nvidia Vulkan ICD + shaderc. Whoever has slack. |
| `continuum-core` (CPU-only, Ares-bootloader-exception) | Any host | No GPU toolchain required. |
| Native Mac binary with Metal | Mac only | `metal` shader compiler is Apple-Clang-only, Xcode-only. No cross-compile. |

**Future airc broadcast:** each mesh peer advertises its toolchain capability flags; a build request broadcasts to the mesh, the peer with matching capability + lowest load answers. Not implemented yet; current flow is manually choosing the right peer.

## Inference — runtime path per cell

The primary path is always "accelerated GPU kernels in-process (native) or in-container via hypervisor GPU passthrough."

### Primary paths (what we're shipping for PR891)

**Mac Dev:** Native binary → `workers/llama` crate → ggml-metal kernels → Metal → GPU. Measured: M1 Pro 26–29 tok/s solo, M5 43–51 tok/s solo, 73 tok/s aggregate 3-stream continuous-batching.

**Mac Carl:** Container via Podman+krunkit → `workers/llama` crate → ggml-vulkan kernels → Vulkan API → krunkit VMM → MoltenVK on host → Metal → GPU. Expected: ~80% of native Metal (reference Phi-3 on M2 Max: 63 vs 78 tok/s in the llama.cpp reference benchmark).

**BigMama Dev:** Native WSL2 binary → `workers/llama` crate → ggml-cuda kernels → CUDA → 5090. Expected: well above Mac baselines; measured in PR891 validation.

**BigMama Carl:** `continuum-core-cuda` container → same binary → `runtime: nvidia` binds 5090 → CUDA → 5090. Expected parity with BigMama Dev (container overhead ≈ 0 on Linux hosts).

### The fallback — reusable UDP sidecar

If the Vulkan-in-container path can't hit the bar (measured, not assumed), we do **not** fall back to CPU. We tunnel out of the container to a host-native binary over UDP.

**The fallback is itself accelerated.** No degradation. The only thing that changes is the IPC boundary — the kernels still run on the GPU.

**Architecture:** one UDP protocol on loopback, N sidecar binaries, reused across subsystems (see `MEMORY.md` → `feedback_udp_sidecar_fallback.md`).

| Sidecar (host-native binary) | Accelerates | Triggers fallback when |
|---|---|---|
| `continuum-inference-host` | llama.cpp + Qwen3.5 + LoRA hot-swap on native Metal/CUDA | Vulkan-in-container is below 60% of native perf, or krunkit/Podman deployment is infeasible on the user's system |
| `continuum-livekit-host` | Native WebRTC + hw-accel encode/decode (VideoToolbox on Mac / NVENC on BigMama) | WebRTC-in-container can't negotiate codec parameters through krunkit, or latency exceeds 50ms |
| `continuum-bevy-host` | Native Metal/MoltenVK render loop for avatar live-call frames | Bevy-in-container framerate drops below 60fps under load |

**Why UDP on loopback, not Unix socket or TCP:**

- No head-of-line blocking — token streams and audio frames can drop one packet without freezing everything behind it.
- Same protocol family as WebRTC so LiveKit reuses the stack on the wire format side.
- Near-zero loopback latency — virtio-net through krunkit on Mac is sub-ms; `--network host` on Linux is actual host loopback.
- Per-message reliability tier (fire-and-forget / ack-required / ordered-stream) like RTP. Control plane gets acks, hot path doesn't pay for them.

**Reliability:**

- Discovery over a Unix socket (control plane). Each session gets an assigned UDP port for data.
- Schemas in the existing `continuum-bridge-protocol` crate — do not fork a new protocol crate per subsystem.
- Sidecar lifecycle managed by the host install: installed alongside Podman+krunkit on Mac Carl; alongside native WSL2 binary on BigMama Dev.

**What this means for Carl and Dev:**

- **Carl (Mac)** — Podman pulls `continuum-core-vulkan` AND brew installs `continuum-inference-host` (and other sidecars as their subsystems require). Primary is Vulkan-in-container; if that underperforms, the container routes inference UDP to the host sidecar. User sees "Qwen3.5 is fast" either way.
- **Dev (Mac)** — same sidecars available. Dev can choose native binary (peak Metal, no container) OR container+Vulkan OR container+UDP-sidecar. All three are accelerated.
- **Carl (BigMama)** — Docker pulls `continuum-core-cuda`. Sidecars not needed (CUDA passthrough works natively in Docker via `runtime: nvidia`). UDP sidecar pattern available for LiveKit/Bevy if those subsystems need VideoToolbox/NVENC optimizations the container can't do.
- **Dev (BigMama)** — native WSL2 binary OR Docker. Same sidecar story.

## What this architecture forbids

- **CPU fallback as a shipped path.** Ares bootloader is the exception; it's time-limited and self-upgrades.
- **Silent degradation.** If a subsystem can't run accelerated on a user's hardware, the install refuses with a clear message. Carl does not get a broken-feeling "it works but it's slow" experience and not know why.
- **Per-subsystem bespoke IPC.** If any subsystem needs a host sidecar, it uses the UDP protocol family. No new socket abstractions per feature.
- **Inference running TCP.** Never. UDP is solved for low-latency streaming; TCP's HOL blocking is a perf regression for token streams.
- **Docker Desktop on Mac as the Carl runtime.** It has no GPU passthrough — using it for continuum-core would force CPU inference. Carl gets Podman+krunkit.
- **Docker image variants that silently run CPU.** Any published `continuum-core-*` image must link a GPU-accelerated ggml backend (metal/cuda/vulkan). The CPU-only `continuum-core:latest` exists ONLY for the Ares bootloader exception and should not be referenced by any Carl or Dev install path.

## Implementation status (2026-04-15)

- ✅ `continuum-core-cuda` image — CI green (commit `90908b4`)
- 🟡 `continuum-core-vulkan` image — CI in flight (commit `2159ebd93`), wall 1 (missing `glslc`) punched, building
- ✅ `scripts/push-image.sh` — dev-side build-test-push with Phase 0–3 (commit `5c3d4bcbb`)
- ✅ `scripts/test-slices.sh` — slice-test harness, per-variant probes (commit `42ddc7308`)
- ✅ `docker-compose.mac.yml` — Mac Carl compose override (commit `92f42a847`)
- ✅ Mac Carl install.sh — Podman + krunkit bootstrap (commit `92f42a847`)
- ⏳ UDP sidecar protocol — contingent on Vulkan measurement; design spec in this doc + `feedback_udp_sidecar_fallback.md`, implementation pending
- ⏳ Self-hosted CI runner on BigMama for cuda slice tests — Phase 3d of `RESTORE-FULL-PARITY-PLAN.md`

## References

- `feedback_carl_and_dev_paradigm.md` — the two user archetypes
- `feedback_inference_runtime_split.md` — llama.cpp for inference, Candle for training, Qwen3.5-4B Q4_K_M as the LCD workhorse
- `feedback_support_all_features_no_cheating.md` — no degradation, UDP is solved for low-latency video
- `feedback_udp_sidecar_fallback.md` — the UDP sidecar pattern
- `feedback_docker_fast_ephemeral.md` — Carl's 60-second budget
- `docs/infrastructure/INSTALL-ARCHITECTURE.md` — the one install script / module shape contract
- `docs/infrastructure/RESTORE-FULL-PARITY-PLAN.md` — sensory capability restoration plan
