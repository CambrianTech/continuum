# Acceleration Architecture

**Status:** committed (2026-04-15, pivoted 2026-04-16). Enforced by PR891 and successors.

## The non-negotiable

Every Continuum user runs inference, rendering, STT/TTS, and WebRTC media on the GPU native to their machine. **There is no CPU-fallback form.** The only tolerated exception is the Ares bootloader mini-persona that runs on constrained bootstrap hardware before the full stack is online — and even that is a temporary self-upgrading mode, not a shipped product state.

This applies to **both audiences**:

- **Carl** — the curl-install user. Runs `curl install.sh | bash` and expects Qwen3.5-4B to stream tokens as fast as his hardware can push them. He never sees a compile, never picks a backend, never ends up on CPU by accident.
- **Dev** — the git-clone user. Iterates on code, runs `npm start` / `cargo run`. Same acceleration contract: whatever backend the machine can do natively, the dev build uses. Different install mechanism, identical inference performance.

(See `MEMORY.md` → `feedback_carl_and_dev_paradigm.md` for the audience definition; `feedback_support_all_features_no_cheating.md` for the "never cheat" directive; `feedback_no_emulation_at_inference.md` for why passthrough is OK but emulation is banned; `feedback_docker_model_runner_mac.md` for the Mac Docker Model Runner pivot.)

## The machine × audience matrix

|  | **Carl (curl)** | **Dev (git clone)** |
|---|---|---|
| **Mac (M1–M5)** | Docker Desktop + Docker Model Runner (vllm-metal) for LLM + native `continuum-core-server` (Metal for Candle/Bevy/vision/audio) + containerized support services. One `curl install.sh \| bash` command, Carl's only manual steps are the two standard Docker Desktop onboarding clicks. | Same as Carl but driven via `npm start` — native `cargo --features=metal`, Docker Desktop support services, Docker Model Runner for LLM. Identical runtime shape, Dev just skips the curl step and iterates on source. |
| **BigMama (RTX 5090 / WSL2)** | Docker + `continuum-core-cuda` container → `runtime: nvidia` binds the 5090. `curl install.sh` handles Docker install + compose. | `cargo run --features=cuda` in WSL2 → native nvcc. Support services via Docker. |
| **Generic Linux + Nvidia GPU** | Docker + `continuum-core-cuda` (container GPU passthrough via `runtime: nvidia`). | `cargo run --features=cuda` or the cuda container. |
| **Generic Linux + AMD/Intel/VirtIO GPU** | Docker + `continuum-core-vulkan` (container GPU passthrough via `/dev/dri` — works on real Linux hosts, unlike Mac where Apple hypervisor blocks it). | `cargo run --features=vulkan` or the vulkan container. |
| **Generic Linux / no GPU** | No accelerated path exists → refuse to install with a clear "this machine has no GPU Continuum can use" message. CPU fallback is banned. | Same: build refuses unless `--allow-cpu-only` (Ares bootloader exception flag). |

The exception row is hard: we explicitly refuse to ship a silently-degraded experience. A user without a supported GPU gets a clear refusal, not a working-but-slow install.

## Why Mac differs from Linux (the 2026-04-16 pivot)

Earlier commits (`92f42a847` and predecessors) attempted Podman + krunkit + Vulkan-in-container on Mac. On real M5 Sequoia hardware this fails with `ERROR_OUT_OF_HOST_MEMORY` at `vkCreateInstance`. **Docker themselves confirmed in Feb 2026: "Metal GPU access requires direct hardware access, no GPU passthrough for Metal in containers."** Apple's hypervisor has no IOMMU on Apple GPUs. No VMM — Docker Desktop, Apple `container`, krunkit, libkrun — can expose Metal to a guest Linux kernel.

The correct Mac shape keeps the things that need Metal on the host:

```
┌─ host (Mac) ──────────────────────────────────────────────────────────┐
│  continuum-core-server        vllm-metal (Docker Model Runner)        │
│  (Metal: Candle embeddings,   (Metal: Qwen3.5 LLM)                    │
│   Bevy render, vision, audio)                                         │
│           │                            │                              │
│           └───── HTTP ─────────────────┘                              │
├───────────────────────────────────────────────────────────────────────┤
│  Docker Desktop containers:                                           │
│    postgres, node-server, widget-server, livekit-bridge, model-init   │
└───────────────────────────────────────────────────────────────────────┘
```

