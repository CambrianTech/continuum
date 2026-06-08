# The `ai/*` Command Namespace — Substrate AI/ML Surface

> Every AI/ML thing the substrate hosts — LLMs, vision, audio,
> embeddings, classical ML, planning algorithms, game/agent AI,
> low-level GPU kernels — sits under one command namespace,
> behind one adapter pattern, with one handle abstraction. The
> commands are stable + narrow. The intelligence lives in the
> daemons behind them.

**Status:** Architecture (2026-05-31). Partial implementation:
inference handles + heuristic adapter ship today; namespace
consolidation under `ai/*` is task #106.

**Parents:**
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md) — substrate runtime contract
- [`MODULE-ARCHITECTURE.md`](MODULE-ARCHITECTURE.md) — module + command shape
- [`EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md`](EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md) — inclusivity thesis

**Siblings:**
- [`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md) — the daemons behind the commands
- [`COGNITION-ALGORITHMS.md`](COGNITION-ALGORITHMS.md) — what the AI surface serves

---

## The thesis stated plainly

The substrate hosts a vast AI/ML surface — not just LLMs, but
classical-ML classifiers, planning algorithms, game/agent AI, vision
CNNs (YOLO and friends as multimodal crutches), audio DSP, low-level
GPU kernels. All of it must be addressable by personas, sentinels,
human commands, and other peers through the same shape of interface.

Joel, 2026-05-31:

> "Need ai commands in their own section. Have a lot of stuff even
> ml cnns like yolo etc for the multimodal crutches (models lacking
> multimodal use these in rag). ... We have a lot of ai stuff.
> Classifiers other random ml. Audio image, video game stuff. Low
> level, algs."

And the architectural rule:

> "Yeah the inference command doesn't do this. It's smart subsystems
> and daemons. Commands are dumb and short."
> "So the commands absolutely cannot negotiate this."

The namespace + the adapter polymorphism + the handle pattern are the
substrate's way of giving every AI/ML modality the same shape, so a
persona's view of "inference" is uniform regardless of whether
it's calling a 70B LLM, a YOLO classifier, a behavior tree, or a
DSP filter.

---

## Namespace tree

All AI/ML commands live under `ai/*`, organized by modality.
Illustrative; names finalize as each lane ships.

```
ai/
├── inference/                # LLM-class workloads (text + multimodal LLMs)
│   ├── open                  # → InferenceHandle (HandleRef)
│   ├── generate              # uses handle, reuses session
│   ├── close                 # release the session
│   ├── inspect               # observability snapshot
│   └── capacity              # host concurrent-slot count, per-modality caps
│
├── vision/                   # CNN / classifier / detector / segmenter
│   ├── classify/{open,run,close,inspect}
│   ├── detect/{open,run,close,inspect}         # YOLO + friends
│   ├── segment/{open,run,close,inspect}
│   └── describe/{open,run,close,inspect}       # multimodal bridge → text
│
├── audio/                    # STT / TTS / sound-event classifier
│   ├── transcribe/{open,run,close,inspect}     # STT
│   ├── synthesize/{open,run,close,inspect}     # TTS
│   └── classify/{open,run,close,inspect}       # sound events
│
├── embedding/                # text + multimodal embeddings
│   └── generate              # (often one-shot; handle pattern optional)
│
├── ml/                       # classical ML / non-NN models
│   ├── classify/{open,run,close}              # logistic regression, RF, SVM
│   ├── regress/{open,run,close}
│   └── cluster/{open,run,close}                # k-means + friends
│
├── alg/                      # classical algorithms (deterministic)
│   ├── search                # A* / D* / MCTS
│   ├── plan                  # planning / scheduling
│   └── optimize              # gradient-free + grad-based optimizers
│
├── game/                     # agent AI / game-shaped tasks
│   ├── behavior              # behavior tree / decision tree evaluation
│   ├── path                  # pathfinding
│   └── sim                   # predictive simulation
│
└── lowlevel/                 # building blocks
    ├── gpu-kernel
    ├── tensor                # tensor ops
    └── dsp                   # DSP filters / FFT / convolution
```

### Why one tree

1. **Discoverability.** Any persona browsing `ai/*` sees every model
   the substrate can run, every adapter that's wired in, every
   modality that's available.
2. **Composability.** RAG can pull from `ai/vision/describe` the
   same way it pulls from `ai/inference/generate`. Sources stay
   modality-agnostic.
3. **Routing.** `ai/capacity` answers "how many concurrent vision
   jobs can I run?" the same way it answers the question for LLMs.
   One pressure-aware allocator, multiple modalities.
4. **Doctrine alignment.** The adapter-trait + handle-pattern
   apply uniformly. The fake/heuristic peer adapter pattern
   ([`inference-is-an-adapter`]) scales — every modality gets a
   stub for CI / sandbox / replay use.

### Multimodal crutches are first-class

CNN-based vision/audio classifiers that bridge text-only LLMs to
sensory parity are NOT utilities — they're first-class peers in
`ai/*`. A `gemma-2b` persona "sees" via `ai/vision/describe` →
text → RAG. Without these crutches as namespace peers, the
inclusivity doctrine breaks at the modality boundary.

---

## The three universal primitives

Every modality in `ai/*` is built on the same three architectural
primitives the substrate already established for the inference lane.

### 1. Adapter polymorphism (OpenCV-style)

Each modality has a trait + a registry + many concrete impls:
- Real impls (local Candle, llama.cpp, cloud APIs, native vision
  libraries)
- Fake / heuristic impls (deterministic stand-ins for CI / sandbox /
  replay, registered as production peers per
  [`inference-is-an-adapter`])
- Remote-grid impls (route to a peer machine over airc; same trait,
  remote execution — see [`INFERENCE-SCHEDULING-AND-SCARCITY.md`]
  §"Cross-grid")

The trait is the contract. Callers don't care which impl handles
the work. This is the substrate's universal OOP rule applied at the
modality layer.

### 2. Handle pattern (establish once, reuse many)

For any modality where setup is expensive (model load, GPU memory
allocation, classifier weights, behavior-tree compilation), the
caller opens a handle once and threads it through many `run` /
`generate` calls. Cold handles get LRU-evicted under memory
pressure. Same shape as [`cell-processor-command-runtime`] handles
elsewhere in the substrate.

```rust
// Pattern is identical across modalities:
let handle = Commands.execute('ai/inference/open', { provider, model, ... });
// Many times:
let r = Commands.execute('ai/inference/generate', { handle, request });
// Eventually:
Commands.execute('ai/inference/close', { handle });
```

### 3. Capture + replay (observability is half the architecture)

Every load-bearing decision (which adapter picked, which handle
warm, what was the prompt, what came out) emits structured capture
events through an opt-in sink. JSONL traces + the
`Replay<Modality>Source` shape let any AI (Claude, a sentinel
persona, the persona itself) honestly inspect "what would I see
right now?" and "what would I say given that?". Default sink is
Noop (zero overhead). See
[`OBSERVABILITY-AS-SUBSTRATE.md`](OBSERVABILITY-AS-SUBSTRATE.md).

---

## Commands are dumb, daemons are smart

The most important architectural rule for `ai/*` — Joel, 2026-05-31:

> "Yeah the inference command doesn't do this. It's smart subsystems
> and daemons. Commands are dumb and short."
> "So the commands absolutely cannot negotiate this."

What this means in practice:

| The command does | The daemon does |
|---|---|
| Parse the envelope | Decide which slot / lane handles the request |
| Validate the handle | Coordinate continuous batching across concurrent requests |
| Look up the adapter | Page LoRA layers in / out based on the working set |
| Call the adapter / store | Dynamically adjust quantization tier under memory pressure |
| Materialize the result | Route to a remote-grid peer when local is saturated |
| Emit capture events | Speculatively warm the next persona's adapter |
| Return | Reuse base model bytes across personas |

The command surface stays stable as the daemons grow arbitrarily
smart. Adding sophistication never breaks callers. This is the
substrate's universal narrow-interface / rich-implementation OOP
rule. See [`INFERENCE-SCHEDULING-AND-SCARCITY.md`] for the
inference daemon's design; equivalents exist (or will exist) per
modality.

### Hard rule: commands carry no policy params

Don't add `max_latency_ms`, `min_quality_tier`, `prefer_local`,
or similar negotiation knobs to the command. Hints flow through
metadata that ALREADY exists in the request (`persona_id`,
`purpose`, `request_id`) and the daemon reads them. Baking policy
into the command surface is the exact mistake that defeated the
prior naive attempts.

---

## What the namespace doesn't do

- **No tiering down.** The same good model serves every persona;
  reuse harder via continuous batching, multi-LoRA per pass, prefix
  dedup, speculative decoding — never by routing background work to
  a dumber model. See [`HOST-THE-SEEMINGLY-IMPOSSIBLE.md`] (and the
  scheduling doc below) for what cleverness this requires.
- **No client-side scheduling.** Callers don't know about slot
  contention or memory pressure. They just call. The daemon
  decides everything.
- **No per-call quality params.** The substrate picks adaptively
  per the adaptive-resolution model (see scheduling doc).
- **No model-stack negotiations at the surface.** LoRA selection,
  base-model reuse, KV cache sharing all happen inside the daemon.

---

## Build status

| Component | Status | File / task |
|---|---|---|
| Adapter trait + registry | Built | `core/continuum-core/src/ai/adapter.rs` |
| Heuristic / canned adapter | Built | `core/continuum-core/src/ai/heuristic_adapter.rs` (#103) |
| Anthropic / OpenAI-compatible adapter | Built | `core/continuum-core/src/ai/{anthropic,openai}_adapter.rs` |
| LlamaCpp adapter | Built | `core/continuum-core/src/inference/llamacpp_adapter.rs` |
| Inference handle store | Built | `core/continuum-core/src/inference/handle_store.rs` (#107A) |
| `ai/inference/{open,generate,close,inspect}` commands | Built | `core/continuum-core/src/inference/handle_module.rs` (#107B) |
| One-shot legacy `inference/llm/request` | Live (back-compat) | `core/continuum-core/src/inference/llm_module_service.rs` |
| Namespace consolidation under `ai/*` | Pending | Task #106 |
| InferenceScheduler daemon | Designed, not built | Task #109; see scheduling doc below |
| Vision / audio / classical-ML / alg / game / lowlevel commands | Mostly in TS today | Migrate over time per [`rust-is-the-core-node-is-the-shell`] doctrine |
| AircRemoteInferenceAdapter (cross-grid) | Designed | Task #108; see scheduling doc §"Cross-grid" |
| `rag-inspect` ServiceModule | Pending | Task #100 |

---

## Open architectural questions

These don't have answers yet. See
[`docs/planning/AI-LANE-OPEN-QUESTIONS.md`](../planning/AI-LANE-OPEN-QUESTIONS.md)
for the lane-by-lane punch list:

- Per-modality capacity reporting — how does `ai/capacity` express
  caps for vision vs audio vs LLM vs classical-ML?
- Cross-modality scheduling — when LLM + vision compete for the
  same GPU, who decides?
- Handle TTL / LRU policy per modality
- Adapter discovery / advertising — how does a persona discover
  what's available on the local host vs across the grid?
- Per-modality LoRA / fine-tune state (LLMs use LoRA; classical ML
  uses weight checkpoints; how does the substrate abstract this?)
- Replay parity across modalities — does ReplayVisionSource look
  identical to ReplayRagSource?
- Metadata flow for daemon scheduling decisions — what fields does
  every request carry to inform the daemon?
