# Chat-to-AIRC Migration: Proof Gates

> Cards: continuum#1130, continuum#1253 · Branch: `codex/chat-sqlite-airc-substrate-1253`
>
> Companion to [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md) and [AIRC-CONTINUUM-BRIDGE.md](AIRC-CONTINUUM-BRIDGE.md). This document specifies what must be PROVEN — not just compiled — at each stage of moving Continuum's chat path from the ORM-backed `chat_messages` collection onto AIRC as the primary transport.

## Why this document exists

> "If chat send moves off ORM to AIRC, agents must manually prove UI behavior and JTAG/command callers before removing old chat commands. Compile-only is not enough." — Joel (proof-gate request, recorded on continuum#1130)

A naïve migration would: change `chat/send` to write into AIRC, leave the rest, and ship. That breaks the things compile-only checks don't surface — UI live updates, persona-inbox reads, ai/report aggregations, the data shape that DataLoader caches. **Each must be proven, individually, before the corresponding ORM dependency can be removed.**

This file is the explicit checklist that per-stage proofs must pass. It is not a design for the AIRC-side wire format; that lives in [AIRC-CONTINUUM-BRIDGE.md](AIRC-CONTINUUM-BRIDGE.md). It is not a re-spec of AIRC primitives; that lives in the airc repo.

---

## Seed inventory: where the ORM `chat_messages` path lives today

A migration without an inventory is a wishlist. This section is a **seed inventory**, not the authoritative migration inventory. A review grep on 2026-05-14 already found additional references outside the first draft, including sentinel pipelines, voice bridge, RAG/tool definitions, context search/slice commands, AIRC bridge, persona task/training modules, and docs.

The current generated inventory for continuum#1253 lives at
[generated/chat-to-airc-inventory.md](generated/chat-to-airc-inventory.md).
That generated artifact is the working source of truth for the next
Postgres-removal/chat migration PRs. This seed section remains here to explain
the categories and proof gates.

The first proof — required before any code change — is a regenerated machine inventory checked into the migration PR. The checked-in artifact must be treated as the source of truth for that PR, and this seed table is only a guide for the highest-risk paths.

### Producers (writes to `chat_messages`)

| Location | Path | Notes |
|---|---|---|
| `src/commands/collaboration/chat/send/server/` | external command surface | the user-facing entry point — `Commands.execute('collaboration/chat/send', …)` |
| `src/system/user/server/PersonaUser.ts:1270` | persona reply path | persona's own utterance back into the room (note: `:1270` is approximate — re-check at migration time) |
| `src/system/user/server/PersonaUser.ts:1302` | persona reply path (second call site) | self-reflection or system-message variant |
| `src/widgets/chat/chat-widget/*` | UI input path | composes `chat/send` calls; verify it routes through the command, not direct DataInsert |
| `src/system/sentinel/pipelines/*` | orchestration pipelines | many pipelines call `collaboration/chat/send`; wrappers must keep working or be migrated |
| `src/system/governance/GovernanceNotifications.ts` | governance notifications | imports and executes chat send types |
| `src/system/voice/server/VoiceWebSocketHandler.ts` | voice/chat bridge | sends chat and subscribes to chat events |
| `src/commands/airc/bridge/server/AircBridgeServerCommand.ts` | AIRC bridge shim | currently delegates AIRC bridge calls back into Continuum chat commands |

### Consumers (reads from `chat_messages`)

| Location | Path | Notes |
|---|---|---|
| `src/widgets/shared/DataLoaders.ts:174` | reactive entity scroller | feeds the `<chat-widget>` message list |
| `src/commands/collaboration/chat/export/server/` | external command surface | `Commands.execute('collaboration/chat/export', …)` for `--output` markdown |
| `src/commands/collaboration/chat/poll/server/` | external command surface | external pollers (CI, AI peers) |
| `src/commands/collaboration/chat/analyze/server/` | external command surface | content analysis aggregations |
| `src/commands/ai/thoughtstream/server/ThoughtStreamServerCommand.ts:79` | internal AI feature | thought stream uses recent chat as context |
| `src/commands/ai/report/server/AIReportServerCommand.ts:531` | internal AI feature | AI performance metrics aggregate over chat history |
| `src/commands/data/read/server/DataReadServerCommand.ts:62` | data layer special-case | `chat_messages` has access-control logic — must not be lost |
| `src/system/user/server/PersonaUser.ts:1865` | event subscription | `getDataEventName(COLLECTIONS.CHAT_MESSAGES, 'created')` for persona inbox |
| `src/system/core/shared/EventConstants.ts:48,182` | event-name registry | `DATA_EVENTS.CHAT_MESSAGES.{created,updated,deleted}` referenced from many places |
| `src/system/user/server/modules/PersonaTaskExecutor.ts` | persona task history | reads `COLLECTIONS.CHAT_MESSAGES` in multiple paths |
| `src/system/user/server/modules/PersonaTrainingSignalExtractor.ts` | training signals | extracts examples from chat history |
| `src/commands/ai/should-respond-fast/server/` | response heuristics | queries `chat_messages` by string collection name |
| `src/commands/ai/context/{search,slice}/server/` | context retrieval | exposes chat messages as a context source/type |
| `src/commands/genome/dataset-prepare/server/` | training dataset preparation | queries chat history for model/persona datasets |
| `src/system/state/EntityCacheService.ts` | cache pressure limits | has a dedicated `chat_messages` cap that may disappear or move |
| `src/system/data/entities/ChatMessageEntity.ts` | entity definition/indexes | schema/index source for the ORM-backed collection |
| `src/system/data/config/EntityFieldConfig.ts` | field config | collection-specific entity config |
| `src/system/rag/sources/*` and `src/system/tools/server/*` | tool/RAG definitions | advertise chat commands and `chat_messages` examples to agents |

### Authoritative inventory rule

**Before opening any migration PR, regenerate this inventory** with the following commands and reconcile into a checked-in artifact such as `docs/grid/generated/chat-to-airc-inventory.md`:

```bash
rg -n "COLLECTIONS\.CHAT_MESSAGES|chat_messages" \
  src/commands src/widgets src/system \
  -g '!**/__tests__/**' -g '!**/*.test.*' -g '!**/*.spec.*'

rg -n "Commands\.execute\\(['\"]collaboration/chat/|command:\\s*['\"]collaboration/chat/|client\\.commands\\[['\"]collaboration/chat/" \
  src/widgets src/system src/commands

rg -n "DATA_EVENTS\.CHAT_MESSAGES|data:chat_messages:" src/
```

A migration PR's body must include the diff between the inventory at PR-open time and the inventory at PR-merge time. **Any new entry not present in the generated artifact blocks the merge.**

---

## Migration stages

Four discrete states. Each transition has its own proof gates (next section). No state collapses without ALL of its predecessor's proofs holding.

```
┌────────────────┐  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐
│ Stage 0        │→ │ Stage 1        │→ │ Stage 2        │→ │ Stage 3        │
│ ORM only       │  │ Dual-write     │  │ AIRC primary   │  │ ORM removed    │
│ (today)        │  │ ORM + AIRC     │  │ ORM mirror RO  │  │ AIRC sole src  │
└────────────────┘  └────────────────┘  └────────────────┘  └────────────────┘
```

| Stage | Writes to | Reads from | Removal-safe? |
|---|---|---|---|
| 0 (baseline) | ORM `chat_messages` | ORM `chat_messages` | n/a — baseline |
| 1 (in progress) | ORM **and** AIRC room | ORM `chat_messages` | revert dual-write |
| 2 | AIRC room (primary) → mirrored to ORM read-only | AIRC OR ORM mirror (transparent) | re-enable ORM writes |
| 3 | AIRC room | AIRC | irreversible (modulo git revert + DB restore) |

---

## Proof gates per transition

Each gate is a CHECKBOX someone (human or peer agent) must explicitly satisfy, with the artifact named. Compile-only checks are listed but not sufficient on their own.

### Stage 0 → 1: enable dual-write

**Compile**:
- [ ] `npm run build:ts` clean
- [ ] `cargo test -p continuum-core` (relevant slices) green

**Functional**:
- [ ] Send a message via `<chat-widget>`. Screenshot shows it appearing within 1s.
- [ ] Same message appears in the AIRC event stream for the corresponding room.
- [ ] Same message present as a row in `chat_messages` collection.

**Persona path**:
- [ ] PersonaUser receives the message via the existing event subscription (no behavioral change in this stage).
- [ ] Persona reply appears in chat-widget AND in airc logs.

**Idempotency / failure**:
- [ ] Stop the AIRC daemon mid-send. Message lands in ORM, AIRC dual-write fails loudly (logged), retry succeeds when daemon comes back. **No silent drop.**
- [ ] Stop the data layer (continuum-core) mid-send. Send fails with explicit error to the user. **No silent ORM-only success.**

**Smoke**:
- [ ] `bash scripts/ci/canary-smoke-airc-queue.sh` passes (validates AIRC primitives still work).
- [ ] New `bash scripts/ci/canary-smoke-chat-dual-write.sh` (added in this PR) passes — sends a message, asserts both stores received it within 1s.

**Stage-1 slice status (2026-05-24)**:
- [x] Chat send builds a generated `AircRealtimeEnvelope` with `chat_transcript` payload, ORM message id as `traceId`, durable delivery, blob/media references only, and no inline base64.
- [x] Chat send publishes through a single `AircChatPublisher` seam after ORM persistence and surfaces AIRC failure in `ChatSendResult.airc` instead of silently swallowing it.
- [x] Replace the original `airc msg` publisher with AIRC's structured publish surface (`airc publish --body-json -`) and parse only the JSON receipt returned by the Rust daemon/API path.
- [x] Add the smoke script that asserts ORM row + AIRC event presence from a running Continuum instance: `bash scripts/ci/canary-smoke-chat-dual-write.sh`.

### Stage 1 → 2: AIRC primary, ORM read-only mirror

**Compile**:
- [ ] `npm run build:ts` clean
- [ ] `cargo test` slices for the new mirror writer green

**Inventory reconciliation**:
- [ ] All read consumers from §Inventory have been audited. Each is either (a) updated to read from AIRC directly, or (b) confirmed to work against the ORM mirror (which lags by ≤ 100ms per the soak gate below).

**Functional**:
- [ ] Send via chat-widget. Message appears in widget within 1s (read served from mirror or AIRC, transparent to user).
- [ ] `Commands.execute('collaboration/chat/export', …)` returns the same message.
- [ ] `Commands.execute('collaboration/chat/poll', …)` returns the same message.
- [ ] `ai/report` aggregates over the same message correctly.

**Mirror-lag SLO**:
- [ ] Mirror lag p99 < 100ms over a 1-hour soak. Measured by sending message via AIRC, polling ORM mirror until row appears, recording delta.
- [ ] Mirror lag never exceeds 5s over the same hour. (5s is the user-perceptible UX bound — anything above that and `chat/poll` callers will return stale data visible to humans.)

**Failure mode**:
- [ ] **Kill AIRC daemon. Mirror is read-only — chat-widget should still serve messages already in the mirror.** Sending should fail explicitly (no silent ORM-only writes).
- [ ] **Kill mirror writer. AIRC keeps writing; mirror falls behind, but recovers from where it stopped on restart (no message loss, possible reorder OK).**

**Smoke**:
- [ ] `bash scripts/ci/canary-smoke-airc-queue.sh` passes.
- [ ] `bash scripts/ci/canary-smoke-chat-airc-primary.sh` (added in this PR) passes — sends via AIRC path, asserts mirror catches up, asserts read serves it transparently.

### Stage 2 → 3: remove ORM `chat_messages`

This is the only irreversible step in the chain (modulo git revert + DB snapshot restore). The proof bar is **categorically higher** than the prior gates.

**Inventory zero-diff**:
- [ ] Re-run inventory commands from §Inventory. Diff against the original. **MUST be empty** — every consumer either reads from AIRC directly, or reads from the (now being removed) mirror via a wrapper that has been updated. Any remaining `COLLECTIONS.CHAT_MESSAGES` reference outside test fixtures and migration-script archive blocks the merge.

**Soak**:
- [ ] 7 days of stage-2 operation with **zero** mirror-write failures, zero mirror-lag SLO violations, zero user-reported message-loss bugs.
- [ ] Carl install + 1 hour of chat usage produces zero `chat_messages` collection writes (verified by data-layer audit log).

**Removal PR shape**:
- [ ] Deletes `chat_messages` collection from `entity_schemas.json` (sha bump regenerated by ts-rs).
- [ ] Deletes `DataLoaders.CHAT_MESSAGES` block.
- [ ] Deletes `DataReadServerCommand.ts:62` chat-message access-control special-case.
- [ ] Deletes the persona-event-subscription path that listens for `DATA_EVENTS.CHAT_MESSAGES.created` (replaces with AIRC inbox subscription — already done as part of Stage 1).
- [ ] Deletes `src/commands/collaboration/chat/{send,export,poll,analyze}` server bodies if those have been migrated to AIRC primitives, OR retains them as thin shims that delegate to AIRC.
- [ ] Each deletion is in a SEPARATE commit on the removal branch so the revert is granular.

**Rollback procedure** (must be tested before merging the removal PR):
- [ ] On a copy of the canary database: apply the removal migration, then revert the removal PR, then run a `data/restore` from the pre-removal snapshot. Verify chat history fully recovers.
- [ ] Document the SHA and the snapshot path in the removal PR's body.

**Smoke**:
- [ ] All prior smokes (`canary-smoke-airc-queue.sh`, `canary-smoke-jtag.sh`) still pass.
- [ ] New `canary-smoke-chat-airc-only.sh` passes — asserts ZERO ORM writes during a full chat session.

---

## Caller migration inventory: per-call-site cutover plan

For every entry in §Inventory, this table specifies the cutover step and the proof. Before stage 2 → 3, every row must be `done`.

| Call site | Cutover step | Proof | Status |
|---|---|---|---|
| `chat/send` server | dual-write at stage 1; AIRC-primary at stage 2; thin shim at stage 3 | dual-write smoke + mirror-lag SLO | not-started |
| `chat/export` server | read from AIRC (or mirror) at stage 2; remove ORM dep at stage 3 | export command returns same content as before | not-started |
| `chat/poll` server | same as export | poll returns same | not-started |
| `chat/analyze` server | same as export | aggregate value matches pre-migration baseline | not-started |
| `DataLoaders.CHAT_MESSAGES` | replace with AIRC-aware loader at stage 2; delete at stage 3 | chat-widget renders correctly post-cutover | not-started |
| `PersonaUser.ts` chat read+write | switch to AIRC inbox subscription at stage 2 | persona reply still appears in widget | not-started |
| `ThoughtStream` thought-context query | read from mirror at stage 2; AIRC at stage 3 | thought-stream test green | not-started |
| `ai/report` aggregate query | same as ThoughtStream | report numbers match baseline | not-started |
| `DataReadServerCommand` chat access-control | re-implement equivalent on AIRC at stage 2 | unauthorized read still rejected | not-started |
| `EventConstants.CHAT_MESSAGES` | remove emit/subscribe at stage 3 (after listeners migrated) | grep returns no matches outside the registry file itself | not-started |

A future PR updating any row to `in-progress` or `done` MUST update this file in the same commit.

---

## Out-of-scope

- **AIRC wire-format design**: see [AIRC-CONTINUUM-BRIDGE.md](AIRC-CONTINUUM-BRIDGE.md) and the airc repo. This document assumes AIRC is the transport and reasons about what proof Continuum needs.
- **Persona memory / engram path**: see continuum#1129 / #1133 / #1134 (typed Engram + IsMemorable Recipe + admission gate). The chat → AIRC migration is orthogonal to memory admission; both can proceed in parallel.
- **CLI ergonomics for AIRC-side chat operations**: `airc msg` already exists; this document does not redesign the airc UX.
- **Rollout to multi-machine grid**: out-of-scope for v1. This document covers the single-machine cutover (which a single Continuum install is). Multi-machine adds the gossip-layer correctness proofs that belong in [GRID-ARCHITECTURE.md](GRID-ARCHITECTURE.md).

## AIRC rust substrate status

The Continuum migration is blocked on typed AIRC interfaces, not on SQL table
access. Continuum should consume AIRC through adapters and typed events:

- AIRC PR #637 added `crates/airc-core` transcript primitives.
- AIRC PR #638 added the first machine-readable `airc logs --json` page shape.
- The next AIRC #563 slices should move page/replay/store ownership deeper into
  Rust and the SQLite ORM-backed store.

Continuum must not bind to AIRC's SQLite tables directly. The migration target
is `Commands.execute(...)` and UI/persona code calling a Continuum adapter that
delegates to AIRC transcript APIs, with compatibility shims retained until the
proof gates pass.

---

## Decision points that must be resolved before stage 1 begins

These are open questions, not gates. Stage 0 → 1 is BLOCKED on each:

1. **Dual-write atomicity**: when ORM write succeeds and AIRC write fails (or vice versa), what's the recovery model? Options:
   - (a) Two-phase: queue local intent; commit when both stores ack.
   - (b) Append-only with reconciler: each store has its own log; periodic reconciliation surfaces drift.
   - (c) Best-effort with explicit error surface to user (no atomicity, but no silent drop).
   - **Recommendation**: (c) for stage 1 (simpler, surfaces real failures), upgrade to (b) before stage 2.

2. **Message ID convention**: AIRC events have their own ID space; ORM `chat_messages.id` is a UUID. At stage 1, where does the canonical ID live?
   - **Recommendation**: ORM ID stays canonical at stage 1; the AIRC event carries it as metadata. At stage 2, AIRC ID becomes canonical and ORM mirror inherits it.

3. **Backfill of pre-migration history**: when stage 1 begins, the ORM has years of messages and AIRC has none. Is the gap left as "AIRC starts at this date forward" OR is there a one-time backfill?
   - **Recommendation**: gap. Backfill is its own card if needed; it's not a stage gate.

4. **Tombstone semantics**: chat-message deletion is currently a soft-delete in the ORM. AIRC doesn't have a native delete primitive; how does deletion propagate?
   - **Recommendation**: stage 1+: deletion stays in ORM; AIRC events are immutable. At stage 3 the tombstone semantics live on the AIRC side as a separate "redact" event type (designed in airc repo, out of scope here).

These decisions go into a follow-up card before stage 1 starts.

---

## Status log

(Updated by the agent driving each stage transition.)

- 2026-05-13 — Document drafted (claude-tab-2). Card #1130 in-progress. No code change yet — this is the planning gate that must be agreed before stage 0 → 1 PRs are filed.
- 2026-05-16 - continuum#1253 regenerated the chat/AIRC inventory artifact and
  tied the proof gates to the AIRC Rust transcript substrate work.
- 2026-05-25 — **Stage 1 complete.** continuum#1432 added `AircChatPublisher` + dual-write via CLI bridge; #1433 swapped CLI bridge to `airc publish` structured JSON receipt path; #1435 added `scripts/ci/canary-smoke-chat-dual-write.sh` proving ORM row + AIRC event correlation by receipt id. All four "Stage-1 slice status" boxes verified merged on canary. Card 6b564a9a-ba4f-4bc4-8ba8-c0fe88dd0eaa drives the Stage 1 → 2 transition (this slice resolves the open decisions blocking it).

---

## Stage 1 → 2 design (2026-05-25)

Resolves the four open decisions and lays the Stage 2 mirror-writer architecture so the Stage 1 → 2 PR can open without re-litigating shape questions.

### Decision resolutions

  1. **Dual-write atomicity (Stage 2 upgrade).** Stage 1 ships option (c): best-effort with explicit error surface in `ChatSendResult.airc`. **Stage 2 ships option (b): append-only with reconciler.** Concretely: AIRC becomes the primary writer (`AircChatPublisher.publish()` → AIRC event); a new `AircToORMMirrorWriter` daemon subscribes to the room's AIRC event stream and writes the mirror row idempotently keyed by `event_id`. The reconciler runs on writer startup + every 60s: it scans the last N AIRC events that have no corresponding ORM mirror row and back-fills. No two-phase commit; AIRC stays the source of truth, mirror is a projection that may lag.

  2. **Message ID convention.** Stage 1 keeps ORM `chat_messages.id` canonical (the `AircChatEnvelope.traceId` carries it as metadata). **Stage 2 inverts: `AIRC event_id` becomes canonical;** the mirror writer composes the ORM row with `id = event_id` (UUID-shaped already, no schema change) and stores the original ORM id (if any) under `metadata.legacyOrmId` for Stage 1 history rows. New rows after Stage 2 cutover share one id space; the special-case mapping in `DataReadServerCommand.ts:62` operates on whichever id is canonical at the time of the read.

  3. **Backfill of pre-migration history.** **No backfill at Stage 2.** AIRC starts at the Stage 1 cutover date; pre-Stage-1 history is read from the ORM directly via the mirror reader path (mirror serves BOTH historical ORM-native rows and Stage-2 AIRC-derived rows transparently). Backfill remains its own card if ever needed (likely never — the gap is a known migration boundary, not a regression).

  4. **Tombstone semantics.** Stage 2 keeps deletion ORM-local (soft-delete on the mirror row, ORM `deletedAt` field unchanged). The `chat_messages` mirror retains its current soft-delete fields; the corresponding AIRC event is NOT redacted/edited (AIRC events stay immutable at Stage 2). The mirror writer treats post-delete UI as "read the mirror, filter `deletedAt`". Stage 3 (out of scope for this slice) introduces a `chat.redact` AIRC event type that consumers honor server-side.

### Stage 2 architecture

```
Producer (chat-widget, persona, sentinel, etc.)
    │
    ▼
ChatSendServerCommand
    │
    ▼
AircChatPublisher.publish(envelope)  ──►  airc publish (JSON receipt)
                                              │
                                              ▼
                                       AIRC event store
                                              │
                                              ▼  subscription stream
                                       AircToORMMirrorWriter (new daemon)
                                              │
                                              ▼  ORM.insert(chat_messages)
                                       ORM `chat_messages` (mirror, read-only to producers)
                                              ▲
                                              │  ORM.query/list (legacy readers)
                                       DataLoaders / chat/export / chat/poll / etc.
```

**Producer side changes:**

  - `ChatSendServerCommand` removes its direct `DataCreate('chat_messages', ...)` call. It still constructs `ChatMessageEntity` for validation + envelope assembly but does NOT write to ORM directly.
  - The command's success path now requires the AIRC receipt; `ChatSendResult.airc.success` becomes the only success signal. ORM mirror write happens asynchronously via the mirror writer subscription.
  - Persona reply paths (`PersonaUser.ts:1270`, `:1302`) similarly switch to the publisher seam; no direct ORM writes from persona paths after Stage 2.

**Mirror writer (new):**

  - New daemon `AircToORMMirrorWriter` in `src/daemons/airc-mirror-daemon/` (separate from `data-daemon` to keep responsibilities crisp).
  - Subscribes to the chat event stream via `LibAircSubstrate.subscribe("chat_transcript")` (gated on continuum#1434 C2 design landing first — Stage 2 cannot ship without the typed subscribe primitive).
  - Maintains a cursor (`(lamport, event_id)`) per room in a small projection table; restart resumes from cursor.
  - Write path: `ORM.insert('chat_messages', {id: event.event_id, ...mapped fields, metadata: {airc_lamport, traceId: event.envelope.traceId, ...}})`.
  - **Idempotency rule:** insert is `INSERT ... ON CONFLICT(id) DO NOTHING`. Replay never duplicates.
  - **Reconciler:** every 60s, query `ORM.list('chat_messages')` for rows where `metadata.airc_lamport > cursor - safety_window` AND no event seen → emit `WARN` log + re-fetch from AIRC + re-insert. Catches the rare case where the subscription stream missed an event.

**Reader side changes:**

  - `DataLoaders.CHAT_MESSAGES` and consumers (`chat/export`, `chat/poll`, `chat/analyze`, `ai/report`, `ThoughtStream`) **stay unchanged in Stage 2.** They read from the ORM mirror, which is now updated by the mirror writer instead of `ChatSendServerCommand`. This is the "transparent to user" property: readers see the same shape, lag is bounded by mirror-write SLO.
  - `PersonaUser.ts` event subscription (`data:chat_messages:created`) continues to fire — the mirror writer's ORM insert triggers it. Persona inbox semantics preserved.

**SLO measurement (from existing Stage 1 → 2 gates):**

  - Mirror lag p99 < 100ms, max < 5s over 1-hour soak: measured by sending message via AIRC, polling ORM mirror for the row, recording delta. The mirror writer should comfortably hit p99 < 100ms on local-host (sub-ms IPC + sub-ms SQLite insert).

### Stage 1 → 2 PR sequence

  1. **PR-A: mirror writer skeleton.** Adds `AircToORMMirrorWriter` with typed source/store ports, cursor advancement, idempotent inserts, and fixture tests. Subscribes via `LibAircSubstrate` once that port is wired to the live AIRC SDK. Includes unit tests + a smoke that runs the mirror writer against a fixture AIRC stream and asserts ORM rows appear.
  2. **PR-B: producer cutover.** Removes direct `DataCreate('chat_messages')` from `ChatSendServerCommand` and the two `PersonaUser` persona-reply paths. Updates `ChatSendResult.airc.success` to be the sole success signal. Updates the smoke script `canary-smoke-chat-airc-primary.sh` (new) to assert mirror catches up < 100ms.
  3. **PR-C: reader audit.** Spot-checks each consumer from the inventory still works against the mirror (no behavior change expected). Updates the inventory's "Status" column from `not-started` → `verified-against-mirror` for each.
  4. **PR-D: Stage 1 → 2 soak.** 1-hour soak run with mirror-lag metrics recorded. Updates Status log here when soak passes.

PR-A is the gating PR. The first implementation slice keeps the live AIRC reader behind an `AircChatEventSource` port so the writer and ORM projection can be proven before binding to a specific runtime subscription API. PR-B/C/D can land in parallel once PR-A is in.

### What this slice does NOT do

  - Does not delete any ORM-side code. Stage 1 → 2 keeps the ORM intact as the read mirror. Removal is Stage 3 (irreversible, much higher bar).
  - Does not change the AIRC wire format. Continues to use `AircChatEnvelope` / `chat_transcript` payload shape from continuum#1432.
  - Does not touch persona memory / engram admission. Orthogonal per the original out-of-scope section.
  - Does not change the `airc publish` CLI bridge. Stage 1's structured CLI continues to carry sends until the C2 `LibAircSubstrate` wiring slice replaces it with typed Rust IPC.
