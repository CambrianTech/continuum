# Chat-to-AIRC Migration Inventory

Generated for continuum#1253 on 2026-05-16.

This is the current Continuum-side inventory for moving chat from the
ORM-backed `chat_messages` collection to AIRC transcript APIs. It is a proof
artifact, not a design sketch: migration PRs must regenerate it and reconcile
the diff before changing storage behavior.

## Regeneration Commands

```bash
rg -n "COLLECTIONS\.CHAT_MESSAGES|chat_messages" \
  src/commands src/widgets src/system \
  -g '!**/__tests__/**' -g '!**/*.test.*' -g '!**/*.spec.*'

rg -n "Commands\.execute\\(['\"]collaboration/chat/|command:\s*['\"]collaboration/chat/|client\.commands\[['\"]collaboration/chat/" \
  src/widgets src/system src/commands

rg -n "DATA_EVENTS\.CHAT_MESSAGES|data:chat_messages:" src/
```

## Storage Entity And ORM Hot Path

| Area | Current path | Migration concern |
|---|---|---|
| Entity schema | `src/system/data/entities/ChatMessageEntity.ts` | `chat_messages` still defines room/timestamp indexes, archive policy, JSON media metadata, receipts, reactions, threading, and metadata semantics. AIRC must preserve equivalent transcript/projection fields before Stage 3 removal. |
| Write command | `src/commands/collaboration/chat/send/server/ChatSendServerCommand.ts` | Builds `ChatMessageEntity`, externalizes media, calls `DataCreate` on `ChatMessageEntity.collection`, then invokes `AircChatDualWriteService` for the Stage 1 AIRC handoff. |
| AIRC chat envelope | `src/system/airc-chat/shared/AircChatEnvelope.ts` | Maps stored ORM chat messages into generated `AircRealtimeEnvelope` / `chat_transcript` payloads. Carries ORM id as `traceId`; media is refs only. |
| AIRC chat publisher seam | `src/system/airc-chat/server/AircChatPublisher.ts` | Isolates the current CLI handoff behind `AircChatPublisher` so the Rust SDK/daemon publish path can replace it without touching chat command code. |
| Export command | `src/commands/collaboration/chat/export/server/ChatExportServerCommand.ts` | Reads via `DataList` using `ChatMessageEntity.collection`, applies filtering, then emits markdown. Stage 2 must prove export parity from AIRC or mirror. |
| Poll command | `src/commands/collaboration/chat/poll/server/ChatPollServerCommand.ts` | Reads `chat_messages` through `ORM.query`, including `afterMessageId` timestamp lookup. This is a direct ORM dependency and a latency-sensitive agent path. |
| Analyze command | `src/commands/collaboration/chat/analyze/server/ChatAnalyzeServerCommand.ts` | Aggregates over `ChatMessageEntity`. Keep as projection consumer until AIRC-backed aggregation is proven. |
| Data read access control | `src/commands/data/read/server/DataReadServerCommand.ts` | Has a `COLLECTIONS.CHAT_MESSAGES` special case. Equivalent AIRC access policy is a Stage 2 gate. |
| Field config/cache | `src/system/data/config/EntityFieldConfig.ts`, `src/system/state/EntityCacheService.ts` | Chat has collection-specific field and cache pressure behavior. Removing ORM chat must replace or delete these intentionally. |

## Producers

| Area | Current path | Migration concern |
|---|---|---|
| Chat command callers | `src/widgets/chat/*`, `src/system/sentinel/SentinelChatBridge.ts`, `src/system/sentinel/pipelines/*` | Many paths call `collaboration/chat/send`; keep command compatibility as a thin shim while swapping the backing store. |
| Persona replies | `src/system/user/server/PersonaUser.ts` | Persona writes to `COLLECTIONS.CHAT_MESSAGES` around reply/system-message paths. These writes must move to AIRC transcript append or a single adapter. |
| Tool results | `src/system/user/server/modules/PersonaTaskExecutor.ts` | Stores tool result messages in `COLLECTIONS.CHAT_MESSAGES`; must become an explicit transcript/projection event, not implicit ORM rows. |
| Voice bridge | `src/system/voice/server/VoiceWebSocketHandler.ts` | Bridges voice and chat events. AIRC should carry presence/control/events, while WebRTC/LiveKit keeps media. |
| Sentinel pipelines | `src/system/sentinel/pipelines/*` | Large fanout of `command: 'collaboration/chat/send'`; do not migrate piecemeal without preserving the command contract. |

