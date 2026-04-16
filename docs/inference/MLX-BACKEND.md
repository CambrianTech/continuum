# MLX Backend Adapter — Planning Doc

**Status:** planning (no code yet). Branch `feature/inference-perf-metal` off `feature/inference-perf`. Closes part of #896 (Apple MLX adapter candidate).

## Why

Continuum's stated goal for its target MacBook audience: **low-latency e2e live Qwen3.5 models that see, hear, speak, and emote via avatars naturally — 3–5 local personas — without routing through slower TTS/STT/YOLO pipelines.**

The existing backends don't deliver that:

| Backend | Text | Audio in | Audio out | Vision in | M-series native |
|---|---|---|---|---|---|
| `llama.cpp` (Metal) | ✅ | — | — | partial (llava adapter) | via ggml-metal translation |
| `candle` | ✅ | — | — | — | via Metal fork |
| ONNX Runtime (existing, used for VAD/Piper) | ✅ maybe | — | — | — | CoreML EP (limited for 4B+ LLMs) |

Qwen3.5-Omni has native audio-token output + vision heads. The `mlx-community` ports of Qwen-Omni are already running on M-series today with the full omni stack. MLX is the backend that matches the deliverable.

## Non-goals

- Replacing `llama.cpp` for CUDA/Vulkan — llama.cpp stays primary for non-Apple hardware.
- Replacing `candle` for training — candle's forward/backward graph stays the training primary per `memory/feedback_upstream_fixes_tri_repo.md`.
- Building custom Metal kernels from scratch — MLX already has them, use them.

## Architecture

Slot into the existing `ModelBackend` trait at `src/workers/continuum-core/src/inference/backends/mod.rs`. One new file: `backends/mlx_adapter.rs`. Nothing else in the stack should need to know an MLX-backed persona is any different from a llama.cpp-backed one.

```
inference/backends/
├── mlx_adapter.rs          ← NEW
├── llamacpp.rs             ← existing (text, any HW)
├── qwen35_gguf.rs          ← existing (GGUF loader)
├── ...
└── mod.rs                  ← trait def (no changes expected)
```

Model selection per persona stays in `model_registry.json`. MLX variants register alongside GGUF/safetensors variants.

## Rust binding strategy (the first unknown to resolve)

**Open question, resolved in PR A** (see below): does the community `mlx-rs` crate cover enough of MLX's C++ API for our needs?

Decision criteria:
1. Does it expose the operations Qwen3.5 inference needs? (embedding, matmul, attention, RMSNorm, RoPE, LoRA weight merging)
2. Is it maintained? Last commit recency, open-issue volume, version churn.
3. Does it wrap MLX ≥ 0.24 (Qwen-Omni requires recent MLX)?
4. Is the memory model sane for our unified-memory sharing with Bevy / LiveKit?

