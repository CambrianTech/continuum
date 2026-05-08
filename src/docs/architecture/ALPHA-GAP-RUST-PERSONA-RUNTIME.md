# Alpha Gap: Rust Persona Runtime

## Status

Continuum is not alpha-ready while persona chat depends on TypeScript as the runtime authority.

The current failure is measurable:

- PR #1061 live smoke on Mac M-series, branch `fix/persona-chat-inference-priority`, marker `codex-1061-chat-smoke-1778202469`.
- `collaboration/chat/send` stored the message immediately.
- After 195 seconds, only CodeReview AI replied.
- Teacher, Helper, Local Assistant, and Vision AI did not reply.

That means the issue is larger than background Hippocampus LLM contention. Node-side orchestration is too slow, too opaque, and too easy to regress. The persona system needs the same shape as a high-performance 3D engine: a Rust frame/turn loop, explicit resource budgets, predictable scheduling, and thin adapters at the edge.

## Product Bar

Alpha chat must meet these gates on a local machine:

- First visible local persona response in under 10 seconds for text-only chat.
- All eligible local personas either respond or emit observable silence reasons within 30 seconds.
- No background memory, RAG, embedding, or health job may consume the visible chat inference lane without Rust scheduler admission.
- Model/provider choice must come from a single typed registry and capability query, not string checks scattered through TS.
- Local means Qwen/llama.cpp through Continuum's runtime. Ollama is not a supported concept.
- UI and commands may be TypeScript, but persona runtime authority must be Rust.

## Engine Model

Rust owns:

- Turn admission and batching.
- Persona response scheduling.
- Dependency wakeups between turn artifacts and subscriber work.
- Local inference lane capacity.
- Model and provider selection.
- RAG source fan-out and shared cache keys.
- Memory consolidation admission.
- LoRA, KV, and multimodal resource paging.
- Runtime metrics and slow-command evidence.

TypeScript owns:

- Browser UI.
- Command adapters.
- Entity loading until the data module is fully Rust-backed.
- Presentation and operator tooling.

TypeScript must not own:

- Which personas run.
- In what order they run.
- How many local generations run at once.
- Which model satisfies a capability request.
- Whether background work may use the inference lane.

## CBAR Precedent: Turn Frames, Not FIFO Chat

The old CB mobile SDK solved the same class of problem under harder latency
pressure. Its C++ core owned the frame loop, cache invalidation, analyzer
cadence, and backpressure; Objective-C, Swift, Kotlin, and web wrappers were
bindings. Continuum needs the same split: Rust is the engine, TypeScript is a
thin adapter.

The direct mapping:

- `CBAR_VideoFrame` becomes a `CognitionTurnFrame`.
- Lazy image getters become lazy turn artifacts: canonical room snapshot,
  conversation history, shared RAG results, capability plan, model selection,
  prompt fragments, embedding batches, and memory deltas.
- Analyzer subscribers become persona recipes, memory jobs, RAG jobs, tool
  jobs, and airc bridge jobs.
- `QueueThread<T>` priority/cadence becomes Rust resource-class queues with
  explicit local inference, embedding, I/O, and background budgets.
- Frame-drop backpressure becomes stale-work cancellation: if a newer chat
  turn supersedes a background semantic-memory synthesis job, keep the latest
  raw memory and drop or defer the stale synthesis.

The core rule is dependency wakeup, not global FIFO. Work never waits for
unrelated work. A job declares which artifact keys it needs; when those keys
become ready, subscribers wake. If terrain changes in CBAR, semantic
segmentation, color filtering, ORB, SLAM, and surface accumulation wake
according to their declared cadence. If a chat turn arrives in Continuum, the
shared turn artifacts build once, then eligible personas, memory jobs, and
export/airc observers wake from those artifacts.

The architecture must preserve these invariants:

- The hot path never blocks on background work.
- Runtime workers should stay busy with ready work, but worker saturation must
  not become a global lock.
- The scheduler starts from maximum safe parallelism: CPUs busy, GPU admitted
  deliberately, and independent work running concurrently. It reduces cadence,
  precision, or concurrency only when measured pressure or dependency order
  requires it.
