# Alpha Gap Analysis — Stability Plan

<!-- markdownlint-disable MD013 MD060 -->

**Updated**: 2026-05-07
**Branch policy**: every change lands as `PR -> canary -> validation -> PR -> main`
**Status**: active planning document, shared by humans and agents
**Operating rule**: Rust owns runtime logic. TypeScript is UI, schema, generated types, and thin command/transport glue.

This document is the alpha source of truth. Work should not proceed as disconnected chat threads or private agent branches. Each implementation PR must name the issue it advances, land in `canary`, publish validation evidence, and only then be considered for promotion to `main`.

The previous 2026-05-01 alpha snapshot was useful but had become a historical log. This revision turns it into an execution plan for the current goal: **stable, GPU-first, Rust-centric Continuum with modular Docker and fast tests that do not depend on the Node/UI stack for core correctness.**

## Alpha Definition

Alpha is ready when a fresh user can install, boot, talk to personas, recover from common failures, and verify the system mostly through Rust-level tests.

The non-negotiable gates:

1. **GPU-first inference**: alpha-critical inference must use Metal/CUDA/Vulkan/DMR GPU paths. No silent CPU fallback.
2. **Rust core owns behavior**: persona cognition, scheduling, resource pressure, paging, inference orchestration, replay, and recovery live in Rust.
3. **Node/TS is thin**: browser UI, command adapters, schemas, generated types, and minimal transport glue only.
4. **Docker is modular**: one opaque "build/seed/start everything" container is not alpha-ready. Services need independent health, logs, and restart boundaries.
5. **Fast tests first**: core work must be covered by `cargo test` or Rust integration tests before Docker/browser tests.
6. **Canary is the sync point**: every fix is merged to `canary` first and tested there by available Mac/Windows/Linux agents.
7. **No silent success**: health checks, install steps, inference readiness, bridge delivery, and UI restore paths must fail loud with actionable evidence.

## Current Snapshot

| Area | Current read | Alpha risk |
|---|---|---|
| AIRC collaboration | Usable enough for agent coordination; PR #1046 bridge harness is open; airc has carried PR review/status traffic | Continuum personas are not yet first-class AIRC peers; internal AI chat still needs bridge validation |
| UI room state | PR #1047 merged to `canary` for stale duplicate General tab recovery | Needs live UI reload validation before `main` promotion |
| Docker | Too much historical bulk and mixed responsibility; several open Docker issues remain | Docker can mask failures and slow iteration |
| Rust core | Strong core exists, but GPU lifecycle, paging, and persona runtime boundaries are still incomplete | Core instability can make UI/Node fixes irrelevant |
| Node/TS | Still owns too much cognition/command behavior | Adds latency, GC/IPC complexity, and harder cross-platform reuse |
| Tests | Many tests exist, but the alpha loop still overuses `npm start`/browser/Docker as proof | Slow tests hide root causes and discourage TDD |

## Issue-Driven Workstreams

### 0. Canary Discipline And Collaboration

**Goal**: stop parallel agents from diverging. Every agent should know the issue, branch, PR, validation command, and current blocker.

| Issue / PR | Role | Required action |
|---|---|---|
| PR #1046 | AIRC bridge harness for Continuum testing | Keep reviewed; use it to reduce manual `jtag chat/send` and paste relay |
| PR #1035 | current canary -> main promotion PR | Do not promote blindly; use this doc's gates to decide when canary is worth main |
| PR #1047 | stale General tab recovery, merged to canary | Validate live UI state, then include in next canary -> main promotion |
| #967 | personas as AIRC peers | Treat as the collaboration unlock: Continuum personas should participate without manual CLI glue |

Rules:

- Implementation starts from an issue. If no issue exists, file it before coding.
- PR body must include: issue link, canary target, validation commands, platform coverage, and what was not tested.
- Agents coordinate on AIRC, but the durable truth is issue + PR comments.
- `main` promotion only happens after canary has been exercised by at least one real UI path and one non-UI/Rust path relevant to the changes.

### 1. First-Run And Install Stability

**Goal**: a new user does not hit a silent or half-working install.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| #1006 WSL2 cannot reach raw.githubusercontent.com | P0 | install must detect network/bootstrap failure early and print a concrete fix | Windows fresh install log shows failure in <30s with remedy |
| #1007 Windows rustc ICE compiling continuum-core | P0 | do not make first-run depend on a fragile local Rust build when a published binary/image can be used | Windows install reaches runnable app without compiling core locally |
| #1008 core socket owned by root container | P0 | fix UID/GID and socket volume ownership; host `jtag` must connect | host `jtag ping` succeeds against container core |
| #980 Carl validator QA bugs | P0 | break into child issues if still bundled | each child has a canary PR or is closed as stale |
| #983 Vulkan deferred model download | P0 | download/prewarm with progress during install or show explicit first-chat loading state | first Vulkan chat never sits silent during multi-GB download |
| #770 fresh install E2E | P0 | make this the release gate, not a one-off QA task | Mac + Windows reinstall logs attached to canary validation |

