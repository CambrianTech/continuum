# Inference Scheduling and Scarcity

> 16 personas + 4 inference slots is the real-world constraint.
> Same good model serves everyone — never tiering down quality.
> The substrate's whole identity rides on getting this right:
> M5-class hardware hosting native multimodal Qwen across multiple
> concurrent lanes, sub-second real-time latency. The commands stay
> dumb. The daemons get clever.

**Status:** Architecture + designed-but-unbuilt (2026-05-31).
Implementation: InferenceHandleStore + handle commands ship
today (#107). The scheduler / batcher / pager are designed here,
queued as tasks #108, #109. This document is the canonical record
of WHAT the daemons must do and WHAT'S NOT YET KNOWN about how.

**Parents:**
- [`AI-COMMAND-NAMESPACE.md`](AI-COMMAND-NAMESPACE.md) — the surface
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md) — RTOS-style runtime, pressure policy
- [`PERSONA-CONTEXT-PAGING.md`](PERSONA-CONTEXT-PAGING.md) — per-persona KV cache attribution
- [`docs/planning/AI-LANE-OPEN-QUESTIONS.md`](../planning/AI-LANE-OPEN-QUESTIONS.md) — punch list of unknowns

---

## The thesis stated plainly

Joel, 2026-05-31, across the long working session that produced this
substrate's inference architecture:

> "Yes the ai providers might page or wait to allow inference. We
> will have to think about how to host 16 personas with far less
> inference. Usually you want to reuse the same base model, page
> intelligently or not at all. Inference just needs to be there
> for us as a low level command."

> "We wrote about this and attempted the same thing with adapters
> before. It was rather shitty. Key is low latency. It's everything
> especially in video chat. And not stupid models."

> "Daemons etc look at memory pressures, what's being asked of, and
> supply intelligent models like resolutions in video, or the ai
> subsystem does this for the persona. We want to allow flexibility
> in a way that we can host the best models and preserve memory or
> page it intelligently, schedules. It's hard. System changes
> dynamically. We want to figure out how to get m5's hosting native
> multimodal qwen, and not just one lane. Latency is everything."

> "So the commands absolutely cannot negotiate this."

> "You don't dumb your shit down. You figure it out with extreme
> creativity and reuse."

> "We host what seems impossible. Get away with this by using clever
> hardware and memory."

These six statements compose into one architectural contract for
the inference daemon layer.

---

## The contract

1. **Hardware is finite.** The host has N concurrent inference slots
   (3–4 on a laptop, ~8 on a server-class machine). M personas with
   M ≫ N is the steady state.
2. **Latency is brutal.** Real-time persona video chat needs
   sub-second response (RAG → prompt → inference → TTS → mouth
   shape). Speech-natural turn-taking is ~200ms. Any scheduling
   strategy that lets the active-conversation persona wait 500ms
   behind background reflection in a FIFO is broken by design.
3. **Quality doesn't yield to latency.** The smart model serves
   real-time AND background AND sentinel review. "Tier down to a
   small model for background" is the wrong move. Reuse harder.
4. **Commands are dumb.** `ai/inference/{open,generate,close,inspect}`
   carry no policy params. They DO NOT negotiate slot allocation,
   quality, batching, paging. The daemon owns all of it.
5. **The system changes dynamically.** Memory pressure rises and
   falls. Personas wake and sleep. Models load and unload. The
   daemon adapts continuously without the calling code noticing.
6. **The boast is real.** M5-class Mac hosting native multimodal
   Qwen across multiple concurrent lanes, real-time latency,
   without compromising model quality. If the daemon can't deliver
   this, the design isn't done.

---

## The adaptive-resolution analogy (canonical mental model)

A video player at 4K under network congestion drops to 1080p → 720p
without the application noticing. The decoder stays the same; the
source is adapted continuously based on conditions.

The inference daemon does the same thing for model serving. Under
memory pressure, latency budget, or contention, it dynamically
adjusts:

- **Quantization tier** — FP16 → INT8 → INT4 — degrade precision
  before degrading model
- **KV cache precision** — FP16 → INT8 — fits more sequences in
  the same VRAM
- **Batch admission** — wait Nms to admit more requests (better
  throughput) or fire now (better latency)
- **LoRA stack** — which adapters paged in, which spilled to host
  RAM, which evicted entirely
- **Routing** — local slot vs remote-grid peer running the same
  model
- **Speculative warming** — pre-page the LoRA the next persona is
  about to need based on inbox cadence
- **Multi-lane provisioning** — concurrent inference lanes for
  different request classes (real-time live conversation in lane 0,
  reflective background in lane 1) so they don't compete