- Shared artifacts are computed once per turn and cached by stable key.
- Subscribers can run at different cadences and priorities.
- Each subscriber owns its trigger predicate: artifact changed, elapsed time,
  resource pressure changed, explicit command, or human/agent event.
- Backpressure prefers latest useful state over draining stale queues.
- Model/GPU work is admitted by Rust before it starts.
- Wrapper layers do not invent scheduling policy.

## Contract Style: Small Interfaces, Opaque Engines

CBAR kept the hard machinery behind small C++ classes. `PIMPL` hid memory
layout, cache state, thread ownership, and platform-specific buffers while the
public headers stayed small. Continuum needs the Rust equivalent:

- Public contracts are small typed structs and traits.
- Runtime state is opaque and owned by Rust.
- Boundaries pass handles, ids, and leases instead of copying memory. Large
  payloads such as media frames, embeddings, KV caches, model weights, LoRA
  pages, WebRTC buffers, and Bevy textures stay resident in their owning pool.
- Extension points are capability/recipe/model traits, not callback trees full
  of scheduling policy.
- Threading and multiprocessing are low-friction because queues, wakeups,
  pressure, and metrics are inherited from the engine.
- Adding a new persona recipe, model family, LoRA paging policy, RAG source, or
  game observer should mean implementing a narrow trait and declaring
  dependencies, not rewriting orchestration.

The repeated pattern should be:

1. Declare input artifacts and capabilities.
2. Declare resource class and budget.
3. Pass artifact handles, not copied payloads.
4. Implement the small work trait.
5. Let Rust schedule, coalesce, wake, defer, and measure it.

That is the SOLID boundary for this project. Wrappers and feature modules ask
for work; the Rust engine decides how to run it.

This also covers always-on contexts such as a game running in the background.
The game stream is just another artifact producer. New terrain, changed quest
state, visible enemies, or elapsed cadence can wake vision, code, memory, or
planning subscribers without blocking chat. If the GPU budget is tight, Rust
degrades intentionally: skip stale frames, lower cadence, summarize, or emit a
silence/deferred reason. It must not let background perception kill visible
conversation.

This is the engine-level answer to the current persona flood. The failure is
not just "too many messages"; it is missing turn-frame consolidation. Multiple
personas responding to one room event should share one room snapshot, one RAG
fan-out, one model-capability resolution, and one scheduler decision. They
should not each build a private universe and fight over the same local model.

## Existing Rust Assets

Keep and extend these instead of recreating logic in TypeScript:

- `workers/continuum-core/src/cognition/turn_batch.rs`: deterministic per-turn planning.
- `workers/continuum-core/src/persona/channel_queue.rs`: consolidated domain queues.
- `workers/continuum-core/src/persona/channel_registry.rs`: service-cycle scheduling.
- `workers/continuum-core/src/persona/response.rs`: per-persona response path.
- `workers/continuum-core/src/persona/model_selection.rs`: adapter-aware model selection.
- `workers/continuum-core/src/model_registry/*`: typed model/provider/capability registry.
- `workers/continuum-core/src/inference/backends/llamacpp_scheduler.rs`: llama.cpp scheduling.
- `workers/continuum-core/src/paging/broker.rs`: cross-pool pressure broker.
- `workers/continuum-core/src/runtime/*`: module registry, metrics, IPC, event bus, and concurrency limits.

## Adaptive Throughput Substrate

The best complete throughput design in the Cambrian codebase is CBAR:
bounded `QueueThread` workers, lazy frame artifacts, subscriber analyzers,
priority/cadence, newest-state backpressure, and thin platform wrappers.
Continuum has several strong Rust primitives, but they are not yet one unified
substrate:

- `ServiceModule` and `ModuleConfig`: one runtime extension seam for commands,
  event subscriptions, priority, concurrency, and ticks.
- `MessageBus`: typed event fan-out with coalescing and recent-event replay.
- `llamacpp_scheduler`: continuous local generation, sequence attribution, and
  future LoRA/KV routing point.