## Consumers

| Area | Current path | Migration concern |
|---|---|---|
| UI loaders | `src/widgets/shared/DataLoaders.ts`, chat widget paths | The browser must render live updates from AIRC or a projection with no stale poll dependency. |
| Persona inbox | `src/system/user/shared/BaseUser.ts`, `src/system/user/server/PersonaUser.ts`, `src/system/user/server/modules/PersonaMessageGate.ts` | Subscribes to `data:chat_messages:created`. Stage 2 requires AIRC subscription/replay to preserve persona response behavior. |
| Training and memory | `src/daemons/training-daemon/server/TrainingDaemonServer.ts`, `src/system/user/server/modules/PersonaTrainingSignalExtractor.ts`, `src/system/genome/fine-tuning/server/TrainingDatasetBuilder.ts` | Training examples and memory candidates consume chat history. Cursor replay and deterministic ordering are mandatory gates. |
| AI context/reporting | `src/commands/ai/thoughtstream/server/ThoughtStreamServerCommand.ts`, `src/commands/ai/report/server/AIReportServerCommand.ts`, `src/commands/ai/context/*`, `src/commands/ai/should-respond-fast/server/*` | These consumers need either AIRC page APIs or bounded SQLite projections. Do not leave them on direct `chat_messages` strings. |
| Voice/live session | `src/system/voice/server/VoiceWebSocketHandler.ts` | Presence and chat events should route through AIRC events; media remains side-channel WebRTC/LiveKit. |
| Event constants | `src/system/core/shared/EventConstants.ts`, `src/system/events/shared/EventSystemConstants.ts` | `DATA_EVENTS.CHAT_MESSAGES` is a compatibility boundary. Stage 3 removal requires no runtime subscriber still depends on it. |

## AIRC Interface Gates

Continuum should not depend on AIRC internals or SQL tables. The expected
contract is a typed adapter over AIRC's Rust transcript/event store:

| Capability | Required behavior |
|---|---|
| Append | Send chat/event/presence entries with idempotent IDs, author metadata, room/activity pointer, and attachment manifest refs. |
| Page | Return recent and cursor-based pages with deterministic ordering, stable IDs, and self-message filtering. AIRC PR #638 provides the first `airc logs --json` CLI page shape. |
| Replay | Resume from a cursor without tailing raw logs or scanning unbounded history. |
| Receipts | Carry delivered/read/processed receipts without coupling to `ChatMessageEntity` fields. |
| Attachments | Preserve media blob hashes, URLs, MIME metadata, and descriptions without reintroducing inline base64 into database columns or events. |
| Presence/control | Carry `is typing`, `is thinking`, speaking, in-call, subscription, and WebRTC/LiveKit coordination events. |
| Health/capacity | Expose queue depth, storage pressure, replay lag, subprocess count, and disk write metrics for performance gates. |

## Stage-1 Blockers

- The AIRC transcript API must be typed and Rust-owned. Python/shell output can remain compatibility glue only.
- Continuum adapters must use command/entity abstractions; no raw SQL migration path is acceptable.
- The dual-write failure model must be explicit: no silent ORM-only or AIRC-only success.
- Media manifests must be proven with real image/audio metadata and no inline base64 persistence.
- Fresh install must work with no local Postgres and no `DATABASE_URL`.

## Performance Evidence Required

Every migration PR must report before/after measurements for:

- chat send latency
- page/export latency
- persona reply roundtrip latency
- event/replay lag
- CPU during idle and active chat
- memory and subprocess count
- disk writes and SQLite/AIRC store growth

The target is lower setup friction and lower runtime load, not a lateral move
from one storage path to another.
