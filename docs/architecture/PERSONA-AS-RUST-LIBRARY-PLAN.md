# Persona-as-Rust-Library — Architectural Plan

> Every TS layer deleted = a Node round-trip eliminated, a copy eliminated, an async overhead removed. Every byte tracked Rust-side avoids a Node↔Rust marshaling round-trip. **Deeper = lighter = more concurrent.** The architecture leans into this everywhere.

**Parent:** [Architecture](README.md)
**Related:** [RECIPE-EXECUTION-RUNTIME.md](RECIPE-EXECUTION-RUNTIME.md), [PERSONA-COGNITION-RUST-MIGRATION.md](PERSONA-COGNITION-RUST-MIGRATION.md), [PERSONA-CONTEXT-PAGING.md](PERSONA-CONTEXT-PAGING.md), [LIVE-VIDEO-CHAT-ARCHITECTURE.md](LIVE-VIDEO-CHAT-ARCHITECTURE.md), [LORA-GENOME-PAGING.md](../personas/LORA-GENOME-PAGING.md)

## Pragmatic delivery — what we are reducing and what every change must satisfy

The work below is in service of three measurable outcomes, in order of weight:

1. **Reduce latency.** Felt latency is FPS for personas. Every IPC round-trip eliminated, every Metal allocation pooled, every encode amortized counts. The 17-min/image encode time observed 2026-04-23 is the canonical example of what "reduce latency" means concretely — until that's down two orders of magnitude, video chat is impossible regardless of feature count.
2. **Reduce brittleness.** A change that breaks vision should fail loudly in a Rust test BEFORE it reaches a deploy. A test that reports PASS while testing zero things is brittleness, not safety. Today's silent-pass on the slow-replay (extractors reading the wrong shape) is the canonical example of what "reduce brittleness" means concretely.
3. **Reduce iteration cost via record/playback at every level.** Every persona turn (chat, vision, audio, tool, recipe step, cognition seam) gets captured to a fixture and is replayable in a Rust test against real models. **No "deploy and pray."** The test loop is: change Rust → `cargo test` against captured fixtures → fix concrete failure → repeat. Live deploy is the *last* gate, not the *only* gate.

Every step in the phases below earns inclusion by serving one of those three. Steps that don't measurably reduce latency, reduce brittleness, or improve the record/playback loop are deprioritized regardless of how interesting they are architecturally.

**The capture-and-replay infrastructure is treated as foundational, not ancillary.** It is the only way out of the deploy-and-pray cycle. Specifically:

- Every `cognition/respond` call captures a fixture today (PRG.ts records `{ rust_request, rust_response, ipc_error, ipc_duration_ms }`). Repaired extractor (commit `66c4d3799`) lets the Rust slow-replay consume them.
- Future capture surfaces to add: per-recipe-step capture inside the executor (Phase B2), per-seam trace events inside `respond()` (Phase E1), per-frame capture for live video (Phase B8 with C5 in place).
- Replay surfaces to add: `cargo test --test recipe_executor_replay`, `cargo test --test live_video_replay`, eventually `cargo test --package continuum-persona` running embedded-host scenarios with no orchestrator.

When a user reports a bug, the workflow becomes: capture the broken fixture → write a `#[test]` that loads it → reproduce the failure in a Rust test → fix → green. No live deploy needed for the inner loop.

## 2026-05-11 Architecture Posture

The library plan is no longer a future refactor. It is the management plan for getting Continuum to alpha.

The target is a Rust persona runtime with browser/TS as an adapter, not a TypeScript persona runtime with Rust helpers. That distinction is load-bearing:

- **PersonaRuntime is the product core.** It owns turn batching, inbox consolidation, RAG/context assembly, model selection, inference, post-processing, memory events, tool execution, and resource accounting.
- **Sensory I/O is core persona behavior.** A standard persona is expected to perceive text, image/video, and audio; speak or produce audio; drive avatar/control output; and appear in WebRTC rooms. Text-only is a compatibility/degraded path, not the product definition.
- **TS is a host adapter.** It renders UI, receives browser/user events, invokes typed Rust commands, and posts results. It must not decide how a persona thinks.
- **Every step must delete the old owner.** A Rust duplicate beside an active TS implementation is not migration; it is two sources of truth. #1068 and #1069 are the pattern: move the behavior to Rust, add Rust tests, remove the TS duplicate.
- **Major rework is allowed when the boundary is wrong.** Do not preserve an API because downstream code is messy. Preserve user-visible behavior, not internal accidental architecture.
- **Concurrency and pressure are first-class design inputs.** Persona code should be designed like a realtime engine: evented, bounded, backpressured, resource-aware, and measured.

