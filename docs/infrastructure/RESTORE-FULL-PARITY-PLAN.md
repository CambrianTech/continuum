# Restore Full Sensory Parity — Post-Docker Plan

## Context

The README ([§ Every persona has a full sensory system](../../README.md)) documents continuum's core promise: Vision / Hearing / Speech / Emotion / Avatar — every persona gets every sense, with enabling-aid bridges for models that lack native capability.

**This was working before the docker-first push.** Our docker containers today ship a stripped-down inference-only continuum because we cut corners to unblock each build wall during PR #891. That is over. This doc enumerates every cheat + the plan to land full parity in-container.

## Cheats currently live in main

Each one strips a documented sensory capability from the containerized product:

| # | Location | Cheat | Impact |
|---|---|---|---|
| 1 | `src/workers/continuum-core/src/main.rs:171` | `CONTINUUM_SKIP_STT=1` env hatch | Personas can't hear. |
| 2 | `src/workers/continuum-core/src/main.rs:172` | `CONTINUUM_SKIP_TTS=1` env hatch | Personas can't speak. |
| 3 | `src/workers/continuum-core/Cargo.toml:62` | `whisper-rs = "0.13"` commented out | No STT backend even when enabled. |
| 4 | `src/workers/vendor/whisper.cpp/` | Submodule vendored but no Rust wrapper | Dead weight until wired. |
| 5 | `docker-compose.yml:63` + `docker-compose.gpu.yml:47` | `--no-default-features` strips `livekit-webrtc` | No in-process WebRTC → no low-latency video calls in container. |
| 6 | `docker/continuum-core*.Dockerfile` | `RUN mkdir -p /app/avatars` (empty) | No VRM models → avatar pipeline dead. |
| 7 | `src/models/avatars/` (133MB) | Git-ignored (`src/.gitignore` `/models/`) | CI checkout has nothing to bake; no way to ship them today. |

Each of these was necessary to unblock a build wall. None is acceptable as a shipped state.

## Plan

### Phase 1 — Un-cheat (priority; this PR or immediate follow-up)

| Slice | Owner | Concrete work |
|---|---|---|
| **1a. Vendor whisper.cpp properly** | `continuum-3bb8` (memento) | New `workers/whisper` crate following the `workers/llama` shape. Safe Rust wrapper (Model / Context / Segment), cmake driver with `POSITION_INDEPENDENT_CODE=ON`, Metal/CUDA feature flags that propagate from continuum-core, `+whole-archive` on ggml static libs, submodule already present at `workers/vendor/whisper.cpp`. Replace the commented-out `whisper-rs = "0.13"` in continuum-core's Cargo.toml. |
| **1b. Remove SKIP_STT/SKIP_TTS hatches** | `continuum-3bb8` (after 1a) | Delete the `CONTINUUM_SKIP_STT` / `CONTINUUM_SKIP_TTS` branches in `main.rs`. STT/TTS init always runs. |
| **1c. Ship avatars in-container** | *unclaimed* | Option A: CI-fetch from CC0 mirror (github release or OCI artifact), cache via GHA. Option B: git-LFS the 133MB. Either way: remove the `mkdir -p /app/avatars` stub and restore real `COPY --from=avatars` in both Dockerfiles + `build-contexts: avatars=…` in `.github/workflows/docker-images.yml`. |
| **1d. Keep livekit-webrtc in container default-features** | *unclaimed* (m5-test or vhsm — livekit-bridge context) | Current `GPU_FEATURES="--no-default-features --features …"` strips livekit entirely. Switch to an additive pattern that keeps `livekit-webrtc` in the default set. If the webrtc build is the reason (slow/heavy), separately fix that — don't drop the feature. |

### Phase 2 — Validate in-container (after phase 1)

| Slice | Owner | Concrete work |
|---|---|---|
| **2a. Extend `continuum doctor` with sensory checks** | *unclaimed* (m5-test — natural extension of 9f8aa0ae8) | Doctor runs: STT model loads + synthesizes first second; TTS loads + generates 500ms of audio; avatar VRM file opens cleanly; LiveKit agent manager initializes without error; Bevy device enumerates at least one render surface. Each is its own check, fails loud with fix. |
| **2b. Voice call e2e over UDP** | *unclaimed* | Actual call between two containers (BigMama + M1, or BigMama-only loopback). Verify LiveKit bridge routes UDP, measure round-trip. Confirms the already-solved UDP strategy holds through the container network story. |
| **2c. Bevy 3D render inside container** | *unclaimed* | Headless Vulkan or wgpu software fallback — whichever we already support. Load VRM, render one frame to PNG, verify image content is not a blank. |

### Phase 3 — Image size + CI cache (feature-preserving, parallel to everything else)

| Slice | Owner | Concrete work |
|---|---|---|
| **3a. strip + LTO Rust binary** | `continuum-3bb8` (alongside 1a) | Add `strip = "symbols"` + `lto = "fat"` to `[profile.release]` in `src/workers/Cargo.toml`. Expected: ~200MB → ~80MB for continuum-core-server binary. No feature change. |
| **3b. cargo-chef GHA registry cache** | *unclaimed* | Key on hash of `Cargo.lock` + feature flags. When deps don't change, `cargo chef cook` becomes a cache hit. ~30 min saved on every PR build. |
| **3c. candle-kernels content cache** | *unclaimed* | Cache `target/release/build/candle-kernels-*/out` separately; CUDA kernel artifacts are content-addressed. ~25 min saved. |
| **3d. Self-hosted CUDA runner** | *unclaimed* (longer term) | Run CI cuda job on BigMama. Pre-warmed cache, native nvcc, no GHA queue. Target: ~15 min total build. |

### Phase 4 — Multimodal qwen3.5 (future, not this PR)

Forge vision + audio encoders onto qwen3.5 base via forge-alloy's modality stage. Reduces the enabling-aid bridge complexity: a model that natively hears + sees is simpler than one whose pipeline does STT→text→model→text→TTS. Separate, later work. The sensory system (Phases 1-2) is what lets the model be optional.

## Non-negotiables (per `feedback_support_all_features_no_cheating.md`)

- **UDP for low-latency video is already solved.** Don't propose TCP / WebSockets / upstream-only fallbacks.
- **No feature drops to save size/time.** Every optimization is feature-preserving. If the only way to get build time down is cutting a feature, we've picked the wrong optimization.
- **Known gaps are TODOs with owners, not permanent state.** Every slice above must have a claim in the "Owner" column before Phase N is considered planned.
- **Never panic and quit.** Each wall gets dug into.

## Coordination

Work divided over `airc` mesh (`continuum-3bb8` / `m5-test` / `airc-96dd` / `vhsm`). When mesh is down, claims are still live in this document — pick up where the last claimer left off by reading the git log on the relevant files.
