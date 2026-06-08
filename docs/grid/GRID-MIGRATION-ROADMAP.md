# Grid Migration Roadmap

**Status:** Live. Updated as PRs land.
**Architectural spec:** [`docs/architecture/GRID-BUS-ARCHITECTURE.md`](../architecture/GRID-BUS-ARCHITECTURE.md) (continuum#1439)
**Multi-peer commands spec:** [`docs/architecture/MULTI-PEER-COMMANDS.md`](../architecture/MULTI-PEER-COMMANDS.md) (continuum#1440 + #1441)
**Alloy generalization design:** [`docs/architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md`](../architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md)
**Trust+contract layer:** [`docs/grid/FORGE-ALLOY-PROOF-CONTRACTS.md`](./FORGE-ALLOY-PROOF-CONTRACTS.md)

---

## Architectural ground rules (Joel directives 2026-05-29)

These are non-negotiable across every layer below. They are why the migration EXISTS, not nice-to-haves.

1. **Rust core; Node.js is web only.** Node.js exists for browser UI, config-loading at boot, and human UX. Nothing else. Anything that handles routing, persistence, inference, command dispatch, or persona reasoning lives in Rust (`core/continuum-core/` and sibling crates). The TS layer is the thin web edge — `Commands.execute()` / `Events.emit()` calls into Rust via the existing IPC; rendering reads back.
2. **AI persona under Rust domain.** `system/user/server/PersonaUser.ts` (2312 LOC) and its orchestrators were CPU-killing the box (V8 single-threaded loop blocking on every reasoning step, JSON marshalling per IPC). Migration target is `continuum-core/src/persona/` — much of which is already Rust (`channel_registry`, `inbox`, `evaluator`, `cognition`, `prompt_assembly`, `genome_paging`). What remains in TS is the orchestrator and dispatchers; those move. See **Layer 0** below.
3. **GPU or fail for inference.** No CPU-only inference path; `llama` crate refuses to build on macOS without `--features metal` by design. Same for training (candle Metal/CUDA). Performant inference cannot exist without GPU acceleration; performant training even more so.
4. **No `dyn Any` / `as_any` patterns.** Type erasure via `Any` hides the wire shape that ts-rs needs to reflect and obscures Rust performance characteristics. When a current trait requires `as_any`, that's debt — file a card to redesign the trait, don't propagate the pattern.
5. **ts-rs is the bindings source of truth.** Rust types are canonical; TypeScript bindings are generated via `#[derive(TS)]` + `cargo test` triggering ts-rs into `shared/generated/`. NEVER hand-write a TS type that crosses the Rust↔TS boundary. The Rust struct is the schema; the TS is a projection.
6. **Inference is llama.cpp through-and-through.** Never ollama, never suggest ollama. Candle stays for training, Orpheus TTS, and legacy backends. Inference flows through the `llama` crate against vendored llama.cpp (`core/vendor/llama.cpp`).

Every roadmap item below is read through these rules. Owner-suggestion text from the original draft (which still said "TS-only" for several Rust-target items) has been updated.

---

## Status (auto-updateable from checkbox state)

| Layer | Complete | Total | % |
|---|---|---|---|
| L0 Persona → Rust migration (CPU win) | 0 | 5 | 0% |
| L1 Foundation (substrate) | 0 | 6 | 0% |
| L2 Chat migration (chat-out-of-ORM finish) | 0 | 5 | 0% |
| L3 Alloy refactor (Domain Extensibility) | 0 | 3 | 0% |
| L4 Per-command opt-in (Phases A–G) | 0 | 18 | 0% |
| L5 Patch deletion (cleanup) | 0 | 5 | 0% |
| **OVERALL** | **0** | **42** | **0%** |

---

## How to use this doc

**For PR authors:**

1. Each PR title format: `[L#-N] short title` — e.g. `[L1-2] AircEventTransport adapter`
2. Each PR body opens with: `Closes roadmap item L#-N` (one per PR; multiple allowed if naturally bundled)
3. Each PR body links back to `docs/grid/GRID-MIGRATION-ROADMAP.md` and the relevant architecture-doc section
4. Each PR body confirms the dependency: `Depends on: L#-X (status: ✅ merged | ⏳ in-progress | ❌ blocked)`
5. If the PR adds a NEW roadmap item not on this list, also amend this doc in the same PR

**For PR mergers / reviewers:**

1. When PR merges, check off `- [x]` the item(s)
2. Append the merge metadata: `merged: <yyyy-mm-dd> <PR#>`
3. Update the per-layer counter in the Status table
4. If the merge unblocks a downstream item, post on `#cambriantech` so the owner can pick it up

**For peers / observers:**

- `grep "^- \[ \]"` shows everything still open
- `grep "^- \[x\]"` shows everything done
- Card IDs map 1:1 to the kanban (`airc work board` to see live status)

---

## Dependency graph (high-level)

```
L0 Persona → Rust migration (CPU win, parallel to L1)
  ├── L0-1 PersonaServiceModule (ServiceModule wrapper for service_cycle)
  ├── L0-2 cognition dispatch in Rust (queue-item → response_orchestrator)
  ├── L0-3 PersonaGenomeManager → Rust (LoRA activation in-process)
  ├── L0-4 PersonaInbox routing in Rust (eliminate TS service-loop IPC)
  └── L0-5 PersonaAutonomousLoop deletion (TS shell becomes thin shim)

L1 Foundation (substrate) — Rust core; TS is browser projection only
  ├── L1-1 EventClass registry (Rust types + ts-rs)
  ├── L1-2 AircEventTransport (Rust impl; TS shim subscribes for browser)
  ├── L1-3 CommandBase.naturalScope (Rust kernel; TS surface generated)
  ├── L1-4 presence:peer-manifest (Rust canonical state + ts-rs view)
  ├── L1-5 grid-router-daemon (Rust router) (needs L1-3 + L1-4)
  └── L1-6 contract event chain (Rust signing + verify) (needs L1-4)
              │
              ▼
L2 Chat migration (needs L1-1, L1-2)
  ├── L2-1 message_admission.rs (replace airc_admission)
  ├── L2-2 UI subscribe(chat:posted)
  ├── L2-3 delete chat_messages collection ⚠ irreversible
  ├── L2-4 revert dual-write PR stack
  └── L2-5 webrtc/presence/media event classes (same shape)

L3 Alloy refactor (independent of L1; gates Phase F of L4)
  ├── L3-1 forge-alloy domain registry (WI 0+1+2 of EXTENSIBILITY)
  ├── L3-2 Continuum-side TS regen + Factory widget (WI 3)
  └── L3-3 regression test + docs (WI 4+5)

L4 Per-command opt-in (Phases A–G from MULTI-PEER §8.2)
  Phase A — proof of life (needs L1 foundation)
  Phase B — single-peer compute, household tier
  Phase C — single-peer compute, trusted-orgs tier (needs L1-6 contract chain)
  Phase D — canonical multi-peer: genome paging cross-peer
  Phase E — multi-quorum: vector-search fan-out, federated training
  Phase F — non-ML alloy contracts (needs L3 alloy refactor)
  Phase G — distributed forge runs (needs L3 + L4-Phase-E)

L5 Patch deletion (interleaved with L2-L4 as upstreams complete)
  ├── L5-1 continuum-airc-bridge.mjs
  ├── L5-2 modules/airc.rs IPC commands
  ├── L5-3 persona/airc_admission.rs
  ├── L5-4 src/system/airc-chat/ directory
  └── L5-5 ChatMessageEntity + chat_messages ORM
```

**Hard prerequisite chains:**
- L1 → L2 (entire chain)
- L1 → L4 (entire chain)
- L3 → L4-Phase-F + L4-Phase-G (non-ML alloy + distributed forge)
- L1-6 → L4-Phase-C+ (contract chain needed for paid tiers)
- L2-2 (UI on new events) → L2-3 (collection delete) — never delete the collection before its consumers migrate
- L0 is independent — runs parallel to L1, no cross-dependency. PersonaUser migration unblocks the CPU on every machine the user runs continuum on, immediately.

---

## Layer 0: Persona → Rust migration (CPU win)

**Why this layer:** the TS `PersonaUser` + its orchestrators were killing the CPU per Joel's 2026-05-29 directive. V8 single-threaded event loop blocked on every reasoning step; JSON marshalling on every IPC round-trip to Rust. With 15 personas active, the box was IPC-bound on persona logic before any inference even ran. The Rust persona implementation already exists (`continuum-core::persona::{channel_registry, inbox, evaluator, cognition, prompt_assembly, genome_paging}`) — this layer **finishes the migration that was 70% complete**, eliminating the TS-side service loops that were the actual CPU sink.

**Parallel to L1:** Layer 0 is independent of the substrate work (L1) — different files, different code paths. Both can ship simultaneously.

- [ ] **L0-1**: `PersonaServiceModule` — `ServiceModule` impl that owns the service cycle in-process
  - **Scope:** `continuum-core/src/persona/service_module.rs`. Wraps `ChannelRegistry::service_cycle()` + `PersonaState` under the runtime's `ServiceModule` trait. Tick at 250ms (matches TS cadence floor) runs the cycle inside the Rust runtime, no IPC. Commands: `persona/<id>/status`, `persona/<id>/drain-now`. Circuit breaker mirrors the TS shape (5 consecutive errors → 30s cooldown).
  - **Status:** Initial commit shipped to branch `continuum-core-airc-embed` (2026-05-29). Build verification blocked on workspace state.
  - **Depends:** none (uses existing Rust persona modules)
  - **Est:** 1 day (already scaffolded; needs cognition-dispatch glue from L0-2)
  - **Done = :** module registers; tick drives `service_cycle()`; `persona/<id>/status` returns JSON snapshot; TS `PersonaAutonomousLoop` can be replaced with a thin shim that just spawns this module.

- [ ] **L0-2**: Cognition dispatch in Rust — translate queue items → `response_orchestrator` input
  - **Scope:** Replace the current TODO in `PersonaServiceModule::service_once` with real dispatch. The Rust `cognition::response_orchestrator` already exists; this is the wiring from a `ServiceCycleResult.item` (JSON value from a `Box<dyn QueueItemBehavior>`) into the orchestrator's request shape + writing the response back to the persona's output channel.
  - **Depends:** L0-1
  - **Est:** 2-3 days
  - **Done = :** dispatching an inbox item runs through cognition in Rust end-to-end without a TS IPC hop; same response shape as today's TS path; integration test with a synthetic inbox item.

- [ ] **L0-3**: `PersonaGenomeManager` → Rust (LoRA activation in-process)
  - **Scope:** Move LoRA paging activation from `system/user/server/modules/PersonaGenomeManager.ts` into `continuum-core/src/persona/genome_paging.rs` (the engine already exists; the orchestration layer needs to move). Activation must be in-process so a service tick that needs a new adapter doesn't pay IPC overhead.
  - **Depends:** L0-1 (service module is the caller)
  - **Est:** 3-5 days
  - **Done = :** an inbox item whose domain needs an adapter not currently active triggers paging in the Rust tick; adapter is loaded into llama crate's context; cognition dispatch uses it; no TS roundtrip on the hot path.

- [ ] **L0-4**: `PersonaInbox` routing fully in Rust (eliminate TS service-loop signaling)
  - **Scope:** Today `PersonaInbox.waitForWork()` is a TS signal that blocks the service loop. With the loop in Rust (L0-1), the waiting can be a tokio condvar/notify directly on the channel queue. Delete the TS signal plumbing once everything subscribed to it moves to the Rust path.
  - **Depends:** L0-1 + at least one consumer migrated
  - **Est:** 2-3 days
  - **Done = :** Rust tick wakes immediately on enqueue; no TS-side `waitForWork` calls remain in `PersonaUser`; signal-channel plumbing in `PersonaInbox.ts` deleted.

- [ ] **L0-5**: Delete `PersonaAutonomousLoop.ts` (TS shell → thin shim or full delete)
  - **Scope:** Once L0-1 through L0-4 are live, `PersonaAutonomousLoop.ts` and the `RustCognitionBridge.serviceCycleFull()` hot-path call are obsolete. The TS PersonaUser becomes a thin shim that creates the Rust persona at startup (one IPC call) and subscribes to "persona response ready" events for widget rendering.
  - **Depends:** L0-1 + L0-2 + L0-3 + L0-4
  - **Est:** 1 day
  - **Done = :** `PersonaAutonomousLoop.ts` deleted; `RustCognitionBridge.serviceCycleFull` IPC command removed; TS `PersonaUser` is < 500 LOC (down from 2312); a 15-persona profiled run shows the V8 main-thread blocking that prompted this layer is GONE.

**L0 exit criteria:** all 5 items checked; a 15-persona profiled run on the Intel Mac (2017) shows V8 main-thread CPU drop measurably (target: 60%+ reduction in the persona service-loop call stack), and a single-persona response latency from inbox-enqueue to response-emit is < 50ms (down from current ~150-300ms median).

---

## Layer 1: Foundation (substrate)

**Why first:** every other layer depends on these primitives. No L2-L5 PR lands before L1 is green. **Owner-suggestions reflect Joel's rust-core / web-only-TS directive — items that the original draft scoped as "tab-2 (TS-only)" are now Rust-primary with thin TS shims for browser concerns.**

- [ ] **L1-1** (card `935a58b8-99cf-4c53-87fc-71ee543c694e`): EventClass declaration system + registry
  - **Card:** (see card on the row above)
  - **Scope:** `continuum-core/src/events/event_class.rs` + `event_class_registry.rs` (Rust source of truth) + `#[derive(TS)]` to emit `shared/generated/code/EventClass.ts` etc. `src/system/events/EventClass.ts` becomes a re-export of the generated types. `Events.emit()` (TS) reads the generated registry; the Rust runtime reads the same registry for cross-process traffic.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §2.2 + §6.2
  - **Depends:** none
  - **Owner suggestion:** Rust kernel (continuum-core) + ts-rs binding pass. Browser-edge subscription wiring is the only TS-touched piece.
  - **Est:** 2-3 days
  - **Done = :** EventClass declarations live in Rust; ts-rs emits TS types; `Events.emit()` reads metadata; existing event uses continue working unchanged (backward-compat); unit tests in Rust for the registry round-trip; ts-rs-generated TS types compile against existing `Events.subscribe()` callers.

- [ ] **L1-2** (card `4f4e77d9-c00a-4062-8f12-580b07752642`): AircEventTransport adapter
  - **Card:** (see card on the row above)
  - **Scope:** Rust `continuum-core/src/airc/event_transport.rs` impls `airc_lib::adapter::ConsumerAdapter` against airc PR #1075's trait, registered via `Airc::register_adapter` (airc PR #1081). Outbound: continuum-core's event bus publishes to airc via `Airc::publish` (or the typed-publish API once it lands). Inbound: airc's dispatch task delivers envelopes whose `forge.body_hint = forge.continuum.event.v1` to the adapter's `on_envelope`. TS shim in `src/system/events/transports/AircEventTransport.ts` is a thin pass-through that subscribes to the Rust core's "incoming event" notification — browser-side only.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §6.1 + §3.1 (matches the proven shape from Lane C2's #1434 design, now framed as a transport)
  - **Depends:** L1-1, plus airc PR #1075 (ConsumerAdapter trait) + #1081 (dispatch wire) merged
  - **Owner suggestion:** Rust adapter impl (continuum-core/airc) primary; TS shim is browser-side projection. Lane C2's prior design is the contract reference, not the implementation surface.
  - **Est:** 3-5 days
  - **Done = :** event round-trips A→B across two machines THROUGH RUST (no TS in the hot path); cursor persists across restart; no `chat_messages` writes side-effect; integration test in `continuum-core` covers the round-trip with the existing `ContinuumAdapter`.

- [ ] **L1-3** (card `e7b4f8ec-64c5-4b9a-b294-91541784ed25`): CommandBase.naturalScope + CommandParams.scope
  - **Card:** (see card on the row above)
  - **Scope:** Source of truth is Rust `CommandSpec` (in continuum-core's command kernel) extended with `natural_scope` + per-call `scope`. ts-rs generates the TS surface. The TS `CommandBase` becomes a thin generated re-export + backward-compat shim mapping old `naturalEnvironment` to `naturalScope` for callers that haven't migrated. `Commands.execute()` (TS) reads the generated registry; the actual scope resolution + dispatch happens in Rust. `remoteExecute()` (Rust) learns the third (grid) path.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §2.1
  - **Depends:** none (orthogonal to L1-1; can land in parallel)
  - **Owner suggestion:** Rust kernel primary (continuum-core command spec + dispatch). TS shim is generated + a small backward-compat mapper, not authored.
  - **Est:** 2-3 days
  - **Done = :** `PingCommand` annotated `natural_scope: "grid"` in Rust (TS sees it through ts-rs); `PingCommand.execute({}, { scope: { target: 'grid', peer_id: '<other>' } })` returns the other peer's info; old `naturalEnvironment` callers still work via the generated shim.

- [ ] **L1-4** (card `9762c4db-561d-4258-8094-9d99a5818db9`): `presence:peer-manifest` event class + capability index
  - **Card:** (see card on the row above)
  - **Scope:** Rust source of truth for manifest schema (`#[derive(TS)]`) + per-peer latest-manifest folder + capability index. All consumers (Rust router, TS browser introspection) read the same generated types. No hand-written TS schema duplication.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §4 + MULTI-PEER-COMMANDS §6.2 (liveness + withdrawal)
  - **Depends:** L1-1 + L1-2
  - **Owner suggestion:** Rust kernel (continuum-core::grid::manifest). Overlaps naturally with #1007 budgeted-context work.
  - **Est:** 3-5 days
  - **Done = :** two peers boot, each sees the other's manifest in their local index; `grid/show-routes` (Rust command, ts-rs surface) lists capabilities by peer; capability-withdrawn event removes the offer; integration test in Rust for join → exchange → withdrawal cycle.

- [ ] **L1-5** (card `d90d9844-2616-430e-82c2-2fa092840f11`): `grid-router-daemon` + bid loop
  - **Card:** (see card on the row above)
  - **Scope:** Rust `continuum-core/src/grid/router.rs` (and a thin daemon entrypoint if a separate process is needed; otherwise an in-process ServiceModule). Subscribes to peer-manifest + resource-pressure + peer-departed events. Maintains routing table. Runs local policy engine in Rust. Implements bid loop (`command:bid-request` → `:bid-response` → `:bid-accepted`/`:bid-released`). Handles routed-command forwarding (multi-hop with `forwarded_by` loop detection). NO TS daemon scaffolding — the router lives entirely in continuum-core; if process isolation is wanted it's a Rust binary.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §3 + §4.1 + §11.1
  - **Depends:** L1-3 + L1-4
  - **Owner suggestion:** Rust kernel only. The "TS daemon scaffolding" suggestion from the original draft is OBSOLETE — Node daemons that own routing semantics are exactly what Joel's "no node for core features" directive removes.
  - **Est:** 5-7 days
  - **Done = :** laptop persona dispatches `inference/run` with `requires: { capability: '...' }`; Rust router resolves to GPU peer; result returns within `max_latency_ms`; introspection (`grid/show-routes`, `grid/show-recent-dispatches` — Rust commands with ts-rs surface) exposes the decision trace.

- [ ] **L1-6** (card `e25898e6-8690-46dc-9693-c67d65b60f6e`): Contract event chain + ed25519 signatures
  - **Card:** (see card on the row above)
  - **Scope:** Rust event classes (`#[derive(TS)]`): `contract:proposed` / `:bid` / `:accepted` / `:executing` / `:delivered` / `:verified` / `:paid` / `:disputed`. Signed envelopes (ed25519) in Rust — both signing AND verify, no TS-side crypto on the hot path. Reference `alloy_hash` for the substance of what's being contracted. Audit-replayable from airc cursor.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §4.4 + MULTI-PEER-COMMANDS §7
  - **Depends:** L1-4 (needs peer signing keys from manifest) + L1-2 (broadcast transport)
  - **Owner suggestion:** Rust kernel (contracts module, ed25519 sign + verify both Rust). TS event-class projection is ts-rs-generated.
  - **Est:** 3-5 days
  - **Done = :** end-to-end contract chain — proposed → bid → accepted → executed → delivered → verified → paid — for a `ping` grid dispatch with zero-LP household terms; ALL crypto in Rust; airc cursor replay reproduces the chain bit-equivalently.

**L1 exit criteria:** all 6 items checked; two-peer smoke test passes (laptop ↔ bigmama-wsl): cross-grid ping, capability advertisement visible both ways, contract event chain replayable from airc cursor.

---

## Layer 2: Chat migration (finishes the chat-out-of-ORM work)

**Why this layer:** the current shim/patch architecture sneaks chat back into ORM. L2 completes the original migration by deleting the patch.

- [ ] **L2-1**: `persona/message_admission.rs` subscribes to `chat:posted` (replace `airc_admission.rs`)
  - **Spec ref:** GRID-BUS-ARCHITECTURE §5.1 + §5.3 step 6
  - **Depends:** L1-1 + L1-2
  - **Est:** 2-3 days
  - **Done = :** persona reacts to airc-sourced chat identically to local-emit-sourced; `persona/airc_admission.rs` no longer imported anywhere (delete in L5-3).

- [ ] **L2-2**: UI widgets subscribe to `chat:posted` for display + airc-cursor tail-N replay on mount
  - **Spec ref:** GRID-BUS-ARCHITECTURE §5.3 step 7
  - **Depends:** L1-1 + L1-2
  - **Est:** 3-5 days
  - **Done = :** chat-widget shows new messages from `Events.subscribe('chat:posted', ...)`; backfill on mount via airc cursor read; no ORM scan against `chat_messages` from the UI path.

- [ ] **L2-3**: ⚠ Delete `chat_messages` ORM collection + `ChatMessageEntity.ts`
  - **Spec ref:** GRID-BUS-ARCHITECTURE §5.3 step 8 — **irreversible**
  - **Depends:** L2-1 + L2-2 (all consumers migrated)
  - **Est:** 1-2 days
  - **Done = :** collection removed from `EntityRegistry`; nothing imports `ChatMessageEntity`; ORM working-set on a 7-day persona-busy machine drops measurably (target: 30%+ row-count reduction).

- [ ] **L2-4**: Revert dual-write PR stack (#1432/#1433/#1435/#1436/#1437)
  - **Spec ref:** GRID-BUS-ARCHITECTURE §5.3 step 9 + §5.1 deletion list
  - **Depends:** L2-1 + L2-2 + L2-3 (the shim it patches is gone)
  - **Est:** 2 days
  - **Done = :** `src/system/airc-chat/` directory deleted; chat send writes only to airc (no parallel store); smoke test confirms airc is the canonical event log; #1432-#1437 closed as superseded.

- [ ] **L2-5**: Same shape for `webrtc:*`, `presence:*`, `media:*` event classes
  - **Spec ref:** GRID-BUS-ARCHITECTURE §5.3 step 10 + §3.3
  - **Depends:** L2-3 (proves the pattern works for chat first)
  - **Est:** 3-5 days
  - **Done = :** WebRTC signaling moves to event-bus; presence + media-frame keepalives use airc; no ORM rows for any of these classes; live audio call between two peers with signaling over airc.

---

## Layer 3: Alloy refactor (forge-alloy Domain Extensibility — prerequisite for non-ML contracts)

**Why this layer:** the current Continuum-side forge alloy types are model-bound (drift from the universal-from-day-one intent). Non-ML use cases (sentinel scans, wallet receipts, code-gen attestation, payment ledger anchors) gate on this refactor.

**Per [`FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md`](../architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md) work items 0-5.**

- [ ] **L3-1**: forge-alloy domain registry refactor (work items 0 + 1 + 2)
  - **Scope:** `forge-alloy` repo gets the domain-registry refactor; `llm-forge` becomes an extension; Continuum-side TS types regenerated from forge-alloy.
  - **Spec ref:** FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md
  - **Depends:** none (independent of L1)
  - **Est:** 1.5 hours (per scoped estimate in the spec)
  - **Done = :** universal alloy core lives in `forge-alloy/src/core/`; ML stages live in `forge-alloy/src/domains/llm-forge/`; Continuum imports the regenerated TS types; existing alloy code untouched.

- [ ] **L3-2**: Domain-aware Factory widget (work item 3)
  - **Spec ref:** FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md WI 3
  - **Depends:** L3-1
  - **Est:** 1 hour
  - **Done = :** Factory widget loads + saves a published `.alloy.json` byte-equivalently through the new domain-aware schema; UI handles the `llm-forge` domain as a first-class first-party plugin.

- [ ] **L3-3**: Backwards-compatibility regression test + docs refresh (work items 4 + 5)
  - **Spec ref:** FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md WI 4 + 5
  - **Depends:** L3-1 + L3-2
  - **Est:** 1 hour
  - **Done = :** all 3 shipped continuum-ai/* alloys + every `forge-alloy/examples/` alloy round-trip byte-equivalently through the new schema; docs reflect the new shape; `FORGE-ALLOY-SPEC.md` cross-references the domain-extension structure.

**L3 exit criteria:** Continuum can emit non-ML alloys (sentinel scan, wallet receipt, payment ledger anchor) using `0x05` / `0x06` / `0xFF` domains. Bit-equivalent regression test green on every existing artifact.

---

## Layer 4: Per-command opt-in (Phases A–G from MULTI-PEER-COMMANDS §8.2)

**Why this layer:** each existing command opts into the grid by flipping metadata (`naturalScope: 'grid'`) and shipping its capability advertisement. Most are 2-line changes (per MULTI-PEER §8.1 worked example).

### Phase A — proof of life

- [ ] **L4-A-1**: `ping` opts into grid (per MULTI-PEER §8.1 worked example)
  - **Depends:** L1 (all)
  - **Est:** half-day
  - **Done = :** laptop pings bigmama-wsl across grid; result has expected envelope shape; no LP contract needed (household-tier reciprocity).

- [ ] **L4-A-2**: `debug/system-info` opts into grid
  - **Depends:** L1 (all)
  - **Est:** half-day

- [ ] **L4-A-3**: `grid/show-routes`, `grid/show-policy`, `grid/show-recent-dispatches` introspection commands
  - **Depends:** L1-5
  - **Est:** 1 day

### Phase B — single-peer compute, household tier

- [ ] **L4-B-1**: `ai/generate` + `ai/embedding` opt into grid (single-peer, household)
  - **Depends:** L1 (all)
  - **Est:** 2-3 days
  - **Done = :** laptop persona infers against household GPU peer transparently; latency budget met; contract chain emits (no LP transfer in household tier).

- [ ] **L4-B-2**: `cognition/vision-describe` opts into grid (single-peer, household)
  - **Depends:** L4-B-1 (proves the pattern)
  - **Est:** 1-2 days

- [ ] **L4-B-3**: `voice/synthesize` + `voice/transcribe` opt into grid (single-peer, household)
  - **Depends:** L4-B-1
  - **Est:** 1-2 days

### Phase C — single-peer compute, trusted-orgs tier (first LP transfer)

- [ ] **L4-C-1**: Phase B commands extended with `accept_inbound_from: ['household', 'trusted-orgs']`
  - **Depends:** L1-6 (contract event chain) + Phase B done + at least one trusted-org peer configured
  - **Est:** 2-3 days
  - **Done = :** an inference dispatch to a trusted-orgs peer fires the full `contract:proposed → bid → accepted → executing → delivered → verified → paid` chain with non-zero LP; sentinel pre-flight optional but tested.

### Phase D — canonical multi-peer (genome paging cross-peer)

- [ ] **L4-D-1**: `genome/paging-activate` cross-peer (per MULTI-PEER §4.1)
  - **Depends:** L4-A done (proves Phase A ergonomics) + L1-5 (router)
  - **Est:** 5-7 days
  - **Done = :** persona on laptop activates an adapter that only lives on bigmama-wsl; FETCH vs DELEGATE policy choice exercised both ways; `RemoteResourceHandle` plumbing works end-to-end.

### Phase E — multi-quorum (fan-out + federated)

- [ ] **L4-E-1**: `data/vector-search` with `quorum: 'any', fan_out: true` (per MULTI-PEER §4.4)
  - **Depends:** L4-D-1 (proves multi-peer pattern + handles)
  - **Est:** 3-5 days

- [ ] **L4-E-2**: `genome/train` federated, `quorum: 'multi'` with FedAvg sync (per MULTI-PEER §4.3)
  - **Depends:** L4-E-1 (proves fan-out routing)
  - **Est:** 7-10 days
  - **Done = :** 2-peer federated LoRA training produces a converged adapter with provenance back to all contributing peers; final alloy references each peer's contract.

### Phase F — non-ML alloy contracts (gated on L3)

- [ ] **L4-F-1**: Sentinel scan emits `0xFF` custom-domain alloys (per MULTI-PEER §7.3)
  - **Depends:** L3 (entire) + L1-6
  - **Est:** 5-7 days

- [ ] **L4-F-2**: Wallet payment receipts emit `0xFF` custom-domain alloys (the LP-clears event)
  - **Depends:** L3 + L1-6 + first revenue-generating contract chain in Phase C
  - **Est:** 5-7 days

- [ ] **L4-F-3**: Code-generation attestation alloys (`0x06` evaluation domain)
  - **Depends:** L3 + L1-6
  - **Est:** 3-5 days

### Phase G — distributed forge runs (capstone)

- [ ] **L4-G-1**: `recipe/run` with parallel stages dispatched as multi-peer contracts (per MULTI-PEER §4.5)
  - **Depends:** Phase E-2 (federated training pattern) + Phase F (non-ML alloys for non-training stages)
  - **Est:** 10-15 days
  - **Done = :** a recipe with 4 parallelizable stages (calibration corpus embedding, importance profile, per-tier quantization sweep, per-benchmark eval) dispatches each to a different peer; parent alloy references all 4 stage alloys; total wall-clock time substantially less than single-peer.

---

## Layer 5: Patch deletion (interleaved with L2-L4 as upstreams complete)

**Why this layer:** the patches that L1-L4 supersede need to be removed, not left lying around. Each deletion gates on its replacement landing first.

- [ ] **L5-1**: Delete `tools/scripts/continuum-airc-bridge.mjs`
  - **Depends:** L1-2 (transport) operational + at least one airc-sourced event flowing through it
  - **Est:** half-day

- [ ] **L5-2**: Delete airc-prefixed IPC commands in `modules/airc.rs` (`airc/queue-scan`, `airc/realtime-publish`, `airc/realtime-replay`)
  - **Depends:** L4 commands using `Events.subscribe('chat:posted')` for everything that used `airc/realtime-replay` historically
  - **Est:** 1 day

- [ ] **L5-3**: Delete `core/continuum-core/src/persona/airc_admission.rs`
  - **Depends:** L2-1 (replacement `message_admission.rs` is live)
  - **Est:** half-day

- [ ] **L5-4**: Delete `src/system/airc-chat/` directory entirely (`AircChatMirrorMapper`, `AircChatDualWriteService`, `AircChatEnvelope`)
  - **Depends:** L2-4 (dual-write stack reverted)
  - **Est:** half-day

- [ ] **L5-5**: Delete `ChatMessageEntity.ts` + `chat_messages` collection registration
  - **Same as L2-3** — listed here for visibility in the deletion summary, checked off via L2-3.

---

## Glossary

| Term | Meaning |
|---|---|
| **AS** (Autonomous System) | A Continuum install. Has its own routing policy, peering relationships, dispatch decisions. |
| **Capability advertisement** | A peer's manifest entry declaring "I can serve `<capability>` at these terms." |
| **Circle** | Trust tier (local / household / trusted-orgs / extended / public-mesh). Per-call policy filters peers by circle. |
| **Contract event chain** | The sequence `proposed → bid → accepted → executing → delivered → verified → paid` on the airc log. Audit substrate. |
| **Forge alloy** | Universal Merkle-chain-of-custody artifact (per FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md). Not model-specific. |
| **`naturalScope`** | Class-level declaration on `CommandBase` of which transport tier a command supports. `local` / `environment` / `grid`. |
| **Peer manifest** | A peer's broadcast `presence:peer-manifest` event carrying hardware, offers, wants, terms, signatures. |
| **Routing table** | Per-peer view of the capability index — which peers offer which capabilities at which terms. Computed from manifest events. |
| **`scope`** | Per-call override on `CommandParams` of where this invocation runs. Includes `target`, `requires`, `peer_id`, `capability`, `policy`. |
| **Type Byte** | forge-alloy domain enum: `0x01` model forging, `0x05` delivery, `0x06` evaluation, `0xFF` custom. |

---

## References

- [`docs/architecture/GRID-BUS-ARCHITECTURE.md`](../architecture/GRID-BUS-ARCHITECTURE.md) — primary architectural spec
- [`docs/architecture/MULTI-PEER-COMMANDS.md`](../architecture/MULTI-PEER-COMMANDS.md) — multi-peer command shapes + handle distribution + hosting + migration
- [`docs/architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md`](../architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md) — L3 alloy refactor design
- [`docs/architecture/FORGE-ALLOY-SPEC.md`](../architecture/FORGE-ALLOY-SPEC.md) — current alloy spec (post-L3, reflects domain refactor)
- [`docs/grid/FORGE-ALLOY-PROOF-CONTRACTS.md`](./FORGE-ALLOY-PROOF-CONTRACTS.md) — trust + contract layer (input to L1-6 + L4-Phase-F)
- [`docs/UNIVERSAL-PRIMITIVES.md`](../UNIVERSAL-PRIMITIVES.md) — the `Commands.execute()` + `Events.subscribe/emit()` primitives the bus extends

---

## Change log

| Date | Change |
|---|---|
| 2026-05-25 | Initial roadmap (tab-2). 37 items across 5 layers. L1 cards seeded; L2-L5 cards to be created as upstreams unblock. |