### Qwen-First Sensory Runtime Target

The base local persona target is Qwen multimodal: Qwen 3.5 now, Qwen 3.6 as soon as it is viable. The runtime should ask for capabilities and budgets, not names: "needs vision + audio + tool/control output + context >= X + GPU residency within Y" is the contract. The model registry then resolves the best available Qwen-family or forged derivative on the current machine.

This is why the model/provider registry belongs in Rust. It must reason about:

- multimodal capability flags: text, vision, audio input, audio output, tool/control, embedding, LoRA, MoE;
- hardware support: Metal, CUDA, Vulkan, DMR, unified memory, VRAM, context/KV footprint;
- residency and paging: base model, mmproj, audio layers, LoRA adapters, KV cache, embeddings, and avatar/render resources;
- degradation: explicit `Unavailable`, `MissingCapability`, `CpuFallbackRequired`, `InsufficientMemory`, or `KernelGap` states surfaced to UI/tests;
- upstream work: llama.cpp, Candle training path, GGUF tooling, projector support, and kernels are modifiable dependencies. Fork/vendor/upstream when Qwen needs a layer or optimization.

STT/TTS remain useful adapters for compatibility models, but they are not the happy-path architecture for standard personas. The happy path is sensory-native personas running on the user's GPU budget.

The next major architectural milestone is a Rust-owned persona turn pipeline:

```text
Signal/RoomEvent
  -> Rust inbox consolidation / admission control
  -> Rust RAG/context builder
  -> Rust recipe or cognition executor
  -> Rust inference/model resolver
  -> Rust post-processing + trace/fixture capture
  -> thin host post/broadcast adapter
```

The system is not considered healthy while this path depends on Node for batching, cognition decisions, prompt/RAG construction, or model/tool behavior.

### Uniform Rust OOP Pattern

Rust does not use Java/C++ base classes directly, but Continuum should preserve the same design discipline: common complexity belongs in shared base traits, default implementations, and reusable engines. Leaf modules should declare what they are, not reimplement how the runtime works.

The model is CBAR-style: `QueueThread<T>` owned the queue, wake cadence, priority behavior, abort/flush semantics, and backpressure; subclasses only implemented `handleItem`. `CBAR_VideoFrame` owned lazy cached derived data; analyzers consumed it without recomputing or copying. Continuum needs the same shape for AI runtime work.

In Continuum terms, a persona component, model backend, recipe step, memory source, transport, or tool should get logs, trace, fixture capture, metrics, comms, concurrency, cancellation, queueing, backpressure, and resource accounting for free by implementing the base contract. If each subclass/implementor has to wire those itself, the abstraction is wrong.

Required pattern:

| Layer | Rust shape | Owns |
|---|---|---|
| Runtime base | `PersonaRuntime`, `RuntimeEngine`, `RuntimeContext` | lifecycle, event loop, cancellation, deadlines, trace, fixture capture |
| Capability contracts | traits such as `InferenceBackend`, `PageableBackend`, `MemoryStore`, `ToolExecutor`, `RecipeExecutor` | uniform behavior contracts and typed errors |
| Policy engines | `PressureBroker`, `PagingPolicy`, `AdmissionController`, `TurnBatcher` | scheduling, backpressure, residency, fairness, resource budgets |
| Data contracts | `Signal`, `PersonaContext`, `RespondInput`, `RecipeStep`, `ModelRequirement` | ts-rs exported wire types and replay fixtures |
| Adapters | `LlamaCppAdapter`, future cloud/local/grid adapters, TS host adapter | eccentric platform/provider details only |
| Leaf behavior | small structs implementing traits | domain-specific logic with no duplicated lifecycle/scheduling/error handling |

Rules:

- **Complexity lives at the base.** Backpressure, cancellation, queue draining, retry, replay capture, tracing, metrics, and typed error propagation are implemented once in the substrate.
- **Leaf modules are boring.** If adding a backend, recipe step, tool, or memory source requires custom lifecycle code, the base trait is missing an abstraction.
- **Uniform command semantics.** Command execution returns typed success/error. Callers own catch/retry/report behavior. Inner command implementations should not swallow errors into fake success.
- **IDs over copies.** Runtime boundaries pass handles, IDs, offsets, buffer references, or artifact keys whenever possible; large media, KV, tensors, embeddings, and frames are not copied through Node.
- **Speed is inherited.** New modules get concurrency, batching, backpressure, and replay automatically by implementing the base contract. Performance is not a per-feature afterthought.
- **Pipelines are inherited.** A new subclass/implementor plugs into the runtime pipeline; it does not invent its own logging, scheduling, IPC, or test harness.
- **Comms are inherited.** A component emits and consumes typed events through the runtime bus. AIRC/grid/host adapters bridge those events; leaf components do not know transport details.

