# L0-2 Dispatch Slicing — Delete-As-We-Go

**Status:** design — refines [GRID-MIGRATION-ROADMAP](GRID-MIGRATION-ROADMAP.md) L0-2 into shippable slices.
**Doctrine:** Joel 2026-05-29 — *no fallbacks, we delete, obsessive elegance, reduce kloc.*
**Predecessor:** L0-1 (#1457, merged) — `PersonaServiceModule` minimum unit.

## The kloc-reduction budget

| Path | Lines |
|---|---|
| `PersonaUser.ts` | 2,385 |
| `PersonaAutonomousLoop.ts` | 358 |
| `PersonaTaskExecutor.ts` | 1,438 |
| `system/user/server/modules/**/*.ts` | 23,429 |
| **L0-5 final TS cull target** | **≈27,610 lines deleted** |

This is the reason the migration is worth shipping. Net Rust added is far smaller than the TS deleted — the Rust path replaces *and* eliminates the orchestration overhead that the TS path carries.

## Why slice (and why this slicing)

A single "L0-2" PR replacing all of `handleItem` + bookmarks + adapter routing + dispatch + executor + every cognition import would be 5k+ lines of Rust against 4k lines of TS deletion. Unreviewable, untestable, single-failure-mode-bricks-the-merge. The doctrine says delete-as-we-go, not delete-all-at-once.

Each slice below is shippable in isolation, leaves the tree green, and deletes its proportional TS counterpart in the same PR. **No "Rust path + TS fallback"** at any boundary — the boundary moves as the slice lands.

## Slice ordering and contents

### L0-2a — Pop+emit shell

**Adds (Rust):**
- `PersonaSlot { persona_id, display_name, channels: ChannelRegistry, persona_state: PersonaState, cognition: PersonaCognition }`
- `PersonaServiceModule::enroll` opens (no longer returns `Err("L0-2 not yet wired")`); takes `rag_engine` from `ModuleContext::initialize`
- `service_once_for(slot)` pops via `channel_registry.service_cycle()` and **emits the item to the runtime event bus**. No cognition dispatch yet — emit-only.
- Per-persona circuit breaker (5 consecutive failures → 30s cooldown) + drain bound (20/tick)

**Tests:** 8 — enroll/idempotency, status reflects enrolled list, emit on pop, circuit breaker trips on N errors, cooldown timer, multi-persona fairness, no item-loss on emit-fail (`pop`'d item travels with the error).

**Deletes (TS):** nothing yet. This slice exists to give L0-2b a place to attach without TS fallback.

**Bench/VDD:** the singleton-tick-15-personas-sustained synthesizer (matches peer's chat-layer bench shape). Assert: per-tick CPU on the module < 50 µs at 5 msg/s sustained across 15 personas.

### L0-2b — Message dispatch + `PersonaAutonomousLoop.ts` deletion

**Adds (Rust):**
- Subscriber on the L0-2a emit-event that dispatches `InboxMessageItem` items through `PersonaCognitionEngine` (extends with `process_message(slot, item) -> Result<Response, DispatchError>` — net new method, ≈80 LOC)
- Bookmark advance via `Drop` guard / explicit always-run (no `try/catch swallow`)
- Domain classification result is propagated as a *result* — failure surfaces, doesn't get swallowed
- LoRA adapter activation routed via `genome_engine.activate_for_domain(classification)`

**Tests:** 12 — message → response happy path, classify-fail propagates as DispatchError (no silent catch), bookmark advances on success AND on dispatch error AND on panic-during-dispatch, ghost-message handling (item refers to deleted message) returns `Skipped` not `Err`.

**Deletes (TS):**
- `PersonaAutonomousLoop.ts` — **358 lines**
- All imports in `PersonaUser.ts`, `autonomous-learning-e2e.test.ts`, `PersonaTaskExecutor.ts`
- `evaluateAndPossiblyRespondWithCognition` wrapper in `PersonaUser.ts` (replaced by Rust path) — *N* lines
- The 3 fallbacks in TS `handleItem`: classify-catch, task-domain-fallback, response-catch-swallow

**Bench/VDD:** end-to-end "15 personas in general room, 5 msg/s, all respond" — assert p99 response latency, assert ZERO ghost retries.

### L0-2c — Task dispatch + `PersonaTaskExecutor.ts` deletion

**Adds (Rust):**
- Subscriber for `TaskItem` variant from L0-2a emit-event
- `process_task(slot, task) -> TaskOutcome` — net new method on `PersonaCognitionEngine` or a sibling `PersonaTaskRunner` (decide which by reading the TS — if it shares state with cognition, same module; if not, sibling)
- Stale-task check (read-then-update) preserved — that's data correctness, not a fallback

**Tests:** 10 — task → in_progress, task → completed, task-vanished-between-read-and-update returns `Skipped`, multi-task drain bound respected.

**Deletes (TS):**
- `PersonaTaskExecutor.ts` — **1,438 lines**
- Task-related callsites in `PersonaUser.ts`

### L0-3 / L0-4 / L0-5

Sized as separate roadmap items already. L0-2's job is to retire the dispatch path; L0-3+ retire the supporting infrastructure that no longer has callers.

## Validation discipline (VDD)

Per Joel 2026-05-29 + peer's #1077/#1079/#1083 methodology — **bench before changing, bench after changing, ship the number not the hypothesis**.

For each slice:
1. Bench against the CURRENT TS path first (baseline number).
2. Land the Rust path under a `#[cfg(feature = ...)]` ONLY long enough to A/B the bench. **NEVER ship the feature flag as a runtime config option** — runtime feature flags are fallbacks. The flag is dev-only, deleted in the same PR.
3. Bench the Rust path.
4. If Rust is not strictly faster, surface the truth — don't paper over it.
5. Delete the TS counterpart in the same PR. The bench harness for that slice can graduate to a regression test pinned at the measured threshold.

## What this doc is NOT

- Not a fallback gate. Each slice merges if and only if it's strictly green; no "if the Rust path errors, fall back to TS." Errors surface, the slice rolls back via revert.
- Not a contract negotiation. Sub-method signatures (`process_message`, `process_task`) are draft — I'll discover the right shape while building L0-2a's emit boundary.
- Not a separate roadmap. It refines L0-2 of [GRID-MIGRATION-ROADMAP](GRID-MIGRATION-ROADMAP.md); the line in that table that says "L0-2" will reference this doc once this lands.

## Next action

Open PR for L0-2a (pop+emit shell). Branch: `grid/l0-2a-pop-emit`. Base: `canary`.