This is standard Docker-on-Mac architecture per Docker's own Feb 2026 guidance: **compute on the host, containers orchestrate**. Docker Model Runner ships vllm-metal as a first-class managed-by-Docker host process.

Linux is different because `/dev/dri` passthrough is real on Linux — `continuum-core-vulkan` actually gets a working Vulkan ICD inside the container on AMD/Intel/VirtIO hosts. And `runtime: nvidia` on Nvidia hosts is proven-path container GPU. So Linux variants stay containerized; Mac's continuum-core goes native.

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
| `continuum-core-vulkan` | Any Linux GPU host, OR BigMama, OR cross-compiled on a Mac via buildx | LINUX-ONLY deploy target (/dev/dri works there). Mac doesn't consume this image anymore. |
| `continuum-core` (CPU-only, Ares-bootloader-exception) | Any host | No GPU toolchain required. |
| Native Mac continuum-core-server (Metal) | Mac only | `cargo --features=metal`. Apple Clang + Metal shader compiler are macOS-only. Runs on host; not containerized on Mac per the 2026-04-16 pivot. |

**Future airc broadcast:** each mesh peer advertises its toolchain capability flags; a build request broadcasts to the mesh, the peer with matching capability + lowest load answers. Not implemented yet; current flow is manually choosing the right peer.

## Inference — runtime path per cell

### Primary paths (what we're shipping for PR891)

**Mac Dev AND Mac Carl** (same runtime; install mechanism differs):
- **LLM:** HTTP request → Docker Model Runner → vllm-metal (host-native) → Metal → Apple GPU
- **Everything else (Candle embeddings, Bevy avatar render, vision processing, audio MPS):** native `continuum-core-server` → `workers/llama` / candle / bevy → ggml-metal / MPS / Metal frameworks → Apple GPU
- **Support services** (postgres, node-server, widget-server, livekit-bridge, model-init): Docker Desktop containers, no GPU needed

**Measured on real hardware (Qwen2.5 via Docker Model Runner, llama.cpp Metal):**
- **M5 (BMW M4 tier):** 50.77 tok/s single / **128.48 tok/s at concurrency 8** (2.53× batch scaling)
- **M1 Pro (BMW 2 Series tier):** 12.49 tok/s single / **20.45 tok/s at concurrency 4** (1.64× batch scaling, plateaus at c=4)
- vllm-metal backend also available; llama.cpp is ~1.6× faster at single + batch on this workload, vllm wins on continuous batching + paged attention + OpenAI-compat API

**BigMama Dev:** Native WSL2 binary → `workers/llama` crate → ggml-cuda kernels → CUDA → 5090.

**BigMama Carl:** `continuum-core-cuda` container → same binary → `runtime: nvidia` binds 5090 → CUDA → 5090. Expected parity with BigMama Dev (container overhead ≈ 0 on Linux hosts).

**Linux + AMD/Intel/VirtIO GPU** (Carl or Dev): `continuum-core-vulkan` container → `workers/llama` → ggml-vulkan kernels → Vulkan API → `/dev/dri` passthrough → real GPU. Works on Linux because the Linux kernel-guest-to-host GPU path is real (unlike Apple's).

### The fallback — reusable UDP sidecar (contingent, NOT Mac-default)

**On Mac this fallback is NOT needed** — Docker Model Runner IS already the host-native-service-via-HTTP pattern (same shape as the sidecar design, just Docker-managed).

**On Linux, if a container-Vulkan path can't hit the bar**, we do **not** fall back to CPU. We tunnel out of the container to a host-native binary over UDP.

**The fallback is itself accelerated.** No degradation. The only thing that changes is the IPC boundary — the kernels still run on the GPU.

**Architecture:** one UDP protocol on loopback, N sidecar binaries, reused across subsystems (see `MEMORY.md` → `feedback_udp_sidecar_fallback.md`). Contingency design; not yet implemented because the primary Vulkan paths are holding.

**Why UDP on loopback (if/when we implement it), not Unix socket or TCP:**

- No head-of-line blocking — token streams and audio frames can drop one packet without freezing everything behind it.
- Same protocol family as WebRTC so LiveKit reuses the stack on the wire format side.
- Near-zero loopback latency — `--network host` on Linux is actual host loopback.
- Per-message reliability tier (fire-and-forget / ack-required / ordered-stream) like RTP.

## What this architecture forbids

- **CPU fallback as a shipped path.** Ares bootloader is the exception; it's time-limited and self-upgrades.
- **Silent degradation.** If a subsystem can't run accelerated on a user's hardware, the install refuses with a clear message.
- **Emulation at inference time** (llvmpipe, QEMU guest running compute kernels, software-only Vulkan ICD). API translation / VMM passthrough is fine; software fallback is not. See `feedback_no_emulation_at_inference.md`.
- **Per-subsystem bespoke IPC.** If any subsystem needs a host sidecar, it uses the UDP protocol family (or, on Mac specifically, the HTTP-to-Docker-Model-Runner pattern which does the same job). No new socket abstractions per feature.
- **Inference running TCP.** Never. UDP is solved for low-latency streaming; TCP's HOL blocking is a perf regression for token streams. (Docker Model Runner uses HTTP internally — that's the orchestration API, not the hot inference loop; the compute still runs directly on Apple GPU, not through the HTTP pipe.)
- **Docker image variants that silently run CPU.** Any published `continuum-core-*` image must link a GPU-accelerated ggml backend (metal/cuda/vulkan). The CPU-only `continuum-core:latest` exists ONLY for the Ares bootloader exception and should not be referenced by any Carl or Dev install path.
- **Containerized continuum-core on Mac.** Containers on Mac get no Metal — period. Running continuum-core in a Mac container means Candle/Bevy/vision/audio CPU-fall-back. Native-on-host is the only valid shape for Mac.