## Status overview (2026-05-11)

- **Phase A (cognition substrate):** A1–A5 ✅ landed
- **Phase A.4/A.5 follow-through:** #1068 moved turn recording fully Rust-side; #1069 moved response cleanup Rust-side and removed the TS duplicate.
- **Phase B (recipes):** Rust Recipe-trait approach RIPPED (was wrong shape — recipes are DATA). Replaced with: JSON recipe entities + Rust-native pipeline executor (per `RECIPE-EXECUTION-RUNTIME.md`). Executor not yet built. Old hardcoded Recipe trait + ChatRecipe deleted in commit `983d30102`.
- **Phase C (paging):** Substrate pieces exist, but the actual resource manager is incomplete. MtmdContext pooling, KV policy, LoRA/model residency, and pressure gates are alpha-critical.
- **Phase D (FFI / embeddable):** All steps unstarted.
- **Phase E (trace + replay):** Recorder exists and is now Rust-owned. Per-seam trace emission and replay tooling still need to become mandatory gates.
- **Phase F (output quality):** Tool/thinking markup cleanup is Rust-owned as of #1069. Echo loops, generic greetings, and prompt/RAG quality remain active blockers.

## What today taught us (load-bearing findings 2026-04-23)

These adjust the original plan's priorities. Capture them here so the next session doesn't re-derive:

1. **Image encoder takes ~17 minutes per image on this hardware (M-series Mac).** Replay test logged: `image slice encoded in 499391 ms; image decoded (batch 1/2) in 384796 ms; image decoded (batch 2/2) in 151229 ms`. **This is the latency catastrophe.** It's the actual reason 4 concurrent personas hit the 300s timeout, not multi-mtmd brick race. C5 (MtmdContext pooling) and an investigation into WHY encode is so slow are now the most urgent items in the whole plan.
2. **Image bytes DO arrive at the encoder through the new IPC path.** Confirmed by replay: `signal.media[].base64` flows through `cognition_io::build_respond_input` → `RespondInput.message_media` → `MtmdContext::generate_with_image` correctly. The IPC reshape did NOT break byte plumbing.
3. **Model output is broken even when bytes arrive correctly.** qwen2-vl returned "SpeakerName: Vision AI" (22 chars, no description) for an image the encoder successfully processed. This is **prompt assembly / system prompt** broken, not vision broken. Echo loops in chat ("Claude Code: <verbatim user message>") are the same family. Drives the new Phase F.
4. **Test infrastructure was silently passing on zero work.** The slow replay (`vision_fixture_describes_image_via_real_model --ignored`) early-exited when its extractors couldn't find media in post-rip fixtures (extractors were reading the OLD flat shape, IPC reshape moved them under `signal`/`personaContext`). Reported PASS while testing nothing. Repaired in `66c4d3799`. **Lesson: a test that early-exits on empty filter looks identical to a test that ran and passed. "0 fixtures matched" = failed gate, not passed gate.**
5. **The rip is right; the executor is what's missing.** Recipes-are-data is correct (Rust trait was wrong shape). But the *executor* that walks recipe JSON belongs in Rust per the same "deeper = lighter" principle. The TS chat path currently bypasses recipes entirely — works because the chat persona's flow is hardcoded into PRG.ts → cognition/respond. To get recipe-driven cognition (and embeddable hosts), the Rust executor in `RECIPE-EXECUTION-RUNTIME.md` becomes Phase B's main deliverable.
6. **The recipe direction adjusted (Joel, 2026-04-23):** "yes everything including recipes should probably make it to rust." Recipe entities stay as JSON data. Recipe loader, executor, dispatcher all become Rust. TS holds only schema (ts-rs generated) + thin IPC binding for the chat surface to feed Signal/PersonaContext.

## Phase A — Cognition substrate ✅