Implementation posture:

- Prefer published Rust artifacts or minimal service images over compiling everything during first-run.
- If build is unavoidable, make it explicit and resumable.
- Install health must distinguish: network unavailable, Docker unavailable, GPU unavailable, model unavailable, Rust core unavailable, UI unavailable.

### 2. GPU Runtime Stability

**Goal**: GPU resource failures degrade or recover; they do not brick the session.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| #1048 mmproj/mtmd init mutex | P0 | one mtmd-capable backend may enter Metal pipeline/mmproj init at a time | Rust concurrency test: parallel vision/audio backend init serializes and all callers receive a sane result |
| #1050 backend recovery state machine | P0 | represent backend as `Healthy`, `Initializing`, `Recovering`, `Dead`, `Unavailable`; recover/drop/recreate on OOM/dead backend | Rust test with injected backend failure recovers or reports `Unavailable`, never hangs |
| #960 Mac Metal throughput 5-7 tok/s | P0 | measure and fix actual GPU path; do not route through slow CPU-shaped fallback | benchmark shows expected Metal path and records tok/s |
| #964 ONNX Runtime CPU spike | P0 | enforce Metal/GPU provider selection for fastembed/TTS/STT/vision bridge or fail loud | test/log proves provider is Metal/GPU; CPU fallback is explicit |
| #948 DMR concurrency failure | P1 | add bounded request scheduling/backpressure around DMR | 4+ persona concurrency test passes without reqwest cascade |
| #915 Kokoro ONNX deadlock | P1 | isolate session creation and apply GPU provider lifecycle rules | regression test for TTS startup no deadlock |
| #918 multimodal-native worker | P2 | after lifecycle is safe, collapse voice chain latency | live voice turn benchmark |

Rust targets:

- `src/workers/continuum-core/src/inference/`
- `src/workers/llama/src/mtmd.rs`
- `src/workers/continuum-core/src/gpu/`
- `src/workers/continuum-core/src/live/audio/`

Do not fix these in TypeScript. TS may display state and call commands; it must not own backend lifecycle.

### 3. Rust Persona Runtime And Cognition

**Goal**: personas can run, replay, and be embedded without Node acting as the brain.

| Issue / doc | Priority | Direction | Test gate |
|---|---:|---|---|
| #969 migrate tool agent loop to Rust | P0 | move persona/tool loop behavior out of TS | net-negative TS cognition lines and Rust replay test |
| #909 local persona tool execution | P0 | wire local DMR/Candle tool execution through Rust path | local persona can call a tool without cloud path |
| #958 DMR repetition penalty / echo | P0 | fix generation config at adapter layer | replay/conversation test proves no verbatim echo loop |
| #837 raw tool-call XML leak | P1 | output rendering and model post-processing both need tests | fixture with tool markup renders/filters correctly |
| #970 missing image marker | P1 | ensure media markers are role/content correct in Rust prompt assembly | vision replay fixture includes media marker |
| docs/architecture/PERSONA-AS-RUST-LIBRARY-PLAN.md | P0 reference | keep as detailed architecture, but alpha doc owns sequencing | cargo tests run without Node |
| docs/architecture/PERSONA-COGNITION-RUST-MIGRATION.md | P0 reference | enforce "Rust = verbs, TS = nouns/shims" | PRs touching cognition show TS line reduction |

Near-term PR sequence:

1. **PR: Rust persona trace/recorder validation**
   - issue: file/link if not already present
   - scope: Rust fixture capture and replay for a chat turn
   - tests: `cargo test --package continuum-core persona`
2. **PR: Rust tool loop migration**
   - issue: #969
   - scope: shrink TS tool-agent loop to a shim
   - tests: Rust tool loop unit/integration test; net-negative TS cognition lines
3. **PR: local persona tool execution**
   - issue: #909
   - scope: local model path can execute tools without cloud-only assumptions
   - tests: local persona tool-call replay; no browser required

### 4. Unified Paging And Pressure Control

**Goal**: support many personas and modalities by paging resources coherently instead of over-allocating and hoping.