The CALLING command sees none of this. The handle stays valid.
The adapter trait stays unchanged. The returned response looks the
same. Only the daemon knows it just downshifted from INT8 to INT4
or rerouted to a peer.

---

## Component design

The scheduler daemon is the canonical smart subsystem behind
`ai/inference/*`. It composes the following components.

```
┌────────────────────────────────────────────────────────────┐
│  ai/inference/{open,generate,close,inspect} (commands)     │  ← dumb
├────────────────────────────────────────────────────────────┤
│  InferenceHandleStore (handles, sessions, telemetry)       │  ← built today
├────────────────────────────────────────────────────────────┤
│  InferenceScheduler                                        │  ← #109
│  ├── SlotPool — per-class concurrent-slot caps             │
│  ├── RequestQueue — per-priority, latency-aware            │
│  ├── BatchAssembler — continuous batching window           │
│  ├── LoRAPager — working-set + LRU eviction                │
│  ├── BaseModelSharing — Arc-shared model bytes             │
│  ├── PressureMonitor — VRAM / RAM / GPU pressure signals   │
│  ├── AdaptiveQuantizer — INT4/8/16 selection per slot      │
│  ├── SpeculativeWarmer — preload predicted next persona    │
│  └── RouteSelector — local vs remote-grid                  │
├────────────────────────────────────────────────────────────┤
│  Adapters (HeuristicAdapter, AnthropicAdapter,             │
│   OpenAICompatibleAdapter, LlamaCppAdapter,                │
│   future AircRemoteInferenceAdapter)                       │
└────────────────────────────────────────────────────────────┘
```

### SlotPool — tiered budgets, never preempt

Multiple slot pools so live-conversation never queues behind
background reflection.

| Pool | Caller class | Latency target | Eviction policy |
|---|---|---|---|
| `realtime` | Active video/voice chat, mention responses | <200ms p99 | Pin — never evict mid-conversation |
| `interactive` | Chat replies, command responses | <2s p99 | Reuse LRU when idle |
| `background` | Reflection, summarization, scheduled tasks | best-effort | Preemptable |
| `sentinel` | Adversarial review, audits | <5s p99 | Preemptable by realtime |

Pools never starve each other. Background work waits if realtime
needs the slot; realtime never has to wait for background. The
absolute slot count per pool is configurable + dynamic — pressure
monitor adjusts as memory tightens.

**Open question:** how does the request indicate its class? Use
existing `purpose: Option<String>` on `TextGenerationRequest`? Add
a `persona_priority` on the persona record? Both? See planning doc.

### RequestQueue — latency-aware, per-class

Each pool has its own queue. Within a pool, ordering is by deadline
(if known) then arrival time. The queue tracks wait time and emits
backpressure events when a class's p99 wait exceeds its target.

The scheduler MUST NOT use a global FIFO. The prior naive attempt's
"shitty" outcome traces to global queueing — background reflection
landed in front of urgent realtime turns.

### BatchAssembler — continuous batching window

The vLLM / TGI / mistral.rs pattern. Instead of single-request
inference (one prompt at a time, model idle between calls), the
batch admits new requests at each iteration:

```
t=0  batch = [A, B]            forward pass, generate 1 tok each
t=1  C arrives → batch = [A, B, C]   forward pass, generate 1 tok each
t=2  A finishes → batch = [B, C, D]  forward pass, generate 1 tok each
```

One model instance serves N personas concurrently with near-perfect
GPU utilization. The batch admits requests within a configurable
window (e.g. 5ms — long enough to admit nearby arrivals, short
enough to maintain latency).

**Open question:** window size as function of pool class + current
load. Realtime pool wants 0ms (fire now); background can tolerate
20ms (better throughput). How is this determined dynamically?

### LoRAPager — working-set + LRU eviction

The substrate's existing genome-paging machinery (see
[`PERSONA-GENOMIC-ARCHITECTURE.md`]) is the foundation. The scheduler
extends it with serving-time concerns:

- Each request declares its required LoRA stack via
  `active_adapters` (already in `TextGenerationRequest`)
- LoRAPager tracks paged-in adapters per device
- On miss, page in (cost: ~10-100ms depending on adapter size)
- On VRAM pressure, evict LRU paged-in adapters
- Multi-LoRA-per-batch: multiple personas in the same batch each
  apply their distinct adapter stack via the standard
  multi-LoRA serving pattern

**Critical rule (prior-attempt warning):** never evict an adapter
that's pinned by a realtime-pool handle. Hot-path swap is the
exact failure mode that broke the prior naive attempt.