| Step | What | Status |
|------|------|--------|
| A1 | Caller-declared capabilities (no global lookup) | ✅ |
| A2 | `MediaPolicy::AtMostOneLatest` | ✅ |
| A3 | Fixture replay (shape + behavior) | ✅ shape; ✅ behavior gate repaired 2026-04-23 |
| A4 | Recorder Rust-side (`persona::recorder` writes per-turn capture from inside `respond()`) | ✅ |
| A5 | `CognitionTrace` value object accumulating per-seam | ✅ value object exists |

## Phase B — Recipes (REVISED — recipes are data, executor is Rust)

The original Phase B was a Rust `Recipe` trait with per-domain impls (ChatRecipe, VisionRecipe, …). That was wrong shape and got ripped (`983d30102`). The new shape per Joel's direction + `RECIPE-EXECUTION-RUNTIME.md`:

- **Recipe definition** = JSON entity (lives in `RecipeEntity`, authored by humans/AIs, shareable on grid)
- **Recipe walker / executor** = Rust-native (`continuum-core/src/recipe_executor/`)
- **Per-domain "behavior"** = the recipe's `pipeline[]` of kernel commands + per-step config
- **TS surface** = thin schema (ts-rs generated `Recipe`, `RecipeStep`, etc.) + dispatcher that hands the chat-time signal to Rust

| Step | What | Dependency | Status |
|------|------|------------|--------|
| B0 | Rip the wrong-shape Rust Recipe trait + ChatRecipe + RecipeRegistry | A4 | ✅ commit 983d30102 |
| B1 | Reshape `cognition/respond` IPC to `{signal, personaContext}` | B0 | ✅ commit 983d30102 |
| B2 | Rust-native pipeline executor: `RecipeExecutor::run(recipe, signal, ctx) → Output` — walks `pipeline[]`, dispatches kernel commands, threads state via interpolation, captures training data per step | B1 | not started |
| B3 | Rust-native command dispatcher (calls Rust commands directly; calls TS commands via existing IPC for now) | B2 | not started |
| B4 | Recipe loader (Rust) — read JSON RecipeEntity, validate against schema, register | B2 | not started |
| B5 | Wire chat path through executor: PRG.ts becomes ~50-line shim that dispatches to `recipe/run` (executor in Rust) instead of `cognition/respond` directly | B2, B3, B4 | not started |
| B6 | Vision pipeline (image media → vision-capable persona) — JSON recipe step + per-step config | B5 + C5 (MtmdContext pool — encoder must be fast enough not to wedge concurrency) | not started |
| B7 | Audio pipeline (audio in/out) — JSON recipe step + Rust audio dispatch | C1, C2 (paging substrate must land first or it bricks) | not started |
| B8 | Live-video recipe (per-frame cadence, change-gate per `LIVE-VIDEO-CHAT-ARCHITECTURE.md`) | C2, C5 | not started |
| B9 | Code recipe (file/diff context, no chat history) — pure JSON, executor walks it | B5 | not started |
| B10 | Game recipe (scene-graph blob → action choice) — pure JSON | B5 | not started |

**Recipes are pluggable.** Adding one = JSON authoring + maybe one new kernel command. No core changes.

## Phase C — Paging substrate (THE latency + brick prevention work)

This is what the branch was named for and what today's findings say is the **most urgent**. Concrete pieces:

| Step | What | Why critical |
|------|------|--------------|
| C1 | `mmproj` init mutex — one mtmd-capable backend may be inside Metal pipeline-compile at a time | Restores qwen2-audio safely; unblocks AudioRecipe |
| C2 | Backend recovery on Metal OOM — catch `kIOGPUCommandBufferCallbackErrorOutOfMemory`, drop+recreate the backend instead of leaving it permanently dead | Today: one OOM = chat dead until reboot |
| C3 | `PressureBroker` as gate (not measure-only) — refuse second mtmd backend creation while another is mid-init or while Metal residency > threshold | Substrate-level guard, not a config-file workaround |
| C4 | `PagedResourcePool` Phase 2 — eviction under pressure. `FootprintRegistry` already tracks; this acts on the data | Phase 1 done, Phase 2 pending |
| **C5** | **MtmdContext pooling** — currently each `generate_with_image` allocates a fresh ~2GB Metal context. Pool + reuse + evict under pressure | **PROMOTED TO TOP PRIORITY 2026-04-23.** Today's replay logged 17-min encode time per image. With per-image fresh allocation, live video at 5+ Hz = ~10GB/s of Metal churn = unsustainable. Even single-image chat is bottlenecked. This is the latency killer. |
| C6 | KV cache eviction policy — currently no policy. Under pressure, evict by `FootprintRegistry`'s per-persona attribution | Many-personas-on-M2-Air goal from `PERSONA-CONTEXT-PAGING.md` |
| C7 | LoRA genome paging primitives — page adapter weights in/out of GPU per active task, LRU eviction | Design exists in `LORA-GENOME-PAGING.md`, runtime not built yet |
| **C8** | **Investigate WHY encode is 17min/image** (NEW 2026-04-23) — pool helps but if a single encode legitimately takes 17 min, video chat is impossible regardless of pooling. Suspects: KV cache size, batch size, Metal kernel coverage gap for qwen2-vl, model loaded with wrong context window | **Blocks anything video-chat-shaped** |