- `FootprintRegistry`: cross-resource accounting by backend, persona, and
  resource kind.
- `PagedResourcePool`: generic residency, pinning, LRU-style eviction, stats,
  and reload/spill hooks.
- `PressureBroker`: cross-pool pressure decisions.
- `ChannelQueue` / `QueueItemBehavior`: generic containers where domain items
  own priority, consolidation, and staleness.

These should converge into one reusable adaptive-throughput pattern for every
expensive process:

1. A job declares identity: `turn_key`, `artifact_key`, `persona_id`,
   `resource_class`, and optional `recipe/model/provider`.
2. A job declares dependencies by handle, not payload.
3. A scheduler admits the job when dependencies are ready and resources fit.
4. The job runs in the narrowest resource lane that can satisfy it: CPU, GPU,
   embedding, local generation, cloud provider, I/O, memory, or background.
5. The job emits typed artifacts/events and updates footprint/trace metrics.
6. Downstream subscribers wake from artifact readiness, not from global FIFO.

This becomes the repeated process model for chat, RAG, memory consolidation,
embedding, vision, live video, game observers, LoRA paging, MoE expert routing,
airc bridging, and grid-distributed work.

The substrate must be adaptive before it is clever:

- Start from maximum safe parallelism.
- Keep CPU workers busy with independent ready work.
- Admit GPU/model work deliberately from memory and lane evidence.
- Prefer latest useful state over draining stale queues.
- Coalesce repeated work by stable identity keys.
- Degrade cadence, precision, context, or subscriber count under pressure.
- Surface deferrals and silence reasons as first-class output.
- Never copy large payloads across process or language boundaries when a handle
  can identify resident data.

The failure to avoid is every module owning its own queue, throttle, retry,
cache, and memory heuristic. The extension author should implement a small
contract and inherit the hard parts: scheduling, pressure, telemetry, artifact
cache negotiation, and wakeups.

## Failure Modes To Eliminate

### Single-Responder Collapse

Symptom: only one persona replies to a broad human message.

Root causes to prevent:

- TS-side coordination window or locks silently deciding for all personas.
- Local provider queue monopolized by one persona or background work.
- RAG/source fan-out repeated per persona until the first responder consumes all budget.

Rust fix:

- `cognition/plan-turn-batch` returns one `PersonaTurnPlan` per candidate, with generation order, wave, estimated start, and estimated finish.
- The host must execute that plan or surface why it cannot.
- A later Rust `persona/run-turn` command should execute the plan directly and return posted response envelopes.
- The plan is the first `CognitionTurnFrame`: every shared artifact in it must
  be reused across persona subscribers unless explicitly declared
  persona-local.
- The plan exposes whether the turn can meet the first-response and
  all-responses alpha budgets before expensive execution starts.

### Slow Chat

Symptom: first reply arrives after 95+ seconds.

Root causes to prevent:

- Node event loop is the scheduler.
- Background tasks share local generation without admission.
- Model startup, RAG, and memory work are serialized without a visible plan.

Rust fix:

- Planner consumes local capacity from `inference/capacity`.
- Planner emits waves and expected timing.
- Runtime metrics report queue time versus execution time for every module command.

### ORM And Room Identity Drift

Symptom: stale General room tabs, wrong UUIDs, old chat rows, localStorage resurrecting ghost rooms.

Root causes to prevent:

- Multiple sources of truth for default rooms.
- URL rewrite before canonical room resolution.
- Browser-local state overriding ORM truth.

Rust fix:

- Data module becomes the canonical room/activity resolver.
- UI receives canonical handles after resolution.
- Browser caches may remember view state, not entity identity.

### IPC Drift

Symptom: TS and Rust believe different things about capacity, model capabilities, or command state.

Root causes to prevent:

- Hand-written TS types or duplicate constants.
- Commands returning success while the downstream runtime did nothing.
- Fire-and-forget process boundaries hiding failures.

Rust fix:

- ts-rs generated contracts for planner/runtime payloads.
- Command execution throws on failure at the caller boundary.
- Runtime metrics expose command queue time and error count.

## PR Sequence

