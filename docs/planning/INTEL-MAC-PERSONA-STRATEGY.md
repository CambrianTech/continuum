# Local + Grid Persona Strategy — From Intel Mac to M5

> Joel (2026-05-31): "We want to know if we can get something
> workable for this Intel Mac." Then sharpened: "we do need to
> run locally on a MacBook m5 24 or 48gb memory or about here.
> And so even though if our machine can't do it we need to build
> it AND grid inference and they're just the same command just
> executed across the wire and airc substrate delivered payloads."
>
> Two co-equal targets. Local is the primary execution path on
> the M5; the Intel Mac is the proof that "substrate works
> everywhere" extends down to 2018 hardware via grid offload.
> The unifying contract: grid inference is **the same command**
> as local inference — `adapter.generate_text(request)` — just
> with an adapter impl whose transport is airc instead of llama.cpp.

**Status:** Strategy (2026-05-31).

**Targets (per Joel 2026-05-31, refined):**

> "We'd really be building for 3090 desktops or m5's at the same
> time. The 5090 is luxury but we will take advantage."
> "I have 1080ti and 5090 windows only. Don't have the 3090. Just
> target sizes. M1 or higher ok ram ought to be good too."

So we design for **target SIZES**, not specific GPUs Joel owns:

| Tier | Class | Sized for | Model class |
|---|---|---|---|
| **Primary Apple** | M1 Pro/Max → M5 Pro/Max, ≥ 16 GB UMA (24+ preferred) | Daily driver Apple Silicon | Qwen-2.5-7B → 14B → 27B at Q4_K_M (depending on RAM) |
| **Primary desktop GPU** | NVIDIA Ampere+ class, 24 GB VRAM (RTX 3090 / A5000 / 5090) | Daily driver desktop GPU | Qwen-2.5-14B → 30B at Q4_K_M |
| **Supported older desktop GPU** | NVIDIA Pascal, 11 GB VRAM (GTX 1080 Ti) | Older desktop still in use; substrate citizen | Qwen-2.5-7B at Q4_K_M (~4.5 GB) |
| **Joel's actual hardware** | 1080 Ti + 5090 on Windows; MacBookPro15,1 + Intel Mac | Drives the test matrix; CI must work on all of these | as per tier |
| **Edge local** | MacBookPro15,1 + AMD Radeon Pro 560X | Lower-bound proof; heuristic + reflective + grid offload | None local (CPU 1.1 tok/s); grid offload for real work |
| **Grid peer** | Any reachable continuum-core-server | Same command surface; transport is the only difference | Whatever that peer hosts |

**Critical principles:**

1. **The design target is Apple Silicon AND desktop GPU
   SIMULTANEOUSLY.** Both must work as primary daily-driver
   substrates out of the box. The substrate runs the same Rust
   code on both; adapter selection + Metal-vs-CUDA backend
   handles the hardware diff.

2. **Apple Silicon floor is M1 with adequate RAM**, not just M5.
   M1 Pro / M2 Pro / M3 Pro / M4 Pro at ≥ 16 GB UMA all qualify
   as "primary local" with appropriate model sizing. M5 is just
   the newest; the design doesn't require it.

3. **Windows is a first-class platform.** 1080 Ti and 5090 are
   Joel's actual hardware and they're Windows boxes — the
   substrate must build, run, and serve personas on Windows the
   same way as on macOS/Linux. (Continuum-core-server already
   targets Windows per the existing infrastructure notes.)

4. **5090 / Ampere+ are luxury sizing**, not requirements.
   Designing AROUND a 5090 would lock out everyone without one.
   The realistic-floor doc's "ONE base model, N persona lanes"
   target is the 3090-class size budget; bigger GPUs use the
   headroom for more lanes / bigger models, not a different
   architecture.