## Phase D — Embedding surface (the "no Node" deliverable)

| Step | What | Why |
|------|------|-----|
| D1 | Split `continuum-core` → `continuum-persona` (the embeddable atom) + the rest (server orchestration) | Smaller link surface for embedded hosts; explicit boundary |
| D2 | `PersonaRuntime` Rust API: `new(config) → tick() → feed(signal) → poll_response()` | Synchronous-feeling, async-implemented; suits game-loop hosts |
| D3 | `continuum-persona-ffi` C-ABI wrapper | Unreal C++ links it; iOS/Vision Pro Swift consumes it |
| D4 | Unreal plugin POC: persona inside an actor, NPC-style | Validates D3 |
| D5 | Swift package POC: persona inside a Vision Pro reality view | Validates D3 |

**Test consequence:** `cargo test --package continuum-persona` exercises the full persona without spinning up the orchestrator, without TS, without the chat surface. Unreal/Swift integration is a thin wrapper around an already-tested library.

## Phase E — Trace / observability ("oscilloscope on every persona")

| Step | What | Status |
|------|------|--------|
| E1 | Each seam in `respond()` emits a `TraceEvent` to the per-turn `CognitionTrace` (Rust-native) | partial — value object exists, per-seam emission incomplete |
| E2 | Trace serializes to fixture (Phase A artifact) AND to a live event bus | not started |
| E3 | Differential replay tool: `cargo run --bin trace-diff -- fixture.json --vs HEAD --vs origin/main` | not started |
| E4 | Live observability consumer (TS or any) subscribes to the event bus — gauges per persona (queue depth, KV bytes, decode tok/s, mood/energy from `PersonaState`, last seam latency) | not started |
| E5 | Differential replay = chaos-engineering hook: substitute "model returned garbage" at the inference seam, assert post-processing handles it | not started |
| E6 | Training corpus: replay each captured turn with a different model / LoRA, measure response quality, build a labeled dataset for fine-tuning | not started |
| **E7** | **Fixture replay extractors track wire shape** (NEW 2026-04-23) — when IPC shape changes, the test gate must update in the same commit. Today's failure: extractors silently early-exited on shape mismatch and reported PASS. Repaired in `66c4d3799` but the principle generalizes. | ✅ in this case; rule is durable |

## Phase F — Output quality (NEW 2026-04-23)

The model returns broken output in patterns that aren't bugs in the IPC or the inference path — they're prompt assembly / system prompt / RAG composition issues. Surfaced in testing today.

| Step | What | Why |
|------|------|-----|
| F1 | ✅ Tool-use markup rendered as collapsible chip in chat widget (commit `980bcbce6`) | Even if the model emits `<tool_use>` markup, it doesn't appear as raw text in chat |
| F2 | ✅ Communication group example targets a different room (commit `980bcbce6`) | Discourages chat/send for current-room replies via the example, not just the instruction |
| F3 | Investigate "SpeakerName: Vision AI" output bug — model returns 22 chars of self-identification with no description even when image bytes processed correctly. Likely prompt-template or system-prompt mismatch | Reproducible in single-fixture replay (no live system needed). Clear test gate. |
| F4 | Echo loop fix — personas regurgitate user/peer messages verbatim. Likely `recent_history` RAG composition feeding own/peer outputs back in | Required for any usable conversation; widely visible in testing |
| F5 | Sentinel marker leak (`Sentinel: dev/build-feature` appearing as text) — model hallucinating from RAG context | Pre-existing issue surfaced more visibly via deliberate testing |
| F6 | Prompt-assembly observability via Phase E (fixture trace) — see exact prompt sent to model for each turn so prompt bugs are diagnosable from a fixture, not from "I think the model is confused" | Multiplies leverage on F3-F5 |

## Dependency ordering (what blocks what)