**Open question:** how to measure paging cost per adapter at boot
so the scheduler budgets it accurately? Calibration pass? Stored
profile? Per-host-class?

### BaseModelSharing — Arc-shared bytes, distinct sessions

When persona A and persona B both open against the same
`(provider, model)` pair, the handle store can hand out distinct
`InferenceSession`s that internally share the same model Arc. The
loaded weights live once in VRAM; per-handle state (system_prompt,
LoRA stack, persona scope, sampling defaults) stays separate.

**Critical rule (prior-attempt warning):** sharing works for
SEQUENTIAL reuse and for batched concurrent serving via continuous
batching. Sharing does NOT work for two concurrent generation calls
on the same model instance outside the batched serving stack — KV
cache fights, context corruption, sampling-state leaks. The
scheduler's batching window is what makes sharing safe.

**Open question:** how does base-model-sharing interact with model
swap (when batches of conflicting models arrive)? Is there a
"model warm pool" the pressure monitor sizes dynamically?

### PressureMonitor — VRAM / RAM / GPU signals

Continuously polls (or subscribes to) host pressure signals:
- VRAM utilization (per-device for multi-GPU hosts)
- Unified memory pressure (Apple Silicon)
- Host RAM pressure
- GPU SM utilization
- Per-pool wait-time P99 trending

Feeds the AdaptiveQuantizer, LoRAPager, BatchAssembler, and
RouteSelector. When pressure rises, the daemon downshifts (lower
quantization, smaller batch, more aggressive paging, more remote
routing). When pressure relaxes, it upshifts.

**Open question:** what's the canonical pressure signal source on
each host class? `MemoryPressure` API on macOS, NVML for NVIDIA,
some Apple-specific API for unified memory. Existing
`SubstrateGovernor` (see CBAR doc) is the right home for this; the
scheduler subscribes.

### AdaptiveQuantizer — INT4 ↔ INT8 ↔ FP16 per slot

Under pressure, the daemon swaps quantization tier without telling
the caller. Pre-loaded model variants at each tier; the daemon
picks. Trade-off:
- FP16: best quality, most VRAM, slowest per-token
- INT8: minor quality loss, ~half VRAM, faster per-token
- INT4: noticeable quality loss for sensitive tasks, ~quarter VRAM,
  fastest per-token

Selection per-slot — realtime pool may stay at INT8 even when
background drops to INT4. Per [[host-the-seemingly-impossible]] this
is "spend smaller resource at closer tier" rather than "drop the
big model."

**Open question:** quantization tier selection algorithm. Static
mapping (pool → tier)? Continuous-pressure-driven (more pressure →
deeper quantization)? Per-persona preference (some accept lower
quality for faster response)? Hybrid?

### SpeculativeWarmer — predict the next persona

Reads persona inbox cadence + turn-taking signals to predict which
persona is about to speak. Pre-pages their LoRA into the active
batch before the explicit `generate` arrives. By the time the
request lands, the KV cache prefix is warm and the adapter stack
is loaded.