## Implementation status (2026-04-16)

- ✅ `continuum-core-cuda` image — CI green (commit `90908b4` + successors)
- 🟡 `continuum-core-vulkan` image — Linux-only deploy target now. CI multi-arch build in flight on the post-pivot branch. Khronos Vulkan-Headers direct install fix applied (commit `d786b8456`) cleared walls 1+2.
- ✅ `scripts/push-image.sh` — dev-side build-test-push with Phase 0–3 (commit `5c3d4bcbb`)
- ✅ `scripts/test-slices.sh` — slice-test harness, per-variant probes (commit `42ddc7308`)
- ✅ `scripts/test-heartbeat.sh` — full-stack integration heartbeat (commit `0b1c5cca2`, Mac variant rewritten in pivot `5e3ca9f87`)
- ✅ `docker-compose.mac.yml` — Mac override sets `continuum-core.replicas=0` so Docker runs only support services; continuum-core runs native via `npm start` (commit `5e3ca9f87`)
- ✅ Mac install.sh — Docker Desktop detection + Docker Model Runner + vllm-metal backend + Rust + Node + native continuum-core launch (commit `5e3ca9f87`, fail-message cleanup `66fb24c36`)
- ✅ Strip + thin LTO on release profile — 200MB → 87MB on continuum-core-server (commit `5407faf2b`)
- ✅ Response cap hard gate removed — personas no longer silenced at 50/session (commit `a1e03d8c7`)
- ✅ M5 acceptance numbers landed (2026-04-16): 50.77 / 128.48 tok/s llama.cpp Metal
- ✅ M1 Pro entry-tier validation (2026-04-16): 12.49 / 20.45 tok/s — confirms flow works on older Apple Silicon
- ⏳ UDP sidecar protocol — contingent on Linux Vulkan measurement; design locked, implementation only if a real perf gap appears
- ⏳ Self-hosted CI runner on BigMama for cuda slice tests — Phase 3d of `RESTORE-FULL-PARITY-PLAN.md`
- ⏳ M5 heartbeat slice end-to-end run — m5-test morning 2026-04-16 (staged: rebased, native cargo building, vllm + Qwen2.5-7B both formats pulled)

## References

- `feedback_carl_and_dev_paradigm.md` — the two user archetypes
- `feedback_inference_runtime_split.md` — llama.cpp for inference, Candle for training, Qwen3.5-4B Q4_K_M as the LCD workhorse
- `feedback_docker_model_runner_mac.md` — the Mac Docker Model Runner pivot (2026-04-16)
- `feedback_no_emulation_at_inference.md` — real silicon at inference always; passthrough OK, emulation banned
- `feedback_support_all_features_no_cheating.md` — no degradation, UDP is solved for low-latency video
- `feedback_udp_sidecar_fallback.md` — the UDP sidecar pattern (Linux contingency)
- `feedback_docker_fast_ephemeral.md` — Carl's 60-second budget
- `docs/infrastructure/INSTALL-ARCHITECTURE.md` — the one install script / module shape contract
- `docs/infrastructure/RESTORE-FULL-PARITY-PLAN.md` — sensory capability restoration plan
