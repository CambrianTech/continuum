# Alpha Gap Analysis — Prioritized Feature Backlog

**Date**: 2026-03-12
**Status**: UI/UX alpha complete. System runs stable with 14+ AI personas in live video calls.
**Branch**: `main` (merged `feature/rust-live-memory-leaks`)

This document is the **single source of truth** for remaining work before open-source launch.
Each item is a self-contained feature branch. Priority order is the implementation order.

---

## Current State (What Works)

| Subsystem | Status | Notes |
|-----------|--------|-------|
| Live video calls | Working | Human + 14 AI avatars, 3D scenes, real-time voice |
| Persona telemetry | Working | INT/NRG/ATN meters, cognitive diamonds, genome bars |
| Memory pressure | Working | Graduated levels (normal/warning/high/critical), RSS bounded |
| Persona cadence | Working | Pressure-aware adaptive timing |
| Chat coordination | Working | ThoughtStream turn-taking, probabilistic responders |
| LoRA training | Proven E2E | Train/discover/load/merge/inference pipeline |
| Academy | Proven E2E | Dual-sentinel teacher/student, RealClassEval 53% pass |
| Sentinel pipeline | Working | 10 step types, 103+ tests, CodingAgent integration |
| ORM | Working | SQLite + Postgres, schema evolution, self-healing |
| RAG (chat history) | Working | Tiered cache L1/L2, 30-50ms cached |
| RAG (codebase) | Proven E2E | CodebaseIndexer + CodebaseSearchSource |
| Vision pipeline | Proven E2E | Tiered perception, content-addressed cache |

---

## Priority 1: Architectural Integrity (Code Quality)

**Why first**: Open-source contributors will copy these patterns. Every `any` cast, every god class, every magic number becomes a template for how the community writes code. Fix the foundation before anyone sees it.

### 1A. Type Safety — Eliminate `any` Casts

**Current**: 831 `any` casts in production code, 1,269 in tests (2,100 total)
**Target**: Zero in production code, minimal in tests

**Top offenders (production)**:
| File | Count | Fix Strategy |
|------|-------|-------------|
| PersonaBrainWidget.ts | 21 | Import proper event/data types |
| InferenceGrpcClient.ts | 16 | Type gRPC response objects |
| FileStorageAdapter.ts | 14 | Use generic `BaseEntity` constraints |
| WidgetEventService.ts | 13 | Type event payloads from AI_DECISION_EVENTS |
| GlobalAugmentations.ts | 12 | Proper module augmentation types |
| MemoryStorageAdapter.ts | 12 | Same as FileStorageAdapter |
| JTAGClient.ts | 11 | Type command response generics |
| DataDaemon.ts | 11 | Type collection operations |
| PersonaUser.ts | 9 | Type tool/genome/RAG interfaces |

**Approach**: File-by-file, starting with public APIs (JTAGClient, DataDaemon, Commands), then adapters, then widgets.

### 1B. Command Infrastructure — Single Source of Truth

**The command generator creates correct patterns automatically.** Any command that bypasses the generator is a pattern violation waiting to be copied by contributors.