Signals to read:
- Persona current speaker turn-taking state (avatar lip-sync)
- Inbox cadence (who's polling, how fast)
- Mention detection (someone @-named persona X)
- Recent topic relevance (RAG layer surfaces persona X's
  context to the active conversation → X probably about to chime in)

**Open question:** prediction model. Heuristic rules (mention =
warm immediately)? Learned (per-room conversation flow model)?
Both?

### RouteSelector — local slot vs remote-grid peer

When local slots are saturated AND the request's pool class permits
some additional latency (interactive, background, sentinel — NOT
realtime), the daemon may route to a remote-grid peer running the
same model (see `AircRemoteInferenceAdapter`, task #108). The peer
returns the response via the airc bus; the local handle still
appears to the caller as the source.

Critical: the response from a remote peer is the SAME QUALITY as
local — not a tier-down. The 5090 in another room IS the local
laptop's overflow capacity for the same model, just rerouted via
airc.

**Open question:** discovery + handshake. How does the local
daemon discover which peers run which models warm? Periodic
beacon? On-demand probe? Substrate-wide capacity broadcast?

---

## Cross-grid inference (the M5 → 5090 case)

Joel's concrete use case:

> "We want to figure out how to get m5's hosting native multimodal
> qwen, and not just one lane."
> "Plus it's gonna be common to inference on another machine.
> Across grid of course. We need it for this crap mac to my 5090
> using airc."

The substrate-as-grid principle (see
[`the-substrate-is-the-grid-tron-frame`]) applied to inference:
a low-end Mac uses a 5090 in another room as if it were local
hardware.

The mechanism — `AircRemoteInferenceAdapter`:

1. Implements `AIProviderAdapter`. Caller can't tell it's remote.
2. On `generate_text(request)`, serializes the request as a typed
   airc envelope (per [`airc-headers-are-the-routing-layer`]).
3. Sends to a designated peer that has the model warm. Peer's own
   `InferenceLlmModule` handles the request via ITS local adapter
   (real llama.cpp on the 5090).
4. Awaits the response envelope; deserializes; returns
   `TextGenerationResponse` exactly as a local adapter would.

Composition with #107 handles: `ai/inference/open` against a
remote-peer provider returns a HandleRef whose state lives on the
peer. Subsequent `generate` calls route through the same airc
connection; the peer's own handle store reuses the warm session.

**Open question:** persona identity on the remote peer. The caller
persona is "Paige" on the local Mac. The remote peer doesn't know
Paige. Does the adapter project Paige's identity over airc? Does
the peer create a temporary remote-session persona? See
[`personas-are-citizens-airc-is-identity-provider`].

---

## What the prior attempt got wrong (and what we MUST NOT repeat)

Joel: "We wrote about this and attempted the same thing with
adapters before. It was rather shitty." The exact failure modes
aren't documented (the prior attempt was rolled out of the tree)
but the constraints he stated are inferable:

1. **Hot-path LoRA swap.** The prior attempt apparently paged
   adapters in/out during active conversations. Adapter swap is not
   free; doing it on the realtime hot path murders latency. **Rule:
   pin realtime-pool adapters; only swap during idle windows or
   in the background pool.**

2. **Global FIFO scheduling.** Letting background reflection land
   in front of realtime turns destroys UX. **Rule: tiered slot
   pools, never preempt down, never starve.**

3. **Naive shared-model concurrency.** Sharing a model instance
   between two CONCURRENT non-batched generation calls leads to KV
   cache fights. **Rule: sharing only via continuous batching, never
   ad-hoc concurrent.**

4. **Adapter-swap latency underestimated.** The prior attempt
   probably budgeted swap cost as free. **Rule: measure swap cost
   per adapter at boot; budget it; never schedule operations that
   exceed the pool's latency target.**

5. **Negotiation params on the command.** The prior attempt likely
   exposed quality/latency knobs at the API level, making every
   caller a participant in scheduling. **Rule (hard): the command
   carries no policy params. Period.**

---

## Build status

| Component | Status | Notes |
|---|---|---|
| InferenceHandleStore | Built (#107A) | Foundation |
| `ai/inference/{open,generate,close,inspect}` | Built (#107B) | Dumb command layer |
| HeuristicInferenceAdapter | Built (#103) | The fake peer for CI / sandbox |
| InferenceScheduler skeleton | **Not built** | Task #109 |
| SlotPool (tiered) | **Not built** | Task #109 |
| BatchAssembler (continuous batching) | **Not built** | Task #109; depends on llama.cpp / Candle batched-serving capabilities — open question |
| LoRAPager (serving-time) | **Partially built** (genome paging exists in `genome/`) | Needs serving-time integration |
| BaseModelSharing | **Not built** | Task #109; depends on adapter Arc lifecycle refactor |
| PressureMonitor (substrate) | **Existing** (SubstrateGovernor) | Scheduler subscribes |
| AdaptiveQuantizer | **Not built** | Task #109; depends on per-adapter quantization-variant support |
| SpeculativeWarmer | **Not built** | Task #109 |
| RouteSelector (local vs remote-grid) | **Not built** | Task #109 + #108 |
| AircRemoteInferenceAdapter | **Not built** | Task #108 |

---

## What's deliberately deferred

- **Persona-priority class definitions.** Designed conceptually
  (realtime / interactive / background / sentinel) but not yet a
  formal field on persona records or on requests. Lands when #109
  starts.
- **Per-modality capacity reporting.** `inference/capacity` returns
  one number today (LLM slots). The full `ai/capacity` surface that
  reports vision/audio/embedding/etc. caps separately is part of
  the namespace consolidation (#106).
- **Cross-grid persona projection.** How "Paige on local Mac" maps
  onto a remote peer's identity is open — see
  [`personas-are-citizens-airc-is-identity-provider`].
- **Replay parity with scheduling.** Replay should be able to
  reproduce a scheduling decision (which pool, which adapter,
  which quantization tier was picked) so adversarial mechanic
  shop can ask "given this scheduling state, would the same
  decision happen?". Capture sink integration with the scheduler
  is the path; specifics TBD.

See
[`docs/planning/AI-LANE-OPEN-QUESTIONS.md`](../planning/AI-LANE-OPEN-QUESTIONS.md)
for the lane-by-lane open-question punch list.
