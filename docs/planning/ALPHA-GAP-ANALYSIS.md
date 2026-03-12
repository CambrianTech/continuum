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

### 1B. God Class Decomposition

**Current**:
| File | Lines | Should Be |
|------|-------|-----------|
| PersonaUser.ts | 2,213 | <500 — extract to focused modules |
| PersonaResponseGenerator.ts | 1,174 | <500 — split RAG assembly, LLM formatting, generation, posting |
| DataDaemon.ts | 1,402 | <500 — extract collection ops, query builder, subscription mgmt |
| RustWorkerStorageAdapter.ts | 1,234 | <500 — extract per-operation modules |
| OllamaAdapter.ts | 1,225 | <500 — extract model management, streaming, tool parsing |
| ChatRAGBuilder.ts | 1,214 | <500 — extract source management, assembly, caching |
| JTAGClient.ts | 1,210 | <500 — extract transport, correlation, subscription |
| PersonaMessageEvaluator.ts | 909 | <500 — extract training signal, topic detection |

**Approach**: Extract using the existing module pattern (PersonaUser already has `modules/`). Each extraction is a PR with tests proving behavior preservation.

### 1C. Magic Number Consolidation

**Current**: Timing values scattered across 8+ files with no single source of truth.

**Fix**: `PersonaTimingConfig.ts` already exists but doesn't cover everything. Audit all timing constants and consolidate:
- Persona cadence (500ms, 1000ms, 2000ms, 3000ms)
- Dedup windows (3000ms in PersonaInbox)
- Poll intervals (10s, 30s, 60s in PersonaAutonomousLoop)
- Self-task intervals (30min, 1hr, 6hr in SelfTaskGenerator)
- Snapshot throttle (2000ms in PersonaState)
- Diamond persist (2500ms in PersonaTile)

### 1D. Rust Panic Safety

**Current**: 21+ `.lock().unwrap()` sites, regex `.unwrap()` without lazy init.

| Fix | Files | Impact |
|-----|-------|--------|
| `.lock().unwrap()` → `.lock().ok()` or match | logger.rs, orchestrator.rs | Prevents panic on mutex poison |
| `Regex::new().unwrap()` → `once_cell::Lazy` | agent.rs | Prevents panic on bad regex |
| `.ok()` on file writes → warn log | sentinel/executor.rs (7 sites) | Silent failures become visible |
| Duplicated error wrapping → `step_err!` macro | sentinel steps (~12 sites) | DRY |

### 1E. Missing ts-rs Exports

Types crossing Rust-TS boundary without `#[derive(TS)]`:
- `RagComposeRequest` (modules/rag.rs)
- Agent tool call types (modules/agent.rs)
- Various IPC request/response types

**Fix**: Audit all `CommandResult::Json()` returns — if it crosses the wire, it needs `#[derive(TS)]`.

---

## Priority 2: Pressure System Completion (Phases 1, 3)

**Why second**: Memory pressure awareness is half-built. Persona cadence responds to pressure (done), but chat flooding and voice broadcasting don't. Under load, 14 personas all try to respond and broadcast simultaneously.

### 2A. ThoughtStream Pressure-Aware Slots

**File**: `system/conversation/server/ThoughtStreamCoordinator.ts`
**Change**: `getProbabilisticMaxResponders()` reads `BackpressureService.pressureLevel`

| Pressure | Max Responders |
|----------|---------------|
| Normal | 1 (70%), 2 (25%), 3 (5%) |
| Warning | 1 (85%), 2 (15%) |
| High | 1 only |
| Critical | 0 (all rejected) |

**~20 lines of code**. Highest impact change in the pressure system.

### 2B. Voice Broadcast Gating

**File**: `system/voice/server/VoiceOrchestrator.ts`
**Change**: After `textAIs` filter, limit broadcast targets by pressure level.

| Pressure | Broadcast Targets |
|----------|------------------|
| Normal | All text AIs |
| Warning | Max 3 |
| High | Max 1 |
| Critical | None (let in-progress TTS finish) |

**~15 lines of code**. Closes the biggest gap — voice has ZERO pressure awareness today.

---

## Priority 3: Coordination Decision Logging (Phase 5C)

**Why third**: Answers "why is general chat incoherent" — full decision context for every respond/silent choice. 90% done, just needs final wiring.

### What's Done
- CoordinationDecisionEntity (630 lines) — complete
- CoordinationDecisionLogger (200+ lines) — complete
- EntityRegistry integration — complete
- PersonaUser import — complete

### What's Left (5 steps)
1. Modify `evaluateShouldRespond()` return type to include `filteredRagContext`
2. Return RAG context from the method
3. Add helper to convert RAG context format
4. Log SILENT decisions with full context
5. Log RESPOND decisions with full context

**Details**: `docs/planning/PHASE-5C-STATUS.md`

---

## Priority 4: Scene Lifecycle (Bug Fix)

**Issue**: Scenes don't unload when hanging up a live call. `AvatarCommand::UnloadIdle` fires (call_server.rs:679) but Bevy isn't cleaning up.

**Investigation needed**:
- Is the command reaching the Bevy command consumer?
- Is `try_get()` returning the Bevy instance?
- Are slots actually marked as idle when call ends?
- Does the render loop need to tick for teardown to execute?

**Impact**: GPU memory leak — scenes accumulate across calls.

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
- Silent `.ok()` swallowing in sentinel executor (7+ sites)
- Missing input validation: sentinel/run doesn't validate pipeline after JSON.parse()
- `e.to_string()` vs `format_pg_error()` pattern may exist in other Postgres sites

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
  ├── 1B: God class decomposition
  ├── 1C: Magic number consolidation
  ├── 1D: Rust panic safety
  └── 1E: ts-rs exports

P2: Pressure System              ← ~35 lines total, huge impact
  ├── 2A: ThoughtStream slots
  └── 2B: Voice broadcast gating

P3: Decision Logging (5C)        ← 90% done, just wiring

P4: Scene Lifecycle Bug           ← GPU memory leak

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
