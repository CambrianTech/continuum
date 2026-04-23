# Persona-as-Rust-Library — Architectural Plan

> Every TS layer deleted = a Node round-trip eliminated, a copy eliminated, an async overhead removed. Every byte tracked Rust-side avoids a Node↔Rust marshaling round-trip. **Deeper = lighter = more concurrent.** The architecture leans into this everywhere.

**Parent:** [Architecture](README.md)
**Related:** [RECIPE-EXECUTION-RUNTIME.md](RECIPE-EXECUTION-RUNTIME.md), [PERSONA-COGNITION-RUST-MIGRATION.md](PERSONA-COGNITION-RUST-MIGRATION.md), [PERSONA-CONTEXT-PAGING.md](PERSONA-CONTEXT-PAGING.md), [LIVE-VIDEO-CHAT-ARCHITECTURE.md](LIVE-VIDEO-CHAT-ARCHITECTURE.md), [LORA-GENOME-PAGING.md](../personas/LORA-GENOME-PAGING.md)

## Status overview (2026-04-23)

- **Phase A (cognition substrate):** A1–A5 ✅ landed
- **Phase B (recipes):** Rust Recipe-trait approach RIPPED (was wrong shape — recipes are DATA). Replaced with: JSON recipe entities + Rust-native pipeline executor (per `RECIPE-EXECUTION-RUNTIME.md`). Executor not yet built. Old hardcoded Recipe trait + ChatRecipe deleted in commit `983d30102`.
- **Phase C (paging):** All steps unstarted. Today proved C5 (MtmdContext pool) is the latency killer — see findings below.
- **Phase D (FFI / embeddable):** All steps unstarted.
- **Phase E (trace + replay):** Replay test infrastructure repaired in commit `66c4d3799`. Trace emission still pending.
- **Phase F (output quality):** NEW phase added 2026-04-23 — model output bugs surfaced during testing (echo loops, "SpeakerName: X" garbage, tool_use markup leak). Widget chip rendering shipped in commit `980bcbce6`. Prompt assembly bugs remain.

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
