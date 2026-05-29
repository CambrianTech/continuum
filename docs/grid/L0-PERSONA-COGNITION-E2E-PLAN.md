# L0 Plan — E2E Persona Cognition in Rust Alone

**Status:** plan, refines [GRID-MIGRATION-ROADMAP](GRID-MIGRATION-ROADMAP.md) L0 layer.
**Predecessor:** [L0-2-DISPATCH-SLICING.md](L0-2-DISPATCH-SLICING.md) — proposed L0-2 as 3 sub-slices a/b/c.
**Priority:** Joel 2026-05-29: *"would take careful planning to migrate. I would get e2e persona cognition first, within RUST alone."*

## What "E2E persona cognition in Rust alone" means concretely

A persona receives a message → evaluates → optionally responds. Every step happens **inside the Rust runtime** with **no TS in the cognition path**.

The boundaries that may legitimately stay TS (because they're form-specific):

- Message INGRESS — the source that delivers a chat message to the persona. Today: TS receives airc events; eventually: airc embed in Rust directly. **Transitional acceptable**: TS receives → puts message into Rust channel.
- Message EGRESS — the path that publishes a generated response. Today: TS `chat/send` command publishes to airc. **Transitional acceptable**: Rust dispatches the `chat/send` command via the universal `CommandExecutor` (which routes through the TS bridge socket until airc embed lands).

What is **not** acceptable as TS:

- Decision logic (should-respond, priority, evaluation gates)
- Cognition state (PersonaCognition, sleep state, rate limiter, message cache)
- Response generation orchestration (prompt assembly, model selection, inference dispatch)
- Loop / tick cadence (the autonomous service loop)
- Genome paging / LoRA activation logic
- Inbox routing
- Admission gate / dedup / engram creation

## Today's state (audit, 2026-05-29)

### Rust side (already exists in continuum-core/src/persona/)

- `PersonaCognition` (unified.rs) — container for all per-persona cognitive state. Has `new(persona_id, persona_name, rag_engine)` constructor + `with_budget` variant.
- `PersonaCognitionEngine` — `fast_path_decision`, `enqueue_message`, `state`, `update_state`, `mark_message_evaluated`.
- `full_evaluate` (evaluator/mod.rs:195) — unified pre-response gate (response_cap → mention → rate_limit → sleep_mode → directed_mention → fast_path).
- `respond` (response.rs:197) — async response generation. Takes `RespondInput`, returns `Result<PersonaResponse, String>`.
- `channel_registry::service_cycle()` — pops next item from the per-persona channel queue, respects priority + state gating.
- `PersonaServiceModule` (L0-1, merged in #1457) — singleton ServiceModule, `persona/status` works, `persona/enroll` returns the L0-2-not-wired error, tick is no-op.
- `airc_admission.rs` — converts a signed airc envelope into an `AdmissionCandidate` for persona memory.

### TS side (still drives the loop today)

- `PersonaAutonomousLoop.ts` (~349 LOC after #1459 doctrine cleanup) — `runServiceLoop`, `serviceInbox`, `handleItem`. Drives every persona's tick. Calls into Rust `serviceCycleFull` to get items, dispatches via `evaluateAndPossiblyRespondWithCognition`.
- `PersonaMessageEvaluator.ts` (~974 LOC) — `evaluateAndPossiblyRespondWithCognition`. Calls `rustCognition.fullEvaluate()` then coordinates with the chat coordinator, builds RAG, calls `respondToMessage`.
- `PersonaResponseGenerator.ts` (~904 LOC after #1459 cleanup) — orchestrates the response pipeline: prompt assembly, model selection, inference, tool execution, response posting.
- `PersonaUser.ts` (~2160 LOC after #1459 cleanup) — receives airc events, routes to the inbox, kicks off autonomous loop, hosts the cognition bridge.
- The cognition path from "received chat" → "posted response" crosses TS↔Rust boundary at least 4–6 times.

## Sequencing

Five sub-slices, each shippable with no silent-drop window, each leaves the tree green.

### L0-2-prep — PersonaSlot extension, enroll opens (no dispatch yet)

**Adds Rust:**
- `PersonaSlot { persona_id, display_name, cognition: PersonaCognition, circuit_open_until_ms, consecutive_failures }` in `service_module.rs`
- `PersonaServiceModule.personas: Mutex<HashMap<Uuid, PersonaSlot>>`
- `enroll(persona_id, display_name, rag_engine)` constructs the slot
- `persona/enroll` command opens (no longer returns L0-2-not-wired error)
- `persona/status` reports enrolled list with persona_id + display_name
- tick remains no-op (no dispatch yet — *but enrollment is now real*, so when L0-2-dispatch lands the slot exists)

**Tests Rust:** 6 — enroll constructs, enroll idempotency, status reflects enrolled list, two distinct personas, unknown command, tick still no-op.

**TS:** none touched.

**Why this is safe to ship alone:** enrolling a persona changes no behavior — TS PersonaAutonomousLoop is still driving everything. The Rust enrollment is *latent* until L0-2-dispatch wires it.

**Net:** ~150 LOC Rust added, 0 TS deleted. Foundation for the next slice.

### L0-2-dispatch — `service_once_for` wired, exercised in tests only

**Adds Rust:**
- `service_once_for(slot)` — pops via `channel_registry::service_cycle` from the slot's cognition channels; dispatches through `full_evaluate`; if `should_respond`, calls `respond()`; emits a structured `persona/responded` event with the generated text + correlation id.
- `tick` iterates enrolled slots, calls `service_once_for`, manages per-slot circuit breaker (5 consecutive failures → 30s cooldown), respects max-drain-per-tick (20 items).
- Bookmark advance via Drop guard on the dispatch handle so it ALWAYS advances (success path AND error path) — matches the existing TS structural-progress invariant.

**Tests Rust:** 10 — empty inbox no-op, single message dispatch, full_evaluate-says-no path, full_evaluate-says-yes path, respond-error path, circuit breaker trips on N consecutive errors, cooldown timer, drain bound respected, two enrolled personas dispatch independently, bookmark advances on error.

**TS:** STILL untouched. The TS PersonaAutonomousLoop is still the production driver. The Rust dispatch is exercised in unit tests but no production callsite invokes `PersonaServiceModule.tick` yet.

**Why this is safe:** the Rust dispatch is fully self-contained; no production path calls it. TS continues unchanged.

**Net:** ~300 LOC Rust + 250 LOC tests. 0 TS deleted.

### L0-2-cutover — atomic switch + TS PersonaAutonomousLoop deletion

**This slice is the cliff.** All TS-side dispatch dies; Rust takes over.

**Adds Rust:**
- `PersonaServiceModule.tick` becomes the production loop. Registered via the runtime's normal module-tick scheduler at module init.
- Response posting: `service_once_for` dispatches `Commands.execute("chat/send", {...})` via the universal CommandExecutor. The TS side handles publish until airc embed lands; the Rust side is the orchestrator.

**Removes TS:**
- `PersonaAutonomousLoop.ts` — entire file, 349 LOC.
- `PersonaUser.startAutonomousServicing()` — replaced with a call to register the persona with the Rust ServiceModule via `persona/enroll`.
- `PersonaUser.stopAutonomousServicing()` — replaced with `persona/unenroll` (new mirror command).
- Callsites in `autonomous-learning-e2e.test.ts` — update or delete tests for the TS loop.

**Verification (gate):**
- 15-persona scenario in general room: every persona receives messages, evaluates, responds (or stays silent based on cognition's decision).
- No ghost retries (bookmark advances correctly).
- No duplicate dispatch (TS loop is gone; only Rust dispatches).
- Circuit breaker observably trips if a persona's cognition keeps erroring.

**Net:** ~50 LOC Rust + ~400 LOC TS deleted. Net -350 LOC, but the value is the architectural cutover.

### L0-3 — Genome / LoRA paging moves to Rust (PersonaGenomeManager.ts deletion)

Out-of-scope details for now; sketched in [LORA-GENOME-PAGING.md](../personas/LORA-GENOME-PAGING.md). After L0-2-cutover, the TS PersonaGenomeManager has no Rust caller; deletion is mechanical.

### L0-4 — Inbox routing moves to Rust (PersonaInbox.ts deletion)

The Rust `channel_registry` already exists. After L0-2-cutover the TS `PersonaInbox` is the only remaining TS-side queue; its routing logic moves to Rust subscribers on airc room events.

### L0-5 — Final `PersonaUser.ts` cull

After L0-2 + L0-3 + L0-4 land, the remaining methods on PersonaUser.ts are mostly form-glue: receive airc events, route to Rust, expose RAG bridges for the response generator. Most of the 2160 LOC is then dead. Final cull.

## Dependencies + blockers

- **Not blocked by airc#1075.** L0-2-prep through L0-2-cutover use the universal CommandExecutor's existing TS-route branch for response posting. No airc embed needed yet.
- **Not blocked by e51ab14e.** That blocks the chat-flow migration (PR #1462 scope). E2E persona cognition in Rust does not require machine-singular daemon — the existing TS bridge for airc-event-ingress + chat-send-egress works.
- **Blocked by knowing the rag_engine source.** L0-2-prep needs a way to obtain `Arc<RagEngine>` at enroll time. Open question: does the runtime's `ModuleContext` already plumb a shared RagEngine, or does PersonaServiceModule construct one? Need to investigate before writing L0-2-prep.

## Pre-implementation investigation

Before writing L0-2-prep code:

1. Confirm how `Arc<RagEngine>` is shared today. Is there a runtime-managed singleton? Per-persona? Constructed lazily?
2. Confirm how `channel_registry` items get populated today. Who writes to it, and does that path need to change for the Rust loop to drain it?
3. Confirm `Commands.execute` is reachable from inside a Rust ServiceModule. The `command_executor.rs` exists; ServiceModule needs to dispatch through it.
4. Identify the existing test fixtures for `PersonaCognition`. If there's a mock RagEngine or test harness, L0-2-prep tests can reuse it.

I'll do those four checks before opening the L0-2-prep implementation PR.

## What this plan is NOT

- Not a contract negotiation — sub-slice boundaries may shift as the implementation reveals the shape.
- Not a substitute for actually shipping. The plan exists so the slices are reviewable and the cutover gate (L0-2-cutover) doesn't surprise anyone.
- Not a deletion of [L0-2-DISPATCH-SLICING.md](L0-2-DISPATCH-SLICING.md). That doc captured the slicing rationale; this one refines the slicing with the post-#1459 doctrine + Joel's "e2e in Rust alone first" priority.
