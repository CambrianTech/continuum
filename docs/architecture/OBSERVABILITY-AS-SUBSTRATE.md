# Observability As Substrate

> Roughly half the substrate's surface area is structured capture
> of load-bearing decisions. That's correct, not bloat. Sophisticated
> behavior is unanalyzable any other way. Every module ships a
> CaptureSink companion to its primary trait. Default is Noop
> (zero hot-path overhead). The opt-in sinks are how AIs inspect
> their own prompts, how mechanics debug, how replay reproduces.

**Status:** Doctrine (2026-05-31). Reference implementation:
`RagCaptureSink` family in `src/workers/continuum-core/src/persona/rag_capture.rs`.

**Parents:**
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md)
- [`AI-COMMAND-NAMESPACE.md`](AI-COMMAND-NAMESPACE.md)

---

## The thesis stated plainly

Joel, 2026-05-31:

> "Yeah we need half the architecture just debug features. I don't
> know how to do anything sophisticated any other way. We don't
> slow anything down of course, but it's easiest to answer what is
> wrong when you're observing the actual inputs and outputs."

> "This is the differentiator between a complex guess and an
> intentional brain. If we have observability and replay at any
> stage, we can iterate, improve, add complexity, try out new
> ideas in realistic scenarios and look at it ourselves: with this
> prompt would I respond as it requests at this step? Which layer
> is broken? Missing, is this contextually relevant (hippocampus
> and caches)?"

Three canonical introspection questions fall out of this:

1. **Counterfactual evaluation** — "with this prompt would I
   respond as it requests at this step?" Requires full prompt
   visible + replay against the same (or different) model.
2. **Fault isolation** — "which layer is broken?" Requires per-layer
   deliveries clearly delineated.
3. **Relevance assessment** — "is this contextually relevant?"
   Requires scoring rationale, marginal-next-item hints, drop
   reasons.

Without these, model bugs and substrate bugs are
indistinguishable. With them, the substrate becomes an intentional
brain instead of a complex guess.

---

## The pattern

Every module with a load-bearing decision ships a
`<Module>CaptureSink` trait alongside its primary trait. The shape
is the same every time:

```rust
// 1. Enum of capture events the module emits
pub enum FooCaptureEvent {
    StageA { captured_at_ms: u64, persona_id: Uuid, ... },
    StageB { ... },
    StageC { ... },
}

// 2. The sink trait
pub trait FooCaptureSink: Send + Sync {
    fn record(&self, event: FooCaptureEvent);
}

// 3. Three concrete impls always ship
pub struct NoopFooCaptureSink;        // zero-cost default
pub struct JsonlFooCaptureSink { ... } // file-backed for replay
pub struct InMemoryFooCaptureSink { ... } // for tests + introspection

// 4. A decorator wraps the primary trait
pub struct RecordingFoo<F: Foo> {
    inner: F,
    sink: Arc<dyn FooCaptureSink>,
}
impl<F: Foo> Foo for RecordingFoo<F> {
    fn do_thing(&self) -> Output {
        self.sink.record(FooCaptureEvent::StageA { ... });
        let result = self.inner.do_thing();
        self.sink.record(FooCaptureEvent::StageB { ... });
        result
    }
}
```

The hot path holds `Arc<dyn FooCaptureSink>`. The sink's `record()`
is a virtual call; the Noop impl reduces to a no-op. **Never branch
in the caller on "is observability enabled?"** — let the sink
decide. The caller's code path is identical in production
(Noop sink) and during introspection (JSONL sink).

---

## What constitutes a load-bearing decision

The bar is "if a future reviewer asks why a module behaved this
way on a specific input, the capture trace must answer that
question without re-running the code."

Examples:
- **Admission gates** — what got admitted as engram, what got
  dropped + which criterion fired, what salience curve was assigned
- **Allocators (RAG L1 budget)** — final allocation per source +
  state (Satisfied / FloorOnly / Dropped / UnderProvisioned) +
  escalation flags + warnings
- **Sources (RagSource, future VisionSource, …)** — items
  delivered, tokens used, continuation cursor state, scoring
  rationale per item