| Issue / doc | Priority | Direction | Test gate |
|---|---:|---|---|
| docs/architecture/UNIFIED-PAGING.md | P0 reference | `PagedResourcePool` is the primitive; migrate consumers one at a time | pool tests plus consumer-specific tests |
| docs/architecture/PERSONA-CONTEXT-PAGING.md | P0 reference | KV/persona context paging policy | tests prove bounded memory with multiple personas |
| #1049 PressureBroker admission gate | P0 | broker must deny unsafe allocations, not just observe them | admission test refuses second unsafe mtmd/backend creation |
| #1051 MtmdContext pooling | P0 | reuse multimodal context instead of fresh multi-GB allocation per image/frame | replay test avoids repeated context allocation |
| #945 data/query memory leak | P0 | apply resource attribution and leak tests | load test stays within memory envelope |
| #944 embedding loop/cache misses | P1 | migrate embedding cache to shared paging primitive | repeated index pass has cache hits and bounded memory |
| #911 16GB MacBook Air | P1 | define reduced alpha profile with strict budgets | 16GB profile starts and reports disabled features honestly |

Implementation order:

1. PressureBroker admission gate.
2. Backend/mmproj lifecycle integration.
3. First consumer migration: embedding cache or mtmd context pool.
4. KV/persona context policy.
5. LoRA adapter paging.

### 5. Docker Modularization

**Goal**: Docker should isolate services and make failures obvious; it must not become a bulk mess that hides Rust/Node/UI problems.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| #892 CUDA Docker path bypasses our substrate | P0 | GPU profile must run Continuum runtime or explicitly documented external service, not orphaned upstream server | GPU compose path exercises our adapter/router health |
| #955 floating CUDA image tag | P0 | pin digest or controlled version | CI verifies pinned image |
| #834 / #776 image size | P1 | split build/runtime layers; remove unused Node/vendor bulk from runtime images | image size trend published in PR |
| #796 Docker compose E2E live mode/grid | P1 | profile-based compose tests, not one giant default | compose profile tests pass independently |
| #908 Windows npm start should route through docker compose | P1 | Windows dev path should use the supported Docker/WSL path | Windows smoke reaches GPU-backed inference |
| #860 config.env as directory | P1 | keep setup file/dir creation idempotent and typed | setup test catches file-vs-dir mismatch |
| #859 compose pull hangs in Git Bash | P1 | Windows shell path needs bounded timeout and clear next step | install does not hang indefinitely |

Docker shape:

- `continuum-core`: Rust runtime, GPU adapters, IPC/HTTP surface, no UI.
- `node-server`: thin command/websocket bridge; no persona cognition logic.
- `widget-server`: static/browser UI only.
- `model-init`: explicit model prewarm/download with progress.
- Optional profiles: `ui`, `grid`, `gpu`, `live`, `forge`, `devtools`.

Health checks:

- Process exists is not health.
- Core health means IPC responds and required GPU/model capability is ready or explicitly unavailable.
- Node health means it can reach core or reports degraded with cause.
- Widget health means static UI and WebSocket proxy are reachable.
- Model health means expected model is present and GPU-serving path is known.

### 6. UI And Realtime Stability

**Goal**: the browser should reflect reality and recover without manual localStorage/database cleanup.

| Issue / PR | Priority | Direction | Test gate |
|---|---:|---|---|
| #961 / PR #1047 | P0 | stale General tab canonicalization merged to canary | browser reload with stale persisted state collapses to one General tab |
| #793 Node does not reconnect when Rust core restarts | P0 | request pipeline must drain/recreate after core restart | kill/restart core test: next command succeeds |
| #794 AI messages not realtime | P0 | event bridge forwards AI senders immediately | browser sees AI message without refresh |
| #962 chat history paging | P1 | ORM cursor + IntersectionObserver | scroll-up test loads older messages |
| #773 browser WS reconnect | P1 | reconnect/rebind without manual refresh | browser survives server restart |
| #785 URL scheme | P1 | one consistent route rule, zero special cases | stale room URL redirects/recovers deterministically |
| #783 stale room URLs | P1 | stale URLs show recovery path, not broken tab | route test |

TS is acceptable here because this is UI/session state. Still, data validation and canonicalization should use existing routing/entity APIs, not hardcoded UUID/string hacks.

### 7. AIRC And Continuum Internal AI Collaboration

**Goal**: Continuum personas and external coding agents can collaborate through the same room/bus without humans relaying messages.

| Issue / PR | Priority | Direction | Test gate |
|---|---:|---|---|
| #967 | P0 | expose personas as AIRC peers | persona receives AIRC room message and replies through Continuum chat |
| PR #1046 | P0 | AIRC bridge harness | bridge protocol test and live room smoke |
| #856 grid event streaming | P1 | persistent event channels between nodes | cross-node event smoke, no polling-only path |
| #798 route inference through mesh | P2 | use grid routing for GPU-heavy inference | command from non-GPU node routes to GPU node |

Design rule:

- AIRC is collaboration transport.
- Continuum chat is product state.
- The bridge should map messages/events without requiring agents to shell out to `jtag chat/send` manually.
- Protocol tests must run without a browser.

## PR Roadmap To Alpha

