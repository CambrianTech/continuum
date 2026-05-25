# Grid Migration Roadmap

**Status:** Live. Updated as PRs land.
**Architectural spec:** [`docs/architecture/GRID-BUS-ARCHITECTURE.md`](../architecture/GRID-BUS-ARCHITECTURE.md) (continuum#1439)
**Multi-peer commands spec:** [`docs/architecture/MULTI-PEER-COMMANDS.md`](../architecture/MULTI-PEER-COMMANDS.md) (continuum#1440 + #1441)
**Alloy generalization design:** [`docs/architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md`](../architecture/FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md)
**Trust+contract layer:** [`docs/grid/FORGE-ALLOY-PROOF-CONTRACTS.md`](./FORGE-ALLOY-PROOF-CONTRACTS.md)

---

## Status (auto-updateable from checkbox state)

| Layer | Complete | Total | % |
|---|---|---|---|
| L1 Foundation (substrate) | 0 | 6 | 0% |
| L2 Chat migration (chat-out-of-ORM finish) | 0 | 5 | 0% |
| L3 Alloy refactor (Domain Extensibility) | 0 | 3 | 0% |
| L4 Per-command opt-in (Phases A–G) | 0 | 18 | 0% |
| L5 Patch deletion (cleanup) | 0 | 5 | 0% |
| **OVERALL** | **0** | **37** | **0%** |

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
L1 Foundation (substrate)
  ├── L1-1 EventClass registry
  ├── L1-2 AircEventTransport ──────────┐
  ├── L1-3 CommandBase.naturalScope ────┤
  ├── L1-4 presence:peer-manifest ──────┤
  ├── L1-5 grid-router-daemon (needs L1-3 + L1-4)
  └── L1-6 contract event chain (needs L1-4)
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

---

## Layer 1: Foundation (substrate)

**Why first:** every other layer depends on these primitives. No L2-L5 PR lands before L1 is green.

- [ ] **L1-1** (card `935a58b8-99cf-4c53-87fc-71ee543c694e`): EventClass declaration system + registry
  - **Card:** (see card on the row above)
  - **Scope:** `src/system/events/EventClass.ts` + `EventClassRegistry.ts`. Typed event class declarations with `broadcast`, `channel`, `schemaVersion` metadata. `Events.emit()` consults registry to pick transport(s).
  - **Spec ref:** GRID-BUS-ARCHITECTURE §2.2 + §6.2
  - **Depends:** none
  - **Owner suggestion:** tab-2 (TS-only)
  - **Est:** 2-3 days
  - **Done = :** EventClass declarations accepted; `Events.emit()` reads metadata; existing event uses continue working unchanged (backward-compat); unit tests for the registry + classifier round-trip.

- [ ] **L1-2** (card `4f4e77d9-c00a-4062-8f12-580b07752642`): AircEventTransport adapter
  - **Card:** (see card on the row above)
  - **Scope:** `src/system/events/transports/AircEventTransport.ts`. Implements existing `EventTransport` interface. Outbound: `Events.emit()` → publishes to appropriate airc channel. Inbound: airc events past local cursor → `Events.checkWildcardSubscriptions()`. Persists cursor per-subscriber for restart-safe replay.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §6.1 + §3.1 (matches the proven shape from Lane C2's #1434 design, now framed as a transport)
  - **Depends:** L1-1
  - **Owner suggestion:** claude-tab-1 / 55c30b28 (Lane C2 author — has the airc-lib trait shapes already)
  - **Est:** 3-5 days
  - **Done = :** event round-trips A→B across two machines; cursor persists across restart; no `chat_messages` writes side-effect; integration test covers the round-trip.

- [ ] **L1-3** (card `e7b4f8ec-64c5-4b9a-b294-91541784ed25`): CommandBase.naturalScope + CommandParams.scope
  - **Card:** (see card on the row above)
  - **Scope:** Rename `naturalEnvironment` → `naturalScope` with backward-compat shim mapping old values. Add `scope` field to `CommandParams`. `Commands.execute()` resolves effective scope from class + per-call override. `remoteExecute()` learns the third (grid) path.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §2.1
  - **Depends:** none (orthogonal to L1-1; can land in parallel)
  - **Owner suggestion:** tab-2 (TS CommandBase) + codex/543c0bf7 (Rust kernel grid-path handler)
  - **Est:** 2-3 days
  - **Done = :** `PingCommand` annotated `naturalScope: 'grid'`; `PingCommand.execute({}, { scope: { target: 'grid', peer_id: '<other>' } })` returns the other peer's info; old `naturalEnvironment` callers still work.

- [ ] **L1-4** (card `9762c4db-561d-4258-8094-9d99a5818db9`): `presence:peer-manifest` event class + capability index
  - **Card:** (see card on the row above)
  - **Scope:** Manifest schema (offers/wants/terms/signatures per GRID-BUS §4). Folder maintains per-peer latest-manifest view; indexed by capability for dispatcher lookup. Rust canonical state + TS read-side bindings.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §4 + MULTI-PEER-COMMANDS §6.2 (liveness + withdrawal)
  - **Depends:** L1-1 + L1-2
  - **Owner suggestion:** codex/543c0bf7 (Rust state) — overlaps naturally with #1007 budgeted-context work
  - **Est:** 3-5 days
  - **Done = :** two peers boot, each sees the other's manifest in their local index; `grid/show-routes` introspection lists capabilities by peer; capability-withdrawn event removes the offer; integration test for join → exchange → withdrawal cycle.

- [ ] **L1-5** (card `d90d9844-2616-430e-82c2-2fa092840f11`): `grid-router-daemon` + bid loop
  - **Card:** (see card on the row above)
  - **Scope:** `src/daemons/grid-router-daemon/`. Subscribes to peer-manifest + resource-pressure + peer-departed events. Maintains routing table. Runs local policy engine. Implements bid loop (`command:bid-request` → `:bid-response` → `:bid-accepted`/`:bid-released`). Handles routed-command forwarding (multi-hop with `forwarded_by` loop detection).
  - **Spec ref:** GRID-BUS-ARCHITECTURE §3 + §4.1 + §11.1
  - **Depends:** L1-3 + L1-4
  - **Owner suggestion:** codex (Rust router logic) + tab-2 (TS daemon scaffolding)
  - **Est:** 5-7 days
  - **Done = :** laptop persona dispatches `inference/run` with `requires: { capability: '...' }`; router resolves to GPU peer; result returns within `max_latency_ms`; introspection (`grid/show-routes`, `grid/show-recent-dispatches`) exposes the decision trace.

- [ ] **L1-6** (card `e25898e6-8690-46dc-9693-c67d65b60f6e`): Contract event chain + ed25519 signatures
  - **Card:** (see card on the row above)
  - **Scope:** Event classes: `contract:proposed` / `:bid` / `:accepted` / `:executing` / `:delivered` / `:verified` / `:paid` / `:disputed`. Signed envelopes (ed25519). Reference `alloy_hash` for the substance of what's being contracted. Audit-replayable from airc cursor.
  - **Spec ref:** GRID-BUS-ARCHITECTURE §4.4 + MULTI-PEER-COMMANDS §7
  - **Depends:** L1-4 (needs peer signing keys from manifest) + L1-2 (broadcast transport)
  - **Owner suggestion:** tab-2 (event classes + TS signing) + codex (Rust signature verify)
  - **Est:** 3-5 days
  - **Done = :** end-to-end contract chain — proposed → bid → accepted → executed → delivered → verified → paid — for a `ping` grid dispatch with zero-LP household terms; airc cursor replay reproduces the chain bit-equivalently.

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

- [ ] **L5-1**: Delete `src/scripts/continuum-airc-bridge.mjs`
  - **Depends:** L1-2 (transport) operational + at least one airc-sourced event flowing through it
  - **Est:** half-day

- [ ] **L5-2**: Delete airc-prefixed IPC commands in `modules/airc.rs` (`airc/queue-scan`, `airc/realtime-publish`, `airc/realtime-replay`)
  - **Depends:** L4 commands using `Events.subscribe('chat:posted')` for everything that used `airc/realtime-replay` historically
  - **Est:** 1 day

- [ ] **L5-3**: Delete `src/workers/continuum-core/src/persona/airc_admission.rs`
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