**Current state**:
- 273 commands have proper static accessors — good
- **39 commands missing static accessors** (agent/*, sentinel/*, state/*, search/*, voice snapshots)
- **266 commands hand-written** without generator specs (only 47 have specs)
- **23 `any` casts** in command Types files
- **5 raw `Commands.execute()` calls** with string literals + `as any` (SearchWorkerClient, DiagnosticsWidget, LogViewerWidget)
- **52 uses of `object`/`Record<string, unknown>`/`unknown`** in command types

**Priority targets** (most-used commands missing proper types):
| Command Group | Issues | Impact |
|---------------|--------|--------|
| sentinel/* (8 commands) | No static accessors, `as any` casts throughout | High — sentinel is core infrastructure |
| agent/* (4 commands) | No static accessors | High — agentic loop depends on these |
| state/* (3 commands) | No static accessors | Medium — state management API |
| search/* (4 commands) | No static accessors, untyped results | Medium — RAG depends on these |
| data/* (create/read/update/query-next) | No static accessors | High — most-used commands in system |

**Approach**:
1. Create generator specs for the 39 missing-accessor commands
2. Regenerate with `npx tsx generator/CommandGenerator.ts generator/specs/<name>.json`
3. Replace all raw `Commands.execute('name', ...)` calls with typed `CommandName.execute()`
4. Eliminate `any` from command Types files
5. Replace `object`/`Record<string, unknown>` with proper interfaces

**Enforcement** (prevent regression):
- **Precommit hook or CI check**: Any new directory under `commands/` MUST have a matching spec in `generator/specs/`. Script scans `commands/*/` dirs, checks for corresponding `.json` spec. Fails if missing.
- **Generator improvement**: Review and modernize the generator during this phase — ensure it produces the latest patterns (path aliases, proper generics, static accessor with `commandName` const).
- **Lint rule**: Flag raw `Commands.execute('string-literal', ...)` calls — must use typed accessor.

**Validation**: `grep -r "Commands.execute(" --include="*.ts" | grep -v "CommandName\."` should return zero results when done.

### 1C. God Class Decomposition — PARTIALLY COMPLETE

**Completed extractions**:
- `DataSchemaManager.ts` extracted from DataDaemon (schema extraction, validation, provisioning)
- `DataVectorOperations.ts` extracted from DataDaemon (6 vector methods + VectorCapableAdapter interface)
- `JTAGClientConnections.ts` extracted from JTAGClient (LocalConnection, RemoteConnection, RemoteConnectionHost interface)
- `PersonaAgentLoop.ts` extracted from PersonaResponseGenerator (agent tool loop, 294 lines)

**Remaining**:
| File | Lines | Should Be |
|------|-------|-----------|
| PersonaUser.ts | ~2,200 | <500 — extract to focused modules |
| RustWorkerStorageAdapter.ts | 1,234 | <500 — extract per-operation modules |
| OllamaAdapter.ts | 1,225 | <500 — extract model management, streaming, tool parsing |
| ChatRAGBuilder.ts | 1,214 | <500 — extract source management, assembly, caching |
| PersonaMessageEvaluator.ts | 909 | <500 — extract training signal, topic detection |

**Approach**: Extract using the existing module pattern (PersonaUser already has `modules/`). Each extraction is a PR with tests proving behavior preservation.

### 1D. Magic Number Consolidation — COMPLETE

All timing constants consolidated into `PersonaTimingConfig.ts`:
- `PersonaState.ts` — snapshot throttle, initial delay, periodic interval
- `SelfTaskGenerator.ts` — sweep cooldown, domain cooldown
- `PersonaAutonomousLoop.ts` — gap assessment interval, circuit breaker
- `PersonaSubprocess.ts` — priority-based wait times (6-case switch → map lookup)
- `PersonaTile.ts` — diamond persist duration
- `PersonaResponseGenerator.ts` — daemon init wait/poll, generation timeout, voice max tokens
- `PersonaUser.ts` — generation timeout

### 1E. Rust Panic Safety — MOSTLY COMPLETE

**Fixed**:
- All regex sites already use `LazyLock` or `once_cell::Lazy` (agent.rs, garbage_detection.rs, parsers.rs, interpolation.rs)
- `.ok()` on file writes → warn log in sentinel/executor.rs (PID write, log writes, mkdir, cleanup — 20 sites)
- Log write errors use first-failure-only logging to avoid flooding
- `step_err()` helper added to `sentinel/types.rs` — used by llm.rs, coding_agent.rs, command.rs, emit.rs, watch.rs, shell.rs

**Remaining (intentionally kept)**:
- 36 `.lock().unwrap()` sites in render_loop, selection, message_bus, livekit_agent, bevy_renderer, archive
  - These are correct: a poisoned mutex = corrupted data, crashing is the right behavior
- 3 `.expect()` in init code (home_dir, AIProviderModule) — genuine preconditions

### 1F. Missing ts-rs Exports — COMPLETE

Added `#[derive(TS)]` with proper exports to 10 types across 4 modules:
- `RagComposeRequest` (rag.rs) → `shared/generated/rag/`
- `AgentStatus`, `ToolCall`, `ToolResult`, `AgentAction` (agent.rs) → `shared/generated/agent/`
- `DatasetManifest`, `DatasetMetrics` (dataset.rs) → `shared/generated/dataset/`
- `MCPTool`, `MCPInputSchema`, `MCPProperty` (mcp.rs) → `shared/generated/mcp/`

Barrel exports created for agent/, dataset/, mcp/ and added to main index.ts.

---

## Priority 2: Pressure System Completion (Phases 1, 3) — COMPLETE

**Status**: Completed in PR #304 (pressure-aware inference + content lifecycle).

### 2A. ThoughtStream Pressure-Aware Slots — DONE

`getProbabilisticMaxResponders()` in `ThoughtStreamCoordinator.ts:440` reads `BackpressureService.pressureLevel` with graduated response counts (Normal: 1/2/3, Warning: 1/2, High/Critical: 1).

### 2B. Voice Broadcast Gating — DONE

`onUtterance()` in `VoiceOrchestrator.ts:239` gates broadcast targets by pressure level (Normal: all, Warning: max 3, High: max 1, Critical: none).

---

## Priority 3: Coordination Decision Logging (Phase 5C) — COMPLETE

**Status**: All 5 wiring steps done.

### What's Done
- CoordinationDecisionEntity (630 lines) — complete
- CoordinationDecisionLogger (200+ lines) — complete
- EntityRegistry integration — complete
- PersonaUser import — complete
- `evaluateShouldRespond()` returns `GatingResult` discriminated union with `filteredRagContext` — complete
- `buildCoordinationRAGContext()` helper converts pipeline → decision format — complete
- RESPOND decisions: logged in `PersonaResponseGenerator.ts:830` with full response content after generation
- SILENT decisions (LLM gating): logged in `PersonaMessageEvaluator.ts:462` with minimal context + coordination snapshot
- Post-inference SILENT: logged in `PersonaMessageEvaluator.ts:626` with full RAG context (was already built)
- Early gate SILENT (rate limit, response cap): file-based only (mechanical, high-volume, low coherence value)

---

## Priority 4: Scene Lifecycle (Bug Fix) — MOSTLY COMPLETE

**Status**: Core fix in PR #305 (resource lifecycle cleanup).

### What's Fixed
- Video loop shutdown: `LiveKitAgent.disconnect()` sends watch channel signal, video loops exit immediately
- Slot pool recovery: `reset_slot_pool()` reclaims zombie-held Bevy slots after idle timeout
- Identity mapping cleanup: `unload_avatar_models()` clears identity→slot map on idle timeout
- Model unloading: STT/TTS adapters unload after 60s idle (ReloadableModel + shutdown() trait)

### Remaining Investigation
- Verify `AvatarCommand::UnloadIdle` actually tears down Bevy scenes (ECS entities, loaded meshes)
- Bevy render loop must tick for commands to process — confirmed it does (idle_cadence gating disabled for this reason)

---

## Priority 5: RAG Codebase Indexing (Full Integration)

**Why fifth**: Foundation exists (CodebaseIndexer proven E2E), but not integrated into persona cognition. This is what makes the AI team actually useful — answering questions about the codebase.

### What Exists
- `CodebaseIndexer` — chunks TS/MD/JS, generates 384-dim embeddings via Rust IPC
- `CodebaseSearchSource` (RAGSource) — queries code_index, injects into system prompt
- Relevance threshold 0.35 filters naturally
- Embedding mixin proven (Rust returns binary f32 data)

### What's Missing
- **Automatic re-indexing** on file change (watch mode or git hook)
- **Wiring into ChatRAGBuilder** — CodebaseSearchSource needs to be a default source for all personas
- **Query optimization** — large codebases need incremental indexing, not full re-index
- **Index persistence** — survives server restart without re-indexing

### Success Metric
Ask Helper AI "Why does PersonaUser have both inbox and coordinator?" and get a correct, cited answer within 5 seconds.

---

## Priority 6: Technical Debt Deep Clean

### 6A. ESLint Configuration
- Missing `@typescript-eslint/eslint-plugin`
- Can't catch type errors, unused imports, style violations
- Technical debt accumulates unchecked

### 6B. Disabled Systems (Dead Code or Re-enable)
- Circuit breaker: `maxConsecutiveFailures: 999999` in BaseAIProviderAdapter (disabled)
- PersonaState energy rates: effectively 0 (thermodynamic system dead)
- Decision: re-enable with proper values, or remove entirely

### 6C. Error Handling Audit
- Silent `.ok()` swallowing in sentinel executor (7+ sites) — **FIXED in P1E**
- Missing input validation: sentinel/run doesn't validate pipeline after JSON.parse()
- `e.to_string()` vs `format_pg_error()` pattern may exist in other Postgres sites

### 6D. Rust Test Failures (14 pre-existing)
- 6 `modules::data::tests::*` — data module integration tests (SQLite-related)
- 3 `orm::connection_manager::tests::*` — connection pooling / LRU eviction
- 3 `orm::migration::tests::*` — migration pause/resume, verify, cross-db
- 1 `orm::sqlite::tests::test_create_and_read` — basic CRUD assertion failure
- 1 `rag::engine::tests::test_parallel_source_loading` — timing-dependent race condition
- Total: 1261 pass, 14 fail, 25 ignored (as of 2026-03-14)

---

## Priority 7: Adapter Management

**User request**: Docker-like management for LoRA adapters.
- 58 old adapters accumulated to 21GB before manual cleanup
- Need `genome/adapter-list`, `genome/adapter-prune`, `genome/adapter-info` commands
- Integrate with EvictionRegistry for usage tracking

---

## Priority 8: PR Review Pipeline

**From PRACTICAL-ROADMAP.md Milestone 2**: AI teammates review PRs automatically.

### What Exists
- WebhookProcessor infrastructure
- CodeReview AI persona
- CodingAgent step in sentinel

### What's Missing
- GitHub webhook receiver for PR events
- PR diff analyzer (extract changed files, hunks)
- Architecture rule checker (environment mixing, type safety)
- Review comment posting via GitHub API

---

## Priority 9: Continuous Learning Loop

**From PRACTICAL-ROADMAP.md Milestones 3-5**: AIs learn from corrections, become proactive, develop scope-based expertise.

### 9A. Correction Detection
- TrainingDaemon observes chat, detects when human corrects AI
- Creates high-priority training examples from corrections
- Weekly LoRA fine-tuning on accumulated data

### 9B. Self-Task Generation
- AIs create their own work (memory consolidation, skill audits)
- Task database and CLI commands (`./jtag task/create`, `task/list`)
- Autonomous scheduling via PersonaAutonomousLoop

### 9C. Scope-Based Expertise
- Context-aware RAG (different sources per domain)
- Module-level LoRA adapters (specialized knowledge)
- Recipe system for training pipelines

---

## Implementation Order

```
P1: Architectural Integrity     ← Foundation for everything else
  ├── 1A: Type safety (any elimination)
  ├── 1B: Command infrastructure (single source of truth + generator enforcement)
  ├── 1C: God class decomposition        ← PARTIALLY COMPLETE (4 extractions done)
  ├── 1D: Magic number consolidation     ← COMPLETE
  ├── 1E: Rust panic safety              ← MOSTLY COMPLETE
  └── 1F: ts-rs exports                 ← COMPLETE

P2: Pressure System              ← COMPLETE (PR #304)
  ├── 2A: ThoughtStream slots    ✓
  └── 2B: Voice broadcast gating ✓

P3: Decision Logging (5C)        ← COMPLETE (PR #305 + current)

P4: Scene Lifecycle Bug           ← MOSTLY COMPLETE (PR #305)

P5: RAG Codebase Indexing         ← Makes AI team useful

P6: Technical Debt Deep Clean     ← ESLint, dead code, error handling

P7: Adapter Management            ← Docker-like adapter ops

P8: PR Review Pipeline            ← Automated code review

P9: Continuous Learning           ← Long-term AI improvement
```

Each priority is a feature branch, a PR, a merge. No mixing concerns across branches.

---

## Superseded Documents

The following planning docs are partially or fully superseded by this analysis:
- `ARCHITECTURE-GAPS-PHASE1.md` — Gap 1 (RAG indexing) now proven E2E, covered in P5
- `TECHNICAL-DEBT-AUDIT.md` — Updated numbers in P1 (was 1,108 `any`, now 831 production + 1,269 test)
- `ELEGANCE-AUDIT-2026-02-15.md` — Rust issues covered in P1D, TS issues in P1A-C
- `PHASE-5C-STATUS.md` — Still accurate, referenced from P3
- `PRACTICAL-ROADMAP.md` — Milestones 2-5 covered in P5, P8, P9
- `PHASE-5C-INTEGRATION-PLAN.md` — Detailed steps for P3