| Order | Branch | Base | Issue(s) | Deliverable | Required validation before canary merge |
|---:|---|---|---|---|---|
| 1 | `codex/alpha-gap-stability-plan` | `canary` | planning doc | this document; shared execution map | docs lint/readability, AIRC review |
| 2 | `fix/gpu-backend-lifecycle` | `canary` | #1048, #1050, #960, #964 | mutex + backend state/recovery | Rust tests with injected failure; GPU provider evidence |
| 3 | `fix/docker-alpha-profiles` | `canary` | #892, #955, #834, #776, #796 | modular Docker profile cleanup | compose profile smoke; image size report |
| 4 | `feature/persona-rust-replay` | `canary` | #969, #909 | Rust persona replay/tool-loop foundation | `cargo test`; net-negative TS cognition lines |
| 5 | `feature/pressure-broker-gate` | `canary` | #1049, #1051, #945, #944 | admission gate + first resource consumer | memory/load tests; no Node required |
| 6 | `fix/realtime-core-reconnect` | `canary` | #793, #794, #773 | core restart + realtime browser recovery | kill core, command recovers, browser receives AI message |
| 7 | `feature/airc-persona-peer` | `canary` | #967, PR #1046 | Continuum persona as AIRC participant | AIRC -> Continuum -> AIRC round trip |
| 8 | `test/fresh-install-e2e` | `canary` | #770, #1006-#1008, #983 | install validation matrix | Mac + Windows logs; no silent waits |

This order can change when a blocker is discovered, but changes must be made in this document and on the issue/PR thread, not only in chat.

## Test Strategy

### Rust-first tests

Use these before Docker/browser validation:

```bash
cargo test --manifest-path src/workers/continuum-core/Cargo.toml
cargo test --manifest-path src/workers/llama/Cargo.toml
```

Add focused tests for:

- backend lifecycle and recovery
- mmproj init serialization
- persona replay fixtures
- paging pool consumers
- pressure admission decisions
- local tool execution

### Docker tests

Docker tests are service/profile tests, not proof that core logic is correct:

```bash
docker compose up -d postgres continuum-core node-server
docker compose --profile ui up -d widget-server
docker compose --profile gpu up -d
docker compose --profile live up -d
```

Each profile needs a bounded smoke command and a log artifact.

### Browser tests

Use browser tests only for browser responsibilities:

- tab restore and route canonicalization
- WebSocket reconnect
- realtime message rendering
- UI state after data reseed

The stale General bug belongs here; backend lifecycle does not.

### AIRC collaboration tests

Use AIRC for live coordination, but also create protocol tests:

- external agent sends AIRC message into room
- Continuum bridge records it as chat event
- persona responds
- response mirrors back to AIRC
- duplicate/replay protection is verified

## Merge Gates

Every alpha PR must answer:

- Which issue does this advance?
- Why does this belong in Rust, TS, Docker, or docs?
- What command proves the core behavior without browser/Node?
- What canary validation was run?
- What platforms were covered?
- What remains untested?
- Did it reduce Node/TS logic or at least avoid adding new TS logic?
- Did it avoid silent fallback/silent success?

Main promotion requires:

- canary contains the PR
- canary has been tested by at least one other agent/human where practical
- failures are linked to issues, not buried in chat
- the promotion PR lists included canary commits and validation evidence

## Document Map

This document owns execution order and alpha gates. Detailed architecture remains in:

- [Persona-as-Rust-Library](../architecture/PERSONA-AS-RUST-LIBRARY-PLAN.md)
- [Persona Cognition Rust Migration](../architecture/PERSONA-COGNITION-RUST-MIGRATION.md)
- [Unified Paging](../architecture/UNIFIED-PAGING.md)
- [Persona Context Paging](../architecture/PERSONA-CONTEXT-PAGING.md)
- `src/shared/models.json` and `src/shared/ModelRegistry.ts`
- [Docker Node Architecture](../grid/DOCKER-NODE-ARCHITECTURE.md)
- [Grid Architecture](../grid/GRID-ARCHITECTURE.md)
- [AIRC Continuum Bridge](../grid/AIRC-CONTINUUM-BRIDGE.md)

If those docs disagree with this one on sequence, update this one first or explicitly revise the sequence in the PR.

## Immediate Next Actions

1. Land this doc to `canary`.
2. Use the newly filed alpha substrate issues as implementation anchors:
   - #1048 mmproj/mtmd init mutex
   - #1050 backend recovery state machine
   - #1049 PressureBroker admission gate
   - #1051 MtmdContext pooling
3. Ask Mac/Windows agents to review the issue mapping and mark any issue stale/misclassified.
4. Start `fix/gpu-backend-lifecycle` from `canary`.
5. In parallel, have another agent inspect Docker profile boundaries and propose `fix/docker-alpha-profiles`.
6. Validate #1047 live in UI before any canary -> main promotion.
