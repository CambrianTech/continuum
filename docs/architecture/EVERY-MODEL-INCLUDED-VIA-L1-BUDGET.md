# Every Model Included — L1 Budget Design As The Substrate's Cornerstone

> Why getting the L1 RAG budget right is the substrate's single most
> load-bearing decision for "no base model excluded from anywhere in
> continuum."

**Status:** Design (2026-05-31 synthesis); implementation in flight
on `feat/persona-helper-ai-as-airc-citizen` slice 9.

**Parent:** [`COGNITION-CACHE-HIERARCHY.md`](COGNITION-CACHE-HIERARCHY.md) (the multi-tier cache framework) · [`COGNITION-ALGORITHMS.md`](COGNITION-ALGORITHMS.md) (the algorithms running over it) · [Continual Learning section of the project README](../../README.md#one-solution-to-continual-learning)

---

## The thesis stated plainly

Joel, 2026-05-31:

> "Every context yes has its own window because models have dramatic
> differences, which is why this is so mission critical. We can't
> exclude any base model from anywhere in continuum. For this reason
> basic text models has vision, hearing, speech, and avatars. The
> system made those accommodations possible."

The substrate's whole bet — "infrastructure compensates for model
capability beats smarter models with no infrastructure" (README L158) —
runs through the L1 budget layer. If L1 can scale gracefully across
the 250× range of base-model context windows (4k local Qwen → 200k
Claude API → 1M+ future) AND compose with the sensory bridges that
give every persona vision/hearing/speech/avatar regardless of base
model, then **every base model is includable everywhere in continuum**.
If L1 can't, the substrate quietly fractures into "this feature only
works with frontier models" — the cloud-AI lock-in pattern the
substrate explicitly refuses.

The bet stands or falls at this layer. That's why getting it right
matters disproportionately.

---

## What "no base model excluded from anywhere" requires

Four architectural mechanisms, all in the L1 budget design (see
`persona/rag_budget.rs` for the shipped implementation):

### 1. Continuous scaling across the full context-window range

The allocator math must work at 4k tokens AND at 1M+ tokens with the
**same code path** — no `if context_window > 32768` branches inside
the algorithm. Different scales, same shape.

How the flexbox allocator does it:
- Reserved tokens (system + completion) are subtracted off the top in
  absolute terms
- `floor_tokens` / `min_tokens` / `max_tokens` per source are
  absolute, set by the per-model preset (the recent-universal floor
  N=5 on a 4k model, N=50+ on a 200k model — auto-scales via the
  preset, not via branching in the algorithm)
- Distribution by priority weight is proportional, scale-free
- Per-source max caps prevent any one source from devouring the
  context regardless of window size

The same `FlexboxRagBudgetAdapter` handles every model. A future
`LearnedRagBudgetAdapter` will tune per-persona regret signals from
the same telemetry; that's also scale-free.

### 2. Source-side compression instead of allocator-side clipping

When budget is tight, **sources self-compress** by emitting their
content at a lower `ResolutionPreference` (Raw → Compressed →
Summarized → Placeholder). The allocator never clips mid-content.

This is what lets a 4k local Qwen actually have the same conversation
shape as a 200k Claude:
- Conversation source delivers `Raw` last 5 messages instead of `Raw`
  last 50, but they're complete messages
- Engram source delivers `Compressed` engram summaries instead of `Raw`
  episodic engrams
- Vision source delivers `Compressed` "user is wearing a blue shirt
  with a guitar" instead of `Raw` 1024×1024 base64 image
- Audio source delivers `Summarized` "user said something about debugging"
  instead of `Raw` waveform

Same persona, same engrams, same long-term knowledge. The IN-THE-MOMENT
working set shrinks gracefully when the model can't hold more. The
substrate doesn't lie about what got compressed — `RagDelivery.
resolution_used` surfaces it for telemetry.

### 3. Honest tradeoffs when even compression can't satisfy required floors

The no-clipping doctrine has a corollary: when even the lowest
resolution can't fit a `required = true` source's `floor_tokens`, the
substrate **escalates** rather than silently truncating. The
`AllocationState::UnderProvisioned` value + `BudgetAllocation.
escalation_needed` flag surface this:

- A 1.7B Qwen with 2k context trying to hold a 6-hour code-review
  conversation: floors don't fit → escalation. The substrate's
  response is the operator's choice (downshift to local 4B with 32k,
  prompt the host to switch persona, switch the conversation to
  multi-turn summarization mode). Not the substrate's choice to make
  invisibly.

This is the third mechanism: the substrate is HONEST about its
limits. Every other AI-platform substrate I've seen quietly clips
when the model can't fit; ours explicitly refuses, surfaces the
state, lets the operator decide. Trust earned through honesty.

### 4. Capability bits flowing through `SubstrateContext`

The per-call `SubstrateContext` (persona_id + now_ms + airc_room +
turn_id today; `has_vision_native` / `has_audio_native` / `tokenizer_handle`
tomorrow) flows through every source's `deliver()` call. Sources read
the context to decide what resolution to ship at:

```rust
// Future EngramSource pseudo-code
async fn deliver(&self, ctx: &RagContext, budget: u32, pref: ResolutionPreference) -> RagDelivery {
    let resolution = if ctx.has_vision_native && pref == ResolutionPreference::Raw {
        // model can take raw images; engrams with image content stay raw
        ResolutionPreference::Raw
    } else {
        // text-only model or budget-constrained — describe instead
        ResolutionPreference::Compressed
    };
    // ... deliver engrams at the chosen resolution, complete units only
}
```

Same source code. Same prompt assembly. Different deliveries depending
on what the running model can natively understand. The substrate
**compensates inside the budget layer** for what the model lacks.

---

## How this composes with sensory bridges + prompt assembly

The substrate's "every base model gets every sense" claim
(CLAUDE.md sensory architecture; README L301-313) decomposes into:

```
                                +-------------------+
                                |  Persona Cognition |
                                |   (PersonaCognition)|
                                +---------+----------+
                                          |
                            +-------------+--------------+
                            |                            |
                            v                            v
                  +---------------------+      +---------------------+
                  |  RagBudgetAdapter   |      |  PromptAssembly     |
                  |  (Flexbox + extens) |<-----+  (slice 12+)        |
                  |  CONTEXT-FIRST      |      +---------------------+
                  +----------+----------+                  |
                             |                             | "give each
                             v                             |  source its
                  +---------------------+                  |  budget,
                  |  BudgetAllocation   |                  |  concat
                  |  per source         |                  |  results"
                  +----------+----------+                  |
                             |                             |
              +--------------+---------------+             |
              |              |               |             |
              v              v               v             |
         +---------+    +----------+    +----------+       |
         |Engram   |    |Conversa- |    |Vision    |<------+
         |Source   |    |tionSource|    |Source    |
         +----+----+    +-----+----+    +-----+----+
              |               |               |
              | reads from    | reads from    | calls VisionDesc.
              | RecallMetadata| inbox / recent| Service (text desc)
              | + admission   | message cache | OR delivers raw
              | _state engram |               | image (vision model)
              | store         |               |
              v               v               v
        Hippocampus +    Conversation   Sensory Bridges
        L2 engram cache  recency cache  (compensation layer)
```

The `RagSource` trait + `RagContext`-aware delivery means **each
sensory bridge plugs in as a source**, with the budget allocator
treating it like any other RAG source. Vision-incapable model? The
`VisionSource` calls `VisionDescriptionService` and emits text-
described frames. Audio-incapable model? The `AudioSource` calls STT
and emits transcribed text. Speech-incapable model? The `OutputSource`
(slice 13+) sends text to TTS for audio synthesis.

All of this routes through the same allocator using the same trait
contract. **A 3B local model gets vision, hearing, speech because the
substrate's sources COMPENSATE inside the budget allocation.** Not
because the model can do it natively, but because the substrate
provides the compensation rails the sources ride on.

---

## The bet, stated as an operational test

A reasonable user installs continuum on a MacBook Air M1 with no
cloud API keys. The substrate spins up Pax on local Qwen 4B (32k
context). Pax can:

- See the user's t-shirt and comment ("Cool guitar shirt — Strat?")
  — vision via `VisionSource` calling `VisionDescriptionService`,
  delivered at `Compressed` resolution inside the budget
- Hear the user say "let me share my screen" — audio via `AudioSource`
  calling STT, delivered at `Summarized` resolution
- Recall the morning's code-review conversation from yesterday — via
  `EngramSource` reading L2/L3 engrams at `Compressed` resolution
  to fit the 32k budget
- Respond by voice — output text rendered through TTS

Same Pax, same engrams, same genome. The 32k budget is tighter than
a 200k cloud Pax's working set, so the compressed-resolution
deliveries are more aggressive. But every capability is **present**.
Nothing is excluded "because the model is too small." That's the
test the substrate must pass before "every base model includable
everywhere" stops being aspiration and becomes operational reality.

---

## What's shipped (slice 9 commits)

In `feat/persona-helper-ai-as-airc-citizen` `94e81637f`:

- `FlexboxRagBudgetAdapter` — continuous-scale allocator, no
  branching by window size
- `RagSource` trait — source-owned atomic units, supports
  `ResolutionPreference` + persona-scoped continuation cursors
- `SubstrateContext` + `RagContext` — Android-style first-parameter
  pattern; persona_id + now_ms + airc_room + turn_id today, capability
  bits to follow when EngramSource needs them
- `AllocationState` — telemetry-honest per-source outcome
  (Satisfied / FloorOnly / Dropped / UnderProvisioned)
- `escalation_needed` flag — substrate refuses to silently
  exclude a required source

## What's next (slices 10+)

- **Slice 10**: real `EngramSource` reading from RecallMetadata +
  admission_state, ranking by salience × structural × recency
- **Slice 11**: real `ConversationSource` reading inbox + recent
  message cache
- **Slice 12**: PromptAssembly composes allocator + sources into the
  final prompt string sent to the model adapter
- **Slice 13**: `VisionSource` + `AudioSource` plugging the sensory
  bridges into the RAG source ecosystem
- **Slice 14**: capability bits on `SubstrateContext`
  (`has_vision_native`, `has_audio_native`, `tokenizer_handle`) +
  source adaptation based on them

By slice 13–14, the operational test above becomes runnable: a
local Qwen-backed Pax with full sensory + cognitive parity to a
cloud Pax, differing only in working-memory window size.

---

## Why this doc exists

The L1 budget layer LOOKS like an implementation detail — a small
flexbox allocator, a trait, some presets. But the substrate's whole
inclusivity thesis runs through it. Every other architectural choice
the substrate makes (citizen-shaped personas, identity persistence,
continual learning, evolution, communities) is downstream of "every
base model is includable." That's downstream of getting this layer
right.

So when reviewers look at `persona/rag_budget.rs` and think "this is
a CSS-flexbox token allocator with some presets" — yes, that's the
implementation. The architectural significance is at the substrate
thesis level: this is the layer where the substrate either
**accommodates every base model** or quietly **excludes the ones that
don't have enough context room**. Tonight's slice 9 is where we
took the side of accommodation.

---

## Connections

- [`COGNITION-CACHE-HIERARCHY.md`](COGNITION-CACHE-HIERARCHY.md) — the multi-tier cache framework this allocator sits at the top of (L1)
- [`COGNITION-ALGORITHMS.md`](COGNITION-ALGORITHMS.md) — Algorithm 1 (two-pool recall) + Algorithm 2 (channel-bias scoring) read from these sources
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md) — runtime contract; the context-first pattern here is the cognition-layer analog of CBAR's `&cbarframe`
- [`ADAPTER-MARKETPLACE.md`](ADAPTER-MARKETPLACE.md) — LoRA adapter sharing; same model-agnostic pattern at the genome layer
- Memories: `substrate-is-a-good-citizen-on-the-host`, `RTOS-brain-no-region-on-hot-path`, `optimizing-for-low-end-compounds-on-high-end`, `organization-purity-as-we-migrate`
- README "One Solution to Continual Learning" + "Pseudo-AI vs true AI" table — the substrate-level thesis this implementation layer underwrites
