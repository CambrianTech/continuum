# Intel Mac Persona Strategy — What's Workable

> Joel (2026-05-31): "We want to know if we can get something
> workable for this Intel Mac." This document answers it.
> Honest measured constraints, the achievable target, and the
> three things we ship to make it real.

**Status:** Strategy (2026-05-31).

**Target hardware:** MacBookPro15,1 (Mac Intel + AMD Radeon Pro 560X
discrete GPU). Classified by the substrate as
`HwCapabilityTier::MacIntelMetalDiscrete` per
`cognition/host_capability_probe.rs`.

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

## What 1.1 tok/s actually means for personas

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
   with a GPU. The Intel Mac runs lanes; the 5090 runs inference.
   The substrate handles the routing transparently.

The realistic floor is not "small model + heroic local serving."
The realistic floor is "substrate works everywhere + cleverness
offloads to where the compute is." This Mac is a first-class
citizen in that vision.