```
A4 (recorder Rust-side) ─┬→ A5 (CognitionTrace)
                         └→ B2 (Rust pipeline executor)
                                ├→ B3 (command dispatcher)
                                ├→ B4 (recipe loader)
                                └→ B5 (chat path through executor) → B6/B9/B10
                                                                     │
                                                                     └→ B7/B8 BLOCKED on C1+C2+C5

C1 (mmproj mutex) ─┬→ C2 (backend recovery)
                   └→ C3 (PressureBroker gate) → C4 (eviction) → C5 (mtmd pool)
                                                                          │
                                                                          └→ B7 (Audio), B8 (Live video)

C8 (encoder slowness investigation) ─→ unlocks ANY video-chat-shaped use case

D1 (crate split) → D2 (PersonaRuntime) → D3 (FFI) → D4/D5 (Unreal/Swift POCs)

E1-E2 (trace emission) parallel to A5 / Phase B
E3-E5 (replay tooling) after A5 + B2

F1-F2 ✅ shipped
F3-F5 attack with replay (fast loop, no live needed) once Phase E trace emission gives visibility into the assembled prompt
```

## Branch ordering

### `feature/persona-recipes` (this branch — currently open)
- ✅ B0, B1 (rip + IPC reshape — commit `983d30102`)
- ✅ F1, F2 (tool-use chip + example fix — commit `980bcbce6`)
- ✅ E7 (replay extractor repair — commit `66c4d3799`)
- Pending decision: do we ship this branch as-is and open the next, or include more here?

### Next branch — `feature/persona-paging-substrate` (the urgent one given today's findings)
- C1, C2, C3 (mmproj mutex + backend recovery + PressureBroker gate)
- C5 + C8 (MtmdContext pool + encoder slowness investigation) — together fix the 17-min/image latency
- C4, C6 (eviction + KV cache policy)

### Next branch — `feature/persona-recipes-executor`
- B2, B3, B4, B5 (Rust pipeline executor + dispatcher + loader + chat-path wiring)
- B6 (vision pipeline through executor — depends on C5 from paging branch landing first)
- B9, B10 (code, game recipes — pure JSON, fast)

### Next branch — `feature/persona-output-quality`
- F3, F4, F5 (prompt assembly + echo loop + sentinel marker fixes)
- Each one attacked via replay test (Phase E gives the prompt visibility)

### Parallel branch — `feature/persona-trace`
- E1, E2 (per-seam trace emission + serialization to fixture + event bus)
- E3, E4, E5, E6 (replay tooling + live observability + chaos hook + training corpus)

### Future branch — `feature/persona-ffi`
- D1, D2, D3 (crate split + PersonaRuntime + C-ABI)
- D4, D5 (Unreal + Swift POCs)

## Discipline anchors (from 2026-04-22/23 hard lessons)

These are the rules I have to keep enforcing on myself. Cross-referenced from auto-memory feedback files:

- **Rust = LOGIC, TS = schema + thin IPC binding only** ([feedback_rust_first_sharpened.md](../../.claude/projects/-Users-joelteply-Development-cambrian-continuum/memory/feedback_rust_first_sharpened.md)). Pre-commit self-check: *"Would Joel write this in Objective-C inside the SDK he licensed to Home Depot?"* If no, doesn't belong in TS either.
- **Forensic, not destructive** ([feedback_forensic_not_destructive.md](../../.claude/projects/-Users-joelteply-Development-cambrian-continuum/memory/feedback_forensic_not_destructive.md)). Capture state BEFORE killing. Investigate BEFORE fixing. Bisect BEFORE guessing.
- **Test before deploy/commit, especially the SLOW replay** ([feedback_test_safer_use_replay.md](../../.claude/projects/-Users-joelteply-Development-cambrian-continuum/memory/feedback_test_safer_use_replay.md)). End-to-end against real models is the gate. "0 fixtures matched" = failed gate.
- **Joel's musings are NOT directives** ([feedback_musings_are_not_directives.md](../../.claude/projects/-Users-joelteply-Development-cambrian-continuum/memory/feedback_musings_are_not_directives.md)). When Joel asks "should we maybe Y" → engage as discussion, never demolish work mid-execution.
- **Don't pile changes on a degrading system.** Memory leaks accumulating, hung process, slow responses → STOP and diagnose, don't ship more.
- **Silent success is a failure signal.** If the visible product surface (chat reply, screenshot) doesn't show success, the change FAILED — even if every internal log says success.
