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

## Priority 5: RAG Codebase Indexing (Full Integration) — MOSTLY COMPLETE

**Why fifth**: Foundation exists (CodebaseIndexer proven E2E), but not integrated into persona cognition. This is what makes the AI team actually useful — answering questions about the codebase.

### What's Done
- `CodebaseIndexer` — chunks TS/MD/JS, generates 384-dim embeddings via Rust IPC
- `CodebaseSearchSource` (RAGSource) — queries code_index, injects into system prompt (priority 55, already wired into ChatRAGBuilder)
- Relevance threshold 0.35 filters naturally
- Embedding mixin proven (Rust returns binary f32 data)
- **Index persistence** — ORM stores code_index entries in SQLite, survives restarts
- **Incremental indexing** — SHA-256 content hashes skip unchanged files on re-index, deleted files pruned automatically
- **Query optimization** — switched from load-all-then-topK to `ORM.vectorSearch()` (delegates to Rust vector similarity, no full scan)
- **Auto-indexing on startup** — ServiceInitializer triggers background incremental index 10s after boot (fire-and-forget, doesn't block startup)
- **Concurrency guard** — `indexing` flag prevents overlapping index operations

### What's Remaining
- **File watcher for live re-indexing** — currently re-indexes on startup only, not on file save (acceptable for alpha — `./jtag ai/rag/index-codebase` available for manual refresh)

### Success Metric
Ask Helper AI "Why does PersonaUser have both inbox and coordinator?" and get a correct, cited answer within 5 seconds.

---

## Priority 6: Tool Calling Reliability — The Local Model Blocker

**Why sixth**: Everything below depends on local models being able to call tools. Today they can't do it reliably. This is THE blocker. A persona with perfect Sentinel pipelines and a perfect workspace is useless if the model can't emit `<tool_use>code/edit...</tool_use>` correctly.

**Competitive insight**: Hermes Agent (Nous Research) solved this with 11 model-specific parsers — Hermes, Qwen, Llama3/4, DeepSeek v3/v3.1, Mistral, Kimi K2, GLM4.5/4.7, Qwen3-Coder. Each handles the model family's unique tool call format (XML tags, JSON blocks, Unicode delimiters, native arrays). Dual-path detection: native `tool_calls` first, XML/JSON text fallback.

**Our current state**: Single regex parser in PersonaAgentLoop. Works for Claude/OpenAI (native tool calling). Breaks constantly for local models that emit malformed XML, wrong parameter names, truncated tags, or model-specific formats like DeepSeek's `＜｜tool▁calls▁begin｜＞`.

**The thesis**: Sentinel pipelines make orchestration deterministic. LoRA makes domain expertise learnable. But neither matters if the model can't reliably invoke the tools. Parser-per-model-family is the bridge.

### 6A. Parser-Per-Model-Family Architecture

**New module**: `PersonaToolCallParser` with strategy pattern

| Parser | Model Family | Format | Key Quirks |
|--------|-------------|--------|------------|
| `NativeParser` | Claude, GPT, Groq | JSON `tool_calls` array | Clean, no parsing needed |
| `HermesParser` | Hermes-tuned (Nous) | `<tool_call>` XML | Standard XML, reliable |
| `QwenParser` | Qwen, Qwen3-Coder | `<tool_call>` with nested JSON | Escaping issues in JSON values |
| `LlamaParser` | Llama 3/4 | `<|python_tag|>` prefix + JSON | Non-standard prefix tag |
| `DeepSeekParser` | DeepSeek v3/R1 | Unicode fullwidth delimiters | `＜｜tool▁calls▁begin｜＞` |
| `MistralParser` | Mistral, Mixtral | `[TOOL_CALLS]` JSON array | Bracket-based format |
| `GenericXMLParser` | Fallback | `<tool_use>` / `<tool_call>` | Current regex, enhanced |

**Approach**:
- AIProviderDaemon already knows the model family per provider → pass parser hint to PersonaAgentLoop
- Each parser: `parse(text: string): ToolCall[] | null` — returns null if format not detected, falls through to next parser
- Dual-path: try native first (from API response), then text parsing with model-specific parser, then generic XML fallback
- Fuzzy tool name repair: `code_edit` → `code/edit`, `codeEdit` → `code/edit` (Hermes does this, we should too)
- Truncation recovery: if XML is truncated mid-tag, attempt to close it and parse partial

### 6B. ToolGroupRegistry → Model-Aware Examples

ToolGroupRegistry already provides few-shot examples per tool group. Extend to emit examples in the model's preferred format:
- Native models get JSON function-calling examples
- XML models get `<tool_use>` examples
- DeepSeek gets fullwidth-delimited examples

### 6C. Tool Call LoRA Fine-Tuning Dataset

The ultimate solution: fine-tune local models to call OUR tools correctly.
- Capture every successful tool call (model output → parsed → executed → success) as training data
- Generate synthetic tool-call training examples from tool definitions
- Academy dojo mode for tool calling: present tasks, grade by successful tool invocation, retrain on failures
- A 3B model fine-tuned on 1000 successful tool-call traces will outperform a 70B model that's never seen our tool schema

**Build checklist**:
- [ ] `PersonaToolCallParser` interface + strategy pattern
- [ ] 5 model-family parsers (Native, Hermes/Qwen, Llama, DeepSeek, Generic XML)
- [ ] Parser selection wired to AIProviderDaemon model family
- [ ] Fuzzy tool name repair (`ToolNameNormalizer` — partially exists in PersonaToolDefinitions)
- [ ] Truncation recovery for partial XML
- [ ] Model-format-aware few-shot examples in ToolGroupRegistry
- [ ] Training data capture for successful tool calls
- [ ] Academy dojo mode for tool-calling proficiency

---

## Priority 7: End-to-End Development Orchestration

**Why seventh**: With reliable tool calling (P6), personas can invoke tools. But invoking individual tools is not "creating things." Claude Code creates things because it has an agent loop that plans → edits → compiles → tests → fixes → iterates → commits. Our personas need the same loop, but built from Sentinel infrastructure instead of hardcoded.

**Competitive insight**: Devin ($500/month) and Claude Code are the only tools that do true end-to-end development. Both are single-agent, cloud-dependent, no training, no persistence. Cursor runs 8 parallel agents in worktrees — 35% of their PRs are agent-authored. Nobody does this locally. Nobody's agents get better at it over time.

**Our advantage**: Sentinel pipelines are JSON-serializable data that personas can create, save, share, and modify. A "build feature" sentinel is a reusable template. A LoRA-trained local model inside a sentinel pipeline with shell verification steps doesn't need to be "smart enough to remember to run tests" — the pipeline MAKES it run tests. The model just writes code. **Infrastructure compensates for model capability.**

### 7A. Sentinel Development Templates

Pre-built pipeline templates stored as JSON. Personas invoke by name, not by constructing JSON.

| Template | Steps | What It Does |
|----------|-------|-------------|
| `dev/build-feature` | LLM(plan) → Shell(branch) → Loop[LLM(code) → Shell(build) → Condition(pass?) → Shell(test)] → Shell(commit) | Plan, implement, verify, commit |
| `dev/fix-bug` | LLM(diagnose) → Shell(reproduce) → Loop[LLM(fix) → Shell(build) → Shell(test)] → Shell(commit) | Reproduce, fix, verify |
| `dev/create-pr` | Shell(git status) → LLM(summarize) → Shell(push) → Shell(gh pr create) | Summarize changes, push, open PR |
| `dev/review-pr` | Shell(gh pr diff) → LLM(review) → Shell(gh pr comment) | Analyze diff, post review comments |
| `dev/refactor` | LLM(plan) → Parallel[LLM(edit-file) × N] → Shell(build) → Shell(test) → Shell(commit) | Multi-file refactor with parallel edits |

**Key design**: Templates use Sentinel's existing step interpolation (`{{steps.N.output}}`, `{{input.feature_description}}`). The LLM steps are where the model's intelligence matters — everything else is deterministic shell execution. A LoRA-tuned 3B model fills in the LLM blanks; the pipeline handles orchestration, verification, and retry.

### 7B. Auto-Triggering From Chat

**Current state**: Persona receives chat message → generates text response (maybe with tool calls). For complex tasks, persona must manually invoke `sentinel/coding-agent` or `sentinel/run`.

**Target**: Persona receives "build an auth middleware" → recognizes this needs a sentinel → selects `dev/build-feature` template → fills parameters from context → spawns sentinel → reports progress to chat.

**Implementation**:
- Add `SentinelDispatchDecider` to PersonaResponseGenerator (parallel to PersonaEngagementDecider)
- Decision logic: if task complexity exceeds single-turn tool loop (>3 files, needs build/test, multi-step), dispatch sentinel instead
- Persona announces in chat: "This needs a multi-step pipeline. Spawning `dev/build-feature` sentinel..."
- Sentinel progress events flow back to chat via SentinelEventBridge
- On completion, persona posts summary + diff to chat

### 7C. PR Workflow (GitHub Integration)

The last mile of "creating things" — code changes must become PRs.

- [ ] `code/git/push` — push workspace branch to remote (Workspace.gitPush already exists)
- [ ] `code/pr/create` — create PR via `gh pr create` (shell command, wrapped as typed command)
- [ ] `code/pr/review` — post review comments via `gh api`
- [ ] `code/pr/status` — check CI status, review comments
- [ ] Wire into `dev/create-pr` sentinel template

### 7D. Sentinel Progress → Chat Bridge

Sentinels currently report via `sentinel/status` polling. For personas creating things, progress must flow into the room naturally.

- [ ] SentinelEventBridge emits to chat room when sentinel step completes
- [ ] Errors are posted immediately (don't wait for pipeline completion)
- [ ] Final result includes summary, files changed, test results, commit hash
- [ ] Other personas in the room can see and react to progress

**Build checklist**:
- [ ] 5 sentinel development templates (build-feature, fix-bug, create-pr, review-pr, refactor)
- [ ] Template loader in SentinelRunServerCommand (load by name, not just inline JSON)
- [ ] SentinelDispatchDecider in response pipeline
- [ ] Auto-triggering for complex tasks
- [ ] PR workflow commands (push, create, review, status)
- [ ] SentinelEventBridge → chat room integration
- [ ] Template parameter extraction from chat context (LLM step)

---

## Priority 8: Distillation Pipeline — The Flywheel That Makes Local Models Excel

**Why eighth**: P6 makes local models call tools. P7 makes them create things via sentinels. P8 makes them get BETTER at it over time. This is the flywheel that no competitor has.

**Competitive insight**: NVIDIA's Data Flywheel achieved 98% of a 70B model's tool-calling accuracy with a 1B model through distillation. FireAct showed 500 GPT-4 trajectories → 77% improvement in fine-tuned Llama2-7B. DeepSeek-R1 used 800K reasoning traces for SFT-only distillation. The research is clear: **small models + good training data > large models + prompting**.

**Our unique position**: We already capture training data from CodingAgent sessions (`captureTraining=true`). We already have the LoRA training pipeline (PEFT, proven E2E). We already have Academy for structured training. What's missing is **closing the loop**: capture → score → filter → train → evaluate → deploy → capture better data.

**See**: [SENTINEL-GAP-ANALYSIS.md](../sentinel/SENTINEL-GAP-ANALYSIS.md) GAP 5 for detailed analysis.

### 8A. Composite Quality Scoring

Replace binary 0.9/0.3 scoring with multi-dimensional quality metrics.

```
TraceQualityScore {
  outcome: 0-1      // did the task succeed?
  correctness: 0-1   // does code compile, do tests pass?
  efficiency: 0-1    // steps taken vs. optimal (fewer tool calls = better)
  complexity: 0-1    // task difficulty (trivial read vs. multi-file feature)
  novelty: 0-1       // different from existing training data?
  composite() → weighted sum
}
```

### 8B. Evaluation Sentinel

After every LoRA training, run an evaluation sentinel that benchmarks the new adapter.
- Held-out task set (not used in training)
- Compare new adapter vs. previous version vs. base model
- Auto-rollback if regression detected
- Report: "Helper AI improved from 53% → 67% on Python class implementation"

### 8C. The Full Flywheel

```
Teacher (Claude Code/Codex/Aider)
  → Completes coding task via sentinel
  → Interaction traces captured with quality scores
  → Filtered by composite quality (>0.7 threshold)
  → LoRA training on filtered traces
  → Evaluation sentinel benchmarks new adapter
  → If improved: deploy adapter, persona uses it for next task
  → If regressed: rollback, adjust training mix
  → Persona uses new adapter → captures traces from its own work
  → Successful traces feed back into training pool
  → The student becomes the teacher for the NEXT student
```

### 8D. Negative Example Training

Agent-FLAN research shows including failed traces with corrections reduces hallucination.
- Capture failed tool calls (wrong params, non-existent tools, truncated XML)
- Pair each failure with the corrected version
- Training data: `[bad_output, correction]` pairs
- This directly trains local models to avoid the exact failure modes from P6

**Build checklist**:
- [ ] TraceQualityScore composite scoring (replace binary 0.9/0.3)
- [ ] Quality-filtered training data pipeline (>0.7 threshold)
- [ ] Evaluation sentinel template (`eval/benchmark-adapter`)
- [ ] Auto-rollback on regression
- [ ] Negative example capture (failed tool calls + corrections)
- [ ] Flywheel automation: capture → score → filter → train → evaluate → deploy
- [ ] Replay buffer: mix 20% historical best traces with new data

---

## Priority 9: Codebase Intelligence

**Why ninth**: Personas creating things (P7) need to understand the codebase they're modifying. Currently, each task starts blind — the model must re-explore every time.

**Competitive insight**: Aider uses PersonalizedPageRank on tree-sitter dependency graphs. Cursor indexes entire codebases into vector DB with sub-100ms lookup. Sweep processes 2M+ files/day with CST entity extraction. OpenCode has native LSP integration for 20+ languages.

**Our current state**: CodebaseIndexer (P5) chunks files and generates embeddings. CodebaseSearchSource injects relevant code into persona context. This is keyword/semantic search — good for "find files about X" but not for "what does changing this function break?"

**See**: [SENTINEL-GAP-ANALYSIS.md](../sentinel/SENTINEL-GAP-ANALYSIS.md) GAP 1, GAP 2 for detailed analysis.

### 9A. Symbol Extraction (Tree-Sitter)

Rust worker using tree-sitter to extract structured symbols from source files.
- Functions, classes, types, interfaces, imports, exports
- Per-file symbol table stored in SQLite via ORM
- Incremental: re-parse only changed files (SHA-256 content hash, same pattern as CodebaseIndexer)
- Expose via `codebase/symbols` command

### 9B. Dependency Graph

Build import/call graph across files from extracted symbols.
- "This function is called by 5 other functions in 3 files"
- "Changing this type breaks these 12 consumers"
- PersonalizedPageRank (Aider's approach) to rank files by relevance to current task
- Expose via `codebase/dependencies` command

### 9C. Sentinel Context Enrichment

LLM steps in sentinel pipelines auto-receive relevant codebase context.
- `contextSources` field on LLM steps: `["codebase.symbols", "codebase.dependencies"]`
- Template variable: `{{codebase.relevant_to_file("src/auth.ts")}}` → symbol table + dependency graph for that file
- Long pipeline context management: step-result summarization to prevent context rot (GSD pattern)

### 9D. LSP Integration (Future)

Native LSP for diagnostics, go-to-definition, find-references.
- TypeScript (tsserver), Rust (rust-analyzer), Python (pyright)
- Diagnostics as a first-class tool: `codebase/diagnostics` returns compiler errors structured, not as raw text
- Go-to-definition enables models to follow code paths without guessing

**Build checklist**:
- [ ] Tree-sitter Rust worker for symbol extraction (TS, Rust, Python, JS)
- [ ] Symbol table storage via ORM (incremental, content-hashed)
- [ ] Dependency graph from import analysis
- [ ] `codebase/symbols` and `codebase/dependencies` commands
- [ ] Sentinel LLM step `contextSources` field
- [ ] Step-result summarization for long pipelines
- [ ] (Future) LSP integration for diagnostics

---

## Priority 10: Persona-Sentinel Deep Integration

**Why tenth**: With P6-P9, personas have reliable tools, development pipelines, a training flywheel, and codebase intelligence. P10 makes all of this AUTONOMOUS — personas create sentinels as naturally as they form thoughts.

**The vision** (from SENTINEL-GAP-ANALYSIS.md GAP 7): Sentinels are currently **adjacent** to personas — invoked explicitly. They should be **part of** persona cognition. A persona should think "I need to learn TypeScript testing" and autonomously spawn an Academy session, or think "this code needs reviewing" and spawn a review sentinel, without human instruction.

### 10A. Autonomous Sentinel Dispatch

Wire sentinel dispatch into PersonaUser's autonomous task generation (`generateSelfTasks()`).
- Persona analyzes own failure patterns → spawns Academy dojo in weak areas
- Persona detects PR merged → spawns review sentinel
- Persona notices test failures in chat → spawns fix-bug sentinel
- Persona reaches training data threshold → spawns training sentinel

### 10B. Sentinel Memory → Persona RAG

Sentinel execution results feed back into persona's RAG context.
- Completed sentinel → summary stored as persona memory
- "I built auth middleware last week using Express + JWT" → searchable via Hippocampus
- Failed sentinels → lessons learned stored as negative examples
- Cross-persona: if Helper AI's sentinel succeeded, Teacher AI can find that memory

### 10C. Natural Language → Pipeline

Persona describes a pipeline in natural language → LLM step generates JSON pipeline definition.
- "Build a feature that adds rate limiting to the login endpoint, with tests"
- → LLM generates `dev/build-feature` template with filled parameters
- Persona reviews generated pipeline, modifies if needed, then dispatches
- Per-persona template library: persona saves and reuses pipelines that worked

### 10D. Multi-Teacher Distillation

Multiple external agents as teachers, each captured in the same training format.
- `ClaudeCodeProvider` (already implemented)
- `CodexProvider` (OpenAI Codex CLI)
- `AiderProvider` (subprocess-based, good for git workflow expertise)
- Multi-teacher training merges traces from all providers → more robust student
- Domain routing: route traces to domain-specific adapters (Python traces → python-expertise adapter)

**Build checklist**:
- [ ] Sentinel dispatch in `generateSelfTasks()` based on failure analysis
- [ ] Sentinel execution summaries stored as persona memories
- [ ] Natural language → pipeline JSON generation (LLM step)
- [ ] Per-persona sentinel template library
- [ ] CodexProvider implementation
- [ ] AiderProvider implementation
- [ ] Multi-teacher training data merger
- [ ] Domain-specific adapter routing

---

## Priority 11: Adapter Management

**User request**: Docker-like management for LoRA adapters.
- 58 old adapters accumulated to 21GB before manual cleanup
- Need `genome/adapter-list`, `genome/adapter-prune`, `genome/adapter-info` commands
- Integrate with EvictionRegistry for usage tracking
- LRU eviction policy: auto-prune adapters not used in N days

---

## Priority 12: Technical Debt Deep Clean

### 12A. ESLint Configuration
- Missing `@typescript-eslint/eslint-plugin`
- Can't catch type errors, unused imports, style violations
- Technical debt accumulates unchecked

### 12B. Disabled Systems (Dead Code or Re-enable)
- Circuit breaker: `maxConsecutiveFailures: 999999` in BaseAIProviderAdapter (disabled)
- PersonaState energy rates: effectively 0 (thermodynamic system dead)
- Decision: re-enable with proper values, or remove entirely

### 12C. Error Handling Audit
- Silent `.ok()` swallowing in sentinel executor (7+ sites) — **FIXED in P1E**
- Missing input validation: sentinel/run doesn't validate pipeline after JSON.parse()
- `e.to_string()` vs `format_pg_error()` pattern may exist in other Postgres sites

### 12D. Rust Test Failures (14 pre-existing)
- 6 `modules::data::tests::*` — data module integration tests (SQLite-related)
- 3 `orm::connection_manager::tests::*` — connection pooling / LRU eviction
- 3 `orm::migration::tests::*` — migration pause/resume, verify, cross-db
- 1 `orm::sqlite::tests::test_create_and_read` — basic CRUD assertion failure
- 1 `rag::engine::tests::test_parallel_source_loading` — timing-dependent race condition
- Total: 1261 pass, 14 fail, 25 ignored (as of 2026-03-14)

---

## The Thesis: Infrastructure > Model Capability

The competitive landscape reveals a market fixated on smarter models. Every competitor races for larger context windows, better reasoning, faster inference. Continuum takes the opposite bet:

**Dumb models + smart infrastructure > smart models + no infrastructure.**

| Layer | What It Does | Why Models Don't Need To |
|-------|-------------|------------------------|
| **Sentinel Pipelines** | Deterministic orchestration: plan → code → build → test → fix → commit | Model doesn't need to "remember" to run tests — pipeline forces it |
| **Generator System** | Encodes correct patterns as code templates | Model doesn't need to know project conventions — generator enforces them |
| **LoRA Fine-Tuning** | Bakes domain expertise into weights | Model doesn't need 200K context of documentation — it already knows |
| **Academy** | Structured training with deterministic evaluation | Model doesn't need to self-assess — benchmarks measure truth |
| **Parser-Per-Model** | Handles each model's unique tool-call format | Model doesn't need to conform to one format — parser adapts |
| **Workspace Isolation** | Git worktrees per task, rollback on failure | Model doesn't need to be careful — infrastructure catches mistakes |

A LoRA-tuned Llama 3.2 3B running inside a `dev/build-feature` sentinel with shell verification, tree-sitter context enrichment, and automatic retry on compilation failure will produce working code more reliably than a prompted GPT-4 in a single-shot terminal session. Because the infrastructure does what the model can't: remember, verify, retry, learn.

**The competitors' ceiling**: They need smarter models forever. Their agents are exactly as good as the latest API release.

**Our ceiling**: Every task our agents complete makes them better at the next task. The flywheel compounds. A persona that's been training for 6 months on YOUR codebase, YOUR patterns, YOUR domain — fine-tuned on thousands of successful tool-call traces — running inside deterministic pipelines with full codebase intelligence — is not competing with Claude Code. It's competing with a junior developer who has memorized your entire codebase. And it works offline, costs nothing per token, and never takes a day off.

**See also**: [COMPETITIVE-LANDSCAPE.md](COMPETITIVE-LANDSCAPE.md) for full market analysis, [SENTINEL-GAP-ANALYSIS.md](../sentinel/SENTINEL-GAP-ANALYSIS.md) for detailed technical gaps and research references.

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

P5: RAG Codebase Indexing         ← MOSTLY COMPLETE (incremental + auto-index + vectorSearch)

P6: Tool Calling Reliability      ← THE local model blocker
  ├── 6A: Parser-per-model-family (7 parsers)
  ├── 6B: Model-aware few-shot examples
  └── 6C: Tool-call LoRA training dataset

P7: E2E Development Orchestration ← Personas creating things, not describing things
  ├── 7A: Sentinel development templates (5 templates)
  ├── 7B: Auto-triggering from chat
  ├── 7C: PR workflow (GitHub integration)
  └── 7D: Sentinel progress → chat bridge

P8: Distillation Pipeline         ← The flywheel: agents that get better
  ├── 8A: Composite quality scoring
  ├── 8B: Evaluation sentinel
  ├── 8C: Full flywheel automation
  └── 8D: Negative example training

P9: Codebase Intelligence         ← Know what you're changing
  ├── 9A: Tree-sitter symbol extraction
  ├── 9B: Dependency graph
  ├── 9C: Sentinel context enrichment
  └── 9D: LSP integration (future)

P10: Persona-Sentinel Integration ← Autonomous creation
  ├── 10A: Autonomous sentinel dispatch
  ├── 10B: Sentinel memory → persona RAG
  ├── 10C: Natural language → pipeline
  └── 10D: Multi-teacher distillation

P11: Adapter Management           ← Docker-like adapter ops

P12: Technical Debt Deep Clean    ← ESLint, dead code, error handling
```

**The narrative**: P1-P5 built the foundation (types, pressure, logging, lifecycle, indexing). P6-P10 are the **creation stack** — the path from "AI that chats" to "AI that ships code." P6 makes tools work. P7 makes tools compose into workflows. P8 makes workflows improve over time. P9 gives workflows understanding. P10 makes it all autonomous. P11-P12 are maintenance.

Each priority is a feature branch, a PR, a merge. No mixing concerns across branches.

---

## Superseded Documents

The following planning docs are partially or fully superseded by this analysis:
- `ARCHITECTURE-GAPS-PHASE1.md` — Gap 1 (RAG indexing) now proven E2E, covered in P5
- `TECHNICAL-DEBT-AUDIT.md` — Updated numbers in P1 (was 1,108 `any`, now 831 production + 1,269 test)
- `ELEGANCE-AUDIT-2026-02-15.md` — Rust issues covered in P1D, TS issues in P1A-C
- `PHASE-5C-STATUS.md` — Still accurate, referenced from P3
- `PRACTICAL-ROADMAP.md` — Milestones 2-5 covered in P5, P7, P8, P9, P10
- `PHASE-5C-INTEGRATION-PLAN.md` — Detailed steps for P3
- `CODING-AI-FOUNDATION.md` — Tiers 1-4 partially superseded; memory persistence done, tool safety done, coding enablement now in P6-P7
- `SENTINEL-GAP-ANALYSIS.md` — Still accurate as deep technical reference; gaps mapped to P6 (GAP tool calling), P7 (GAP 4 UX), P8 (GAP 5 quality), P9 (GAP 1 codebase, GAP 2 context), P10 (GAP 7 persona integration, GAP 6 multi-provider)