Outcome branches:
- **Viable** → use `mlx-rs`, focus on adapter logic.
- **Thin / stale** → hand-FFI against `mlx-c` (Apple's C bindings layer). This is a 3–5 day bridge before any adapter work starts.
- **Nonexistent `mlx-c` for needed surface** → escalate to Joel; scope may shift to "contribute upstream bindings first, adapter after."

## Staged PRs

All target `feature/inference-perf` (the PR #891 integration branch). Same branch memento is on for Vulkan work; our file surfaces don't overlap.

### PR A — binding choice + scaffold (this PR)

**Contents:**
- This doc.
- `backends/mlx_adapter.rs` scaffold — `ModelBackend` impl with `unimplemented!()` bodies + capability declarations. Compiles, links, `continuum doctor` doesn't regress.
- `Cargo.toml` feature flag `mlx` gated on `target_os = "macos"` and `target_arch = "aarch64"`.
- Follow-up issue opened: "Evaluate mlx-rs vs hand-FFI — decision + bench."

**Does not include:** actual MLX library link yet. That's PR A.5 (or folded into PR A if mlx-rs turns out to be a one-liner).

**Acceptance:**
- `cargo check --features mlx` on M-series passes.
- `cargo check` on non-Mac targets still passes (mlx feature flag disables the module entirely on non-Apple).

### PR B — text-only Qwen3.5 via MLX (the outlier validation)

**Contents:**
- Fill in `generate()`, `prefill()`, tokenization, EOS handling for Qwen3.5-4B Q4.
- Register `qwen3.5-4b-mlx-q4` in `model_registry.json`.
- Benchmark harness: N concurrent generation requests, measure tokens/s + TTFT + memory. Compare to `qwen3.5-4b-gguf-q4_k_m` over `llamacpp` on the same M5.

**Acceptance:**
- ≥ parity with `llama.cpp-Metal` on tokens/s + TTFT on M5 for Qwen3.5-4B Q4.
- No CPU fallback path (per `memory/feedback_support_all_features_no_cheating.md`).
- Concurrent sensory envelope holds: run test assembly of `MLX-persona + Bevy + Piper-kept-for-reference + LiveKit` and measure whether the other components degrade.

**Outlier-validation payoff:** if the trait survives a backend this different from llama.cpp, every other candidate in #896 is a smaller lift.

### PR C — Audio output head (the Omni win)

**Contents:**
- Extend adapter to emit Qwen-Omni audio tokens + audio decoder.
- `MediaArtifactSource` integration — personas backed by MLX bypass the Piper TTS call on hot path.
- Preservation path: non-MLX personas keep Piper; routing is per-persona.

**Acceptance:**
- Persona speaks natively, no Piper on the hot path.
- Audio latency ≤ current Piper+pipeline latency (probably much better, but that's the floor).

### PR D — Vision input head

**Contents:**
- Image encoding via Qwen-Omni vision adapter.
- `VisionDescriptionService` routes MLX-backed personas to the native head; others keep the existing image-to-text classifier.

**Acceptance:**
- Persona "sees" a screenshot natively, no YOLO / CLIP / VisionDescriptionService fallback for MLX personas.

### PR E — Forge tier publication

**Contents:**
- `ForgeRecipe.quantTiers[]` gains MLX Q4 + Q8 entries.
- Foundry template emits MLX artifacts alongside GGUF.

**Acceptance:**
- Forge publishes a Qwen3.5 alloy with both GGUF and MLX tiers.
- Continuum's auto-download picks the right tier per device.

## Risks named upfront

1. **mlx-rs maturity** — unknown until PR A. If thin, PR A expands from "days" to "1-2 weeks of FFI bridge" before any adapter work starts. This is the load-bearing unknown.
2. **Qwen3.5-Omni MLX port stability** — `mlx-community` ports the architecture but the audio-head API surface isn't frozen. May need to pin a specific port commit + track upstream.
3. **M5-16GB unified memory pressure** — Qwen3.5-Omni + avatar + LiveKit + system OS is tight. Benchmarks must include the full sensory envelope, not just tokens/s in isolation (per CLAUDE.md's concurrent-sensory-envelope rule).
4. **Benchmark parity is not a given** — ggml-metal is heavily optimized. MLX may win on some ops and lose on others. Honest reporting over green-washed comparisons.

## Coordination

Memento is on `feature/inference-perf` for Vulkan (Carl-on-Mac Podman+krunkit path). Different `backends/*.rs` file, different `model_registry.json` entries, no overlap. Syncing with memento's work = `git pull` on `feature/inference-perf` before each commit.

Airc cross-mesh DM bug (filed as airc#14, fix in airc#16) currently blocks direct DM coordination with memento. Until that merges, coordination goes via Joel or shared-log broadcasts.

## Related

- #896 — open invitation for ModelBackend adapters
- #891 — feature/inference-perf integration branch
- `memory/project_m5_is_primary_audience.md` — M-series is the target
- `memory/feedback_support_all_features_no_cheating.md` — no CPU fallback
- `memory/feedback_upstream_fixes_tri_repo.md` — fork-first, cycle upstream as fixes merge
- `memory/project_3d_immersive_vision.md` — 3D-immersive-first product identity (avatars, audio, vision are load-bearing)