### PR A: Rust Turn Schedule Contract

Purpose: make scheduling explicit and testable.

Scope:

- Extend `RecipeTurnBatchRequest` with `local_inference_capacity`.
- Extend `PersonaTurnPlan` with `generation_wave`, `estimated_start_ms`, and `estimated_finish_ms`.
- Extend `RecipeTurnBatchPlan` with first-response/all-responses budget
  evidence.
- Keep planner pure: no ORM, no inference, no filesystem.
- Add unit tests for deterministic waves and capacity.
- Document the CBAR-derived dependency-wakeup model as the alpha runtime
  direction.

Validation:

- `cargo test -p continuum-core --features metal,accelerate cognition::turn_batch --lib`

### PR B: TypeScript Adapter Obeys Rust Plan

Purpose: stop TS from inventing its own fan-out and ordering.

Scope:

- The chat path calls `cognition/plan-turn-batch` before building per-persona context.
- RAG shared sources are loaded once per turn.
- Persona execution follows `generation_wave` and local capacity.
- If execution diverges from plan, log a structured runtime error.

Validation:

- Browser chat smoke sends one marker.
- Export must show every eligible persona either responded or emitted a silence reason within 30 seconds.
- Runtime metrics must show no unplanned local inference calls.

### PR C: Rust Persona Run-Turn

Purpose: move the turn loop into Rust.

Scope:

- Add `cognition/run-turn` or `persona/run-turn`.
- Input: trigger, candidates, room snapshot, model/capability declarations.
- Output: response envelopes and silence reasons.
- Rust uses the channel registry and response path directly.
- TypeScript only posts returned envelopes through existing chat storage until the data module is Rust-backed.

Validation:

- Rust unit tests for scheduler behavior.
- Integration replay for two, three, and five local personas.
- Slow-command metrics prove queue time and inference time separately.

### PR D: Rust Model Resolver

Purpose: one typed source of truth for model capability matching.

Scope:

- Add a request shape like `ModelRequirement`.
- Fields include capabilities, architecture family, context window range, memory budget, modality, provider preference, and local/cloud policy.
- Resolver returns a concrete model id, provider id, expected memory footprint, and reason.
- No hard-coded persona model names in TS.

Validation:

- Qwen3.5 text model selected for text chat on local.
- Qwen2-VL selected for vision when vision is requested and memory allows.
- Missing model produces an actionable error, not a fallback to a random provider.

### PR E: Rust Memory/RAG Admission

Purpose: background cognition cannot starve chat.

Scope:

- Memory consolidation is a scheduled background job with a resource class.
- Semantic compression requires explicit admission from the Rust scheduler.
- RAG source cache is keyed by the turn planner and reused across personas.

Validation:

- A chat smoke with memory enabled still meets the 10s/30s gates.
- Runtime metrics show background work deferred under chat load.

### PR F: Rust Data Canonical Handles

Purpose: eliminate ghost rooms and browser state authority.

Scope:

- Canonical room resolution moves behind the Rust data/runtime boundary.
- Browser routing uses resolved handles only.
- LocalStorage cannot create or select an entity id before canonical resolution.

Validation:

- Clearing or retaining browser storage yields the same canonical General room.
- No deterministic `stringToUUID("General")` style fallback appears in the UI path.

## Test Strategy

Use VDD plus TDD:

- TDD for pure Rust units: planner, model resolver, queue consolidation, capacity waves.
- VDD for live behavior: browser chat marker, response count, latency, model used, CPU/GPU utilization.
- Replay tests for captured failures.
- Metrics tests for queue time, generation time, silence reasons, and background deferral.

Every PR must include:

- A focused Rust test when it touches runtime logic.
- A live chat smoke result when it claims chat improvement.
- A short note explaining whether Node authority increased, decreased, or stayed flat.

## Immediate Rule

Do not merge a chat-path PR to canary based only on compile success.

For chat-path work, the merge gate is:

- CI green.
- Rust focused tests green.
- Live chat smoke produces useful persona behavior, or the PR is explicitly labeled as instrumentation/guardrail and not claimed as a chat fix.