**Classification (`cognition/model_resolver/types.rs`):**
- M5 → `HwCapabilityTier::M5UmaProMax` (**not yet enumerated** — task #115 adds the variant; current code would classify M5 as M3UmaProMax fallback)
- RTX 3090 → `HwCapabilityTier::Sm86`
- RTX 5090 → `HwCapabilityTier::Sm120`
- GTX 1080 Ti → `HwCapabilityTier::Sm60` (Pascal, compute capability 6.1; **not yet enumerated** — task #115 adds the variant + probe detection)
- Intel Mac → `HwCapabilityTier::MacIntelMetalDiscrete`

**Parents:**
- [`docs/architecture/INFERENCE-LANES-REALISTIC.md`](../architecture/INFERENCE-LANES-REALISTIC.md) — realistic floor
- [`docs/architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md`](../architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md) — aspirational ceiling

---

## The measured baseline (honest)

The substrate has direct evidence from 2026-05-30 runs on this
hardware (preserved in `cognition/host_capability_probe.rs:139`
and `model_resolver/types.rs:58`):

| Path | Result |
|---|---|
| Metal-on-AMD (llama.cpp's Metal shaders) | **0.8 tok/s + garbled output + nil tensor buffer errors** — broken |
| CPU-only (`n_gpu_layers=0` forced via `CONTINUUM_TIER=mac_intel_discrete`) | **1.1 tok/s + coherent output** — works |

The hardware probe (`install.sh` + `governor`) sets the env-var so
the LlamaCppAdapter forces `n_gpu_layers=0` at adapter load. That
hard truth shapes everything downstream.

---

## Apple Silicon class (primary local) — what's workable

Apple Silicon (M1 Pro/Max → M5 Pro/Max) at ≥ 16 GB UMA is where
personas run as their daily-driver substrate. The full realistic-
floor design ships here. Throughput scales with generation; the
floor is M1.

| Resource | M1 Pro/Max 16-32 GB | M2/M3 Pro/Max 16-48 GB | M4/M5 Pro/Max 24-48+ GB |
|---|---|---|---|
| Default model | Qwen-2.5-3B → 7B Q4_K_M | Qwen-2.5-7B → 14B Q4_K_M | Qwen-2.5-14B → 27B Q4_K_M |
| Inference path | LlamaCppAdapter via Metal (UMA, no n_gpu_layers throttle) | same | same |
| Throughput (Qwen-7B) | ~20-30 tok/s | ~30-45 tok/s | ~50-70+ tok/s |
| n_seq_max | **2-4** (RAM-dependent) | **4** (auto-enabled by #110 probe) | **4-6** depending on KV budget |
| Concurrent lanes | 2-3 active personas | 3-4 | 4-6 |
| Real-time voice/video | Borderline on M1, comfortable from M2 Pro up | YES | YES + room for vision pipeline |

The realistic-floor doc's "ONE base model, N persona lanes via
continuous batching" is the Apple Silicon path's primary mode.
Lane multiplexing through the in-backend scheduler (already
shipped per #109) serves 3-4 concurrent personas (real
conversations + reflection + sentinel review) on one model load
on M2+ class hardware.

**Apple Silicon alone is enough for a single-user substrate with
rich persona behavior at M2 Pro and above.** Grid offload is the
unlock for multi-user / heavier work, not a precondition. M1 is
the floor; below that (M1 base / 8 GB) you're more in the Intel
Mac territory — heuristic adapter + small models + grid offload
for serious work.

### NVIDIA desktop GPU class (Ampere+ 24 GB / Blackwell 32 GB)

The CUDA equivalent of the Apple Silicon path. The LlamaCppAdapter
uses llama.cpp's CUDA backend instead of Metal; everything else is
the same code path. Joel's 5090 sits in this class.

| Resource | Ampere+ 24 GB VRAM class (RTX 3090 / A5000) | Blackwell 32 GB VRAM (RTX 5090) |
|---|---|---|
| Default model | Qwen-2.5-14B Q4_K_M (~9 GB) | Qwen-2.5-32B Q4_K_M (~19 GB) or 14B at FP16 |
| Throughput | ~60-80 tok/s on 7B; ~30-40 on 14B | ~100+ tok/s on 7B; 50-60 on 14B |
| n_seq_max | **4-6** | **6-8** |
| Concurrent lanes | 4-6 active personas + background | 6-8 |
| Real-time voice/video | YES | YES + room for vision pipeline |

The 24-GB class is the substrate's "good desktop" baseline that
sizing decisions target. The 5090 (which Joel has, Windows) is
opportunistic upper-class — same code path, more headroom for
bigger models or more concurrent lanes.

### NVIDIA Pascal class (GTX 1080 Ti, 11 GB VRAM, Windows)

The substrate's "older desktop still in use" target. Pascal is
two generations behind Ampere; smaller VRAM means smaller model.
Joel has one of these (Windows).

| Resource | 1080 Ti class |
|---|---|
| Default model | Qwen-2.5-7B Q4_K_M (~4.5 GB) |
| Throughput | ~30-40 tok/s on 7B |
| n_seq_max | **2-3** (VRAM headroom dictates) |
| Concurrent lanes | 2-3 active personas |
| Real-time voice | Borderline — 7B at 30 tok/s gives ~3-sec responses; chat-class voice works, fast turn-taking marginal |
| Real-time video | Likely needs grid offload for the avatar |

### Windows support is required

Both of Joel's NVIDIA boxes (1080 Ti + 5090) are Windows.
Continuum-core-server runs on Windows as a first-class platform —
not a compatibility afterthought. The CUDA paths use llama.cpp's
CUDA backend the same way as Linux; the substrate doesn't care
about OS as long as the adapter + build artifacts produce. Build
matrix MUST include Windows; CI MUST exercise the Windows path on
at least the heuristic-adapter substrate flow.

### Substrate-runs-everywhere principle

The same Rust code, the same lane substrate, the same RAG layer,
the same coordinator + handle store + capture sinks ship on
**M5 + 3090 + 1080 Ti + 5090 + Intel Mac**. Adapter selection
(Metal vs CUDA vs CPU-only) + model picks per tier are the only
hardware-aware bits; everything above the adapter trait is host-
agnostic.

The grid principle compounds this: a user with an Apple Silicon
laptop + an older NVIDIA box on Windows + a newer NVIDIA box
elsewhere (Joel's actual setup) gets the substrate's lane
coordinator multiplexing locally AND remotely across all of them.
The substrate doesn't care which lane is where.

### M2+ Pro/Max throughput math (worked example)

- Qwen-2.5-7B Q4_K_M @ ~40 tok/s on M2/M3 Pro (faster on M5)
- 100-token response = 2.5 seconds wall-clock
- 4-lane continuous batching: ~25-30 tok/s per lane (aggregate
  doesn't double, but is much better than serializing)
- Voice chat: a 50-token reply in ~2-3s — speech-natural turn
  pacing works
- Video avatar: avatar lip-sync runs ahead of the audio generation;
  needs the local TTS path which is its own pipeline

This is the substrate's defining boast realized locally on any
modern Apple Silicon laptop. No grid required.

## Grid inference — the same command across the wire

Joel (2026-05-31): "grid inference and they're just the same
command just executed across the wire and airc substrate
delivered payloads."

This is the architectural contract:

```rust
// LOCAL — LlamaCppAdapter on M5 via Metal
let response = adapter.generate_text(request).await?;

// REMOTE — AircRemoteInferenceAdapter (#108) on the same TextGenerationRequest
let response = remote_adapter.generate_text(request).await?;
```

The CALLER sees no difference. Both impls return
`TextGenerationResponse`. The remote impl:

1. Serializes `TextGenerationRequest` as a typed airc envelope
2. Sends via airc to a peer (the 5090 with continuum-core-server running)
3. The peer's local `InferenceLlmModule` handles the request via
   ITS local adapter (whichever is registered there)
4. The peer serializes the response back as an airc envelope
5. Local `AircRemoteInferenceAdapter` deserializes and returns
   `TextGenerationResponse`

Everything ABOVE the adapter trait (handle store, lane coordinator,
RAG inspection, persona response, chat module, sentinel review)
treats remote and local identically. Composes with #109's lane
multiplexing — the coordinator can hold a mix of local AND remote
handles in the same lane budget.

**Practical use (Joel's actual hardware grid):**

- Apple Silicon laptop hosts most personas locally on a real model
- Joel's 5090 (Windows desktop, in another room) hosts overflow /
  specialty personas (bigger model, vision pipeline, code-gen
  specialist) when reachable via airc
- Joel's 1080 Ti (Windows) hosts a smaller model serving its own
  lanes; reachable as a grid peer for additional offload
- Joel's Intel Mac participates as a citizen via heuristic
  adapter + reflective lanes locally, and routes any real-model
  work to one of the GPU boxes via grid

The point: this isn't a single-machine substrate. Joel's actual
setup is a grid of heterogeneous boxes, and the substrate routes
lanes wherever capacity is available.

The substrate doesn't know or care where the inference happens.
That's the whole point.

---

## Intel Mac edge target — what 1.1 tok/s actually means for personas

This section is specific to the Intel Mac (MacBookPro15,1) — the
substrate's lower-bound proof point. Skip ahead if you're working
on M5.

A typical persona response is 100-300 tokens. At 1.1 tok/s:

| Response length | Wall-clock |
|---|---|
| 50 tokens (terse reply) | ~45 seconds |
| 100 tokens (normal chat) | ~90 seconds (1.5 min) |
| 300 tokens (verbose) | ~4.5 minutes |

Speech-natural turn-taking is ~200ms. Live video chat at 30fps
demands frame-rate budgets. **Neither is feasible locally on this
hardware.** Anything that requires latency under a few seconds has
to either:

1. Use the heuristic adapter (no real intelligence, but zero
   latency + deterministic for tests).
2. Offload to a grid peer via [#108
   AircRemoteInferenceAdapter](../../docs/architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md#cross-grid-inference)
   — Joel's 5090 in another room running the same architecture.

---

## What IS workable on this Mac

Specific use cases that fit the 1.1 tok/s budget:

### ✓ Single-persona slow-chat (the realistic baseline)

- ONE persona at a time
- Text chat with explicit "thinking..." UX
- 30 second to 2 minute response time is acceptable for thoughtful
  reflective conversation
- Persona's RAG layer + L1 budget already shipped; the bottleneck
  is purely the model
- **Smallest viable model: Gemma-2-2B Q4_K_M (~1.6 GB) or
  Qwen-2.5-1.5B Q4_K_M (~1 GB).** Creative capacity ceiling but
  fits CPU comfortably.

### ✓ Background reflection / journaling

- Personas process inbox during idle periods
- Generation runs in the background; user doesn't see it real-time
- 1.1 tok/s × multi-minute idle = ~hundreds of tokens of reflection
  per idle window
- Works at any model size that fits RAM

### ✓ The heuristic adapter in all paths

- The heuristic adapter (`HeuristicInferenceAdapter`, task #103) is
  zero-cost on any host
- It's NOT real intelligence, but it IS:
  - Deterministic (same prompt → same response)
  - Sub-millisecond latency
  - Substrate-correct (full lane lifecycle, capture sinks, eviction)
- **The heuristic adapter is what makes CI possible** — the lane +
  coordinator + handle module + rag-inspect tests all pass without
  a GGUF
- The heuristic adapter is also a viable "thinking placeholder" UX
  on this Mac: the persona's RAG layer surfaces real context, the
  heuristic stand-in echoes it back as proof of substrate health,
  while a real model warm-up happens in the background

### ✓ Substrate validation (the test suite)

- 110+ tests across the lane substrate, all green on the heuristic
  adapter, no GGUF required
- The full RAG → prompt → response → capture loop runs end-to-end
  in unit tests on this Mac in seconds
- This IS our "workable persona on the Intel Mac" baseline for CI

---

## What is NOT workable on this Mac

- **Real-time voice chat.** 1.1 tok/s × ~3 second target = 3
  tokens per turn. Useless.
- **Real-time video avatar.** Avatar lip-sync needs sub-100ms
  inference. Two orders of magnitude off.
- **16 concurrent personas with real model.** Even multi-seq
  batched, CPU bandwidth is the bottleneck; 16 × any decent
  response = hours.
- **Big-model quality.** Anything > 3B parameters at Q4 is too
  slow for any interactive use.

These all become workable when we add grid offload (#108) — the
M5 / 5090 elsewhere handles latency-sensitive work; the Intel Mac
runs reflective / background lanes locally.

---

## The three things we ship to make this Mac workable

### 1. CI proves the heuristic adapter end-to-end ("working persona")

Joel's plan: "we probably want to get our tests to prove working
persona, into CI, so your heuristic adapter will also have to prove
itself in a live environment."

What this means concretely:

- A CI job (or local headless harness) that:
  - Boots `continuum-core-server`
  - Boots `airc` daemon
  - Attaches a Paige-class persona via the real persona persistence
    + airc-attach path
  - Sends a chat message via `chat/send` → routes through the
    persona's cognition cycle → through the inference command →
    heuristic adapter responds
  - The response posts back via airc and shows up in chat
- The heuristic's `[heuristic:<hash>] ack: "..."` output is
  deterministic, so the test asserts the substrate produced the
  right shape (lane opened, response captured, posted back) without
  asserting on the response prose itself
- **Validates: every substrate layer is wired correctly, end to
  end, with no real GGUF needed.** A user on the same hardware
  who installs and runs continuum gets a usable system out of the
  box with the heuristic; swapping in the small Gemma model is a
  config change, not a code change.

Concrete tasks (separate, focused):
- PersonaResolver impl for `persona/rag-inspect` reading
  `~/.continuum/personas/<name>/seed.json` + airc_lib::Airc::attach_as
- Headless CI harness that exercises the full chat flow
- Smoke test asserting heuristic response makes it from inference
  through airc

### 2. Smallest viable GGUF for "thoughtful slow personas"

When the user explicitly wants a real model:

- Default model on this tier: **Gemma-2-2B Q4_K_M** (~1.6 GB) — best
  creative density at small size
- Fallback: **Qwen-2.5-1.5B Q4_K_M** (~1 GB) for hosts under 8 GB RAM
- LlamaCppAdapter already configured for CPU-only (`n_gpu_layers=0`)
  on this tier
- n_seq_max stays at 1 (the architecture probe is overkill at this
  speed — even on safe arches, multi-seq batching on CPU at this
  scale doesn't help meaningfully; one slow sequence at a time is
  the right shape)
- Inference handle held by ONE active persona at a time; background
  lanes wait

Concrete task:
- Model registry default-pick for `MacIntelMetalDiscrete` tier set
  to Gemma-2-2B Q4_K_M
- Validate the GGUF actually exists on a clean install (#49 is the
  related pending task — "Resolve missing GGUF in 0.8b/2b forge
  repos")

### 3. AircRemoteInferenceAdapter (the unlock — #108)

Once #108 lands:

- Joel's Intel Mac runs reflective / background lanes locally on
  Gemma-2-2B at 1.1 tok/s
- Joel's 5090 (in another room, on its own continuum instance)
  hosts the real persona work — voice/video/realtime chat
- Lanes route via airc: the Intel Mac's coordinator opens a remote
  handle, generates via the airc transport, gets responses back at
  GPU speed
- **The Intel Mac becomes a fully functional substrate citizen** —
  reflective work locally, hot work remotely

This is the substrate's defining boast realized for Joel's actual
hardware: "We host what seems impossible" (per
[[host-the-seemingly-impossible]]) — a Mac Intel Pro from 2018
participates in the 16-persona substrate at full quality, with the
heavy work offloaded over airc to whatever GPU is reachable on the
grid.

---

## Why this strategy is honest

What we're NOT doing:
- **Not tiering down model quality on the Mac to "make voice work."**
  Per [[host-the-seemingly-impossible]], we don't degrade quality
  for capacity. Voice on the Mac is degraded by ARCHITECTURE
  (offload to grid), not by tiering down the model.
- **Not pretending Metal-AMD works.** The 2026-05-30 evidence is
  in the codebase; the substrate forces CPU on this tier.
- **Not running 16 personas concurrently on this Mac.** Lane
  multiplexing is built; on this hardware it's used for 1-2 slow
  lanes locally, with the rest of the budget routed remotely.

What we ARE doing:
- Using the heuristic adapter to make the substrate fully
  observable + testable + deterministic on this Mac (and any other
  modest host).
- Sizing the local model to what 1.1 tok/s can serve well (small
  reflective work, single-persona slow-chat).
- Building the grid offload (#108) as the unlock for anything
  real-time.

---

## Timeline (the order things land)

1. **Now (committed):** Lane substrate + heuristic adapter + RAG
   inspection + n_seq_max probe + production wiring of multi-seq
   for safe architectures + bypass audit. The heuristic adapter
   proves the substrate works on any host.

2. **Next slice (small):** PersonaResolver implementation + CI
   harness that proves the full chat flow end-to-end on the
   heuristic adapter. "Working persona on this Mac" achieved at
   zero compute cost. This is the proof Joel asked for.

3. **Then (#108):** AircRemoteInferenceAdapter — the substrate's
   defining capability for Joel's specific hardware constellation.
   Crap Mac plus distant 5090 equals viable persona host.

4. **Then (model picks):** Per-tier default model selection so
   installing continuum on this Mac gives a usable
   Gemma-2-2B-backed persona out of the box. The realistic floor's
   "creative capacity, not stupid" target.

---

## Summary

**Q: Can we get something workable for this Intel Mac?**

**A: Yes.**

1. **Workable today (no further code):** Heuristic adapter through
   the full substrate stack. The 100+ tests landed this session are
   a working persona substrate. Lights-on demonstration that the
   architecture is real, on this exact Mac.

2. **Workable soon (small slice):** CI proves the heuristic adapter
   in a live end-to-end chat flow (PersonaResolver + CI harness).
   "Working persona" by the definition of "any AI in airc can chat
   with Paige, get a deterministic heuristic response, and observe
   the full substrate trace."

3. **Workable for real personas (#108):** Grid-offload to a peer
   with a GPU. The Intel Mac runs lanes; an NVIDIA box (Joel's
   1080 Ti or 5090) runs inference via the
   AircRemoteInferenceAdapter — same `adapter.generate_text(req)`
   command, airc transport. The substrate handles routing
   transparently.

The realistic floor is not "small model + heroic local serving."
The realistic floor is "substrate works everywhere + cleverness
offloads to where the compute is." This Mac is a first-class
citizen in that vision.