- **Schedulers (#109)** — which pool the request landed in, which
  slot served it, what quantization tier, batch composition,
  routing decision (local / remote-grid), wait time
- **Inference adapters** — adapter chosen, model loaded, LoRA
  stack applied, prompt hash, response hash, latency
- **Personas (cognition turn)** — full turn capture per
  [`persona-record-replay-is-a-product-requirement`]

If your module makes a decision and you can't, after the fact,
explain why from the capture trace alone, the trace is
insufficient — add scoring rationale, drop reasons,
marginal-next-item hints, whatever's needed. The bar is
mechanic-grade.

---

## The Noop default is non-negotiable

The production hot path pays zero for observability it didn't ask
for. Concretely:

- The Noop impl's `record()` is `#[inline]` and empty.
- No allocations on the production path (the Noop sink doesn't
  touch the event struct's owned strings — it drops the argument).
- The branch on sink type happens at sink construction (which is
  before any hot work), not per `record()` call.
- Tests should assert this on the next slice that touches each
  module. The discipline is permanent.

If a perf regression traces to observability, the right fix is
NEVER "skip the sink on the hot path" — it's "the Noop sink isn't
actually noop, fix it."

---

## How AIs use it

Captured data is the substrate's truth for that decision. Tests,
mechanic-shop introspection, AIs analyzing other AIs, replay
adversarial review, training fixtures all read the same trace
format. Don't fork "debug output" and "telemetry" and "test
fixtures" — one stream, multiple consumers.

The canonical introspection question every AI inspecting another
AI's behavior asks:

> "What did the model actually see, and would I respond the same way?"

The capture answers the first half (look at the trace, see the
exact prompt). Replay against a model answers the second half
(feed the captured prompt back, see the response, compare). Both
sides are first-class substrate primitives.

---

## Introspection must be reachable as a command

Files on disk are for replay-after-the-fact. **Commands are how
other AIs reach observability live.** When a sentinel persona is
reviewing Paige's turn, it doesn't `cat | jq` — it calls
`Commands.execute('persona/rag-inspect', { persona: 'Paige' })`
and gets back structured data immediately.

Module-level inspection commands should be normal additions to
the `<module>/*` namespace. The future:

- `ai/inference/inspect` — handle observability snapshot
- `persona/rag-inspect` — RAG layer state (#100)
- `cognition/state` — current cognitive load
- `genome/working-set` — paged-in adapters
- `scheduler/queue` — current pool queue depth + wait times

Each is a thin wrapper over the same in-memory state the JSONL
sink captures. The file format and the command shape return the
same data, just to different audiences (humans replay files; AIs
call commands).

---

## What the doctrine forbids

- **Conditional observability.** Code like `if log_enabled { …
  capture … }` is wrong. The sink decides; the caller doesn't
  guard.
- **String-based debug logs as the truth.** `tracing::debug!` is
  fine for narration, but the truth a future reviewer needs lives
  in the structured capture, not in unstructured log strings.
- **Per-module unique formats.** Every capture sink follows the
  same trait shape so consumers (replay, mechanics, sentinels)
  don't relearn the format per module.
- **Silent truncation.** If a sink can't write the full event
  (file full, IPC backpressure), it emits a typed dropped-event
  marker — never just drops on the floor.
- **Heavy-handed hot-path work.** Capture sinks at production-mode
  Noop must be free; the JSONL impl writes asynchronously through
  a small buffer; the InMemory impl uses a bounded ring.

---

## What's built and where

| Reference | File |
|---|---|
| `RagCaptureSink` trait + Noop / JSONL / InMemory | `src/workers/continuum-core/src/persona/rag_capture.rs` |
| `RecordingRagSource` decorator | same file |
| `ReplayRagSource` (the consumer side) | `src/workers/continuum-core/src/persona/rag_replay.rs` |
| Capture-aware inspection (deep introspection mode) | `src/workers/continuum-core/src/persona/rag_inspect.rs` |
| Demo binary that exercises the loop end-to-end | `src/workers/continuum-core/src/bin/airc_rag_demo.rs` |

---

## What's not yet built (per doctrine compliance)

- **Admission sink.** AdmissionState doesn't yet emit capture
  events. Engram-side observability gap; needs a slice to add
  `AdmissionCaptureSink` shape mirroring the RAG side.
- **Scheduler sink.** InferenceScheduler (#109) ships with capture
  from day one — no retrofitting tolerated. The slice that lands
  the scheduler MUST land the sink in the same PR.
- **Inference-call sink.** `ai/inference/generate` doesn't yet
  emit per-call capture (adapter chosen, prompt hash, response
  summary). Add as part of #109 or earlier as standalone.
- **Cognition sink.** PersonaCognition's turn loop has partial
  capture (RagAssemblySeed exists) but the full
  ConsolidatedInboxChunk → DecideTurn → Generate → Replay loop
  doesn't yet record every decision. Task #56.
- **Multi-sink composition.** Today a sink is one of Noop / JSONL
  / InMemory. Composing (JSONL + IPC publish + InMemory) is open
  question Q15 in the AI-lane open-questions doc.

---

## When to violate

Don't.

If a slice ships a module without observability hooks, that slice
is incomplete. The follow-up is sized into the same task. The
doctrine survives because it's never optional.
