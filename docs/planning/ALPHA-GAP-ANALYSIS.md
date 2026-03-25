# Alpha Gap Analysis — Master Plan

**Updated**: 2026-03-24
**Status**: UI/UX alpha complete. System runs stable with 14+ AI personas in live video calls.
**Branch**: `main`

This document is the **single source of truth** for remaining work before open-source launch. Each phase is ordered by dependency — later phases build on earlier ones. Every open GitHub issue is mapped to exactly one phase.

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
| Academy | Proven E2E | Dual-sentinel teacher/student, RealClassEval 53% pass (cloud) |
| Sentinel pipeline | Working | 12 step types, 55 Rust tests, CodingAgent integration |
| Sentinel workspaces | Working | Identity chain, git worktree isolation, lifecycle cleanup |
| Dev CLI front door | Working | `--repoPath` on all dev commands |
| Recipe-Sentinel convergence | Working | Recipes declare sentinelTemplates, RAG filters by recipe |
| Recipe commands | Working | recipe/list, recipe/run, recipe/generate |
| Capability registry | Working | Skill domains, all 10 adapters self-register |
| ORM | Working | SQLite + Postgres, schema evolution, self-healing |
| RAG (chat history) | Working | Tiered cache L1/L2, 30-50ms cached |
| RAG (codebase) | Proven E2E | CodebaseIndexer + CodebaseSearchSource, auto-index on startup |
| Vision pipeline | Proven E2E | Tiered perception, content-addressed cache |
| Neural compression | Proven E2E | Head pruning + Q3_K_S: 32B model on 32GB MacBook, 5.3 tok/s |
| Compression pipeline | Built | Planner + GGUF writer + pipeline orchestration, 142 tests |
| HuggingFace distribution | Live | continuum-ai/qwen2.5-coder-14b-compacted published |
| Local GGUF inference | Working | Candle Metal backend, Qwen2 architecture support |
| Auto model discovery | Working | CandleAdapter finds local GGUFs, falls back to HF download |
| Pressure system | Complete | ThoughtStream slots + voice broadcast gating (PR #304) |
| Decision logging | Complete | CoordinationDecisionLogger, full RAG context capture |
| Widget system | Working | 32 auto-discovered widgets, Lit + Shadow DOM |
| Command system | Working | 320 auto-discovered commands, zero central registries |
| AI providers | Working | 12 providers (Anthropic, OpenAI, DeepSeek, Google, Groq, xAI, Fireworks, Together, Mistral, Candle, Candle-gRPC, Sentinel) |
| continuum-core | Working | 26 Rust modules, 1,179+ tests |

---

## Phase 0: Critical Bugs (Ship-Blockers)

> Fix before anything else. These break the first-run experience.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#376](https://github.com/CambrianTech/continuum/issues/376) | **chat/send userId bug** | DONE (PR #387) | Fixed — resolves to human owner, not @cli/agent. |
| [#335](https://github.com/CambrianTech/continuum/issues/335) | **Multiple browser tabs on npm start** | DONE (PR #387) | Fixed — removed shell script browser launch, orchestrator handles it. |
| [#317](https://github.com/CambrianTech/continuum/issues/317) | **Live mode starts twice on page load** | DONE (PR #388) | Fixed — activation guard prevents duplicate join from racing code paths. |
| [#385](https://github.com/CambrianTech/continuum/issues/385) | **install.sh incomplete on new nodes** | TODO | Tower needed manual pytest install, API keys uncommenting. Needs cross-platform testing. |
| [#381](https://github.com/CambrianTech/continuum/issues/381) | **Headless health check timeout** | TODO | Grid nodes without browser can't be health-checked. Needs headless node to test. |
| [#373](https://github.com/CambrianTech/continuum/issues/373) | **Rust compiler ICE on Linux/WSL2** | TODO | Can't build continuum-core on the 5090 tower. Needs tower access. |

**Done when**: `git clone && cd src && npm install && npm start` works on macOS and Ubuntu. Personas chat. No duplicate tabs. Health checks pass on headless nodes.

---

## Phase 1: Architectural Integrity (Code Quality)

> Open-source contributors will copy these patterns. Fix the foundation before anyone sees it.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#333](https://github.com/CambrianTech/continuum/issues/333) | **Type safety — eliminate 831 `any` casts** | TODO | Production code has 831 `any` casts. Target: zero. |
| [#363](https://github.com/CambrianTech/continuum/issues/363) | **Eliminate hardcoded switch statements** | TODO | 72+ sites with type switch statements violating modular architecture. |
| [#362](https://github.com/CambrianTech/continuum/issues/362) | **Unify content routing** | TODO | Kill ContentTypeRegistry, route everything through recipes. |
| [#356](https://github.com/CambrianTech/continuum/issues/356) | **Enforce generator usage** | TODO | Prevent manual module creation without spec. |
| [#355](https://github.com/CambrianTech/continuum/issues/355) | **Generator v2: emit IPC mixins, health, ts-rs** | TODO | Generator must produce complete Rust+TS scaffolding. |
| [#353](https://github.com/CambrianTech/continuum/issues/353) | **Generator v2: Rust modules + tokio** | TODO | Full Rust module generation with IPC and tests. |
| [#351](https://github.com/CambrianTech/continuum/issues/351) | **Magic strings → command constants** | TODO | All Rust modules must use constants, not string literals. |
| [#361](https://github.com/CambrianTech/continuum/issues/361) | **Maximum lint/clippy strictness** | TODO | Enforce across TypeScript and Rust. |
| [#354](https://github.com/CambrianTech/continuum/issues/354) | **Git pre-push hooks** | TODO | Infrastructure and mission-critical test gates. |
| [#352](https://github.com/CambrianTech/continuum/issues/352) | **Formalize test architecture** | TODO | Unit, integration, infrastructure, mission-critical tiers. |
| [#379](https://github.com/CambrianTech/continuum/issues/379) | **Sentinel test coverage: 55 → 100+** | TODO | 12 step types need thorough coverage. Approve and WebResearch likely untested. |
| [#334](https://github.com/CambrianTech/continuum/issues/334) | **Technical debt deep clean** | TODO | ESLint config, disabled systems, error handling audit, 14 failing Rust tests. |
| [#360](https://github.com/CambrianTech/continuum/issues/360) | **ORM date/pagination/indexes** | INVESTIGATED | Dates work correctly (TIMESTAMPTZ/RFC3339). Composite indexes working for high-traffic tables. Cursor pagination unimplemented (OFFSET fine for alpha). |

**Previously completed:**
- 1D: Magic number consolidation (PersonaTimingConfig.ts) — DONE
- 1E: Rust panic safety — MOSTLY DONE (36 `.lock().unwrap()` intentional)
- 1F: ts-rs exports — DONE (10 types across 4 modules)
- God class decomposition — PARTIAL (DataSchemaManager, DataVectorOperations, JTAGClientConnections, PersonaAgentLoop extracted)

**Remaining god classes:**

| File | Lines | Target |
|------|-------|--------|
| PersonaUser.ts | ~2,200 | <500 |
| RustWorkerStorageAdapter.ts | 1,234 | <500 |
| ChatRAGBuilder.ts | 1,214 | <500 |
| PersonaMessageEvaluator.ts | 909 | <500 |

**Done when**: Zero `any` in production. All commands generator-backed. Lint/clippy clean. Pre-push hooks enforced. 100+ sentinel tests.

---

## Phase 2: Live Call Quality & Resource Management

> The 3D video calls work but leak memory, have high latency, and break offline.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#331](https://github.com/CambrianTech/continuum/issues/331) | **Live call quality** | TODO | Memory leaks (10.5GB RSS after close), 30s latency, simultaneous speech. |
| [#338](https://github.com/CambrianTech/continuum/issues/338) | **Deterministic resource deallocation** | TODO | Closing live chat must free resources in <30s, not idle timeout. |
| [#339](https://github.com/CambrianTech/continuum/issues/339) | **Live mode latency: 30s STT delay** | TODO | STT→LLM→TTS pipeline too slow. Need streaming TTS, speculative STT. |
| [#340](https://github.com/CambrianTech/continuum/issues/340) | **AIs talk over each other** | TODO | ThoughtStream coordination doesn't prevent overlap in live mode. |
| [#318](https://github.com/CambrianTech/continuum/issues/318) | **Avatar models eating 26GB** | TODO | Too many models loaded on Mac. GPU governor must manage. |
| [#322](https://github.com/CambrianTech/continuum/issues/322) | **More CC0 avatar models** | TODO | Equal male/female distribution needed. |
| [#332](https://github.com/CambrianTech/continuum/issues/332) | **Offline-first architecture** | TODO | Live calls fail without internet. All local features must work offline. |
| [#380](https://github.com/CambrianTech/continuum/issues/380) | **GPU governor: full consumer management** | TODO | Only 3 subsystems, no dynamic consumer tracking or pressure-driven eviction. |
| [#399](https://github.com/CambrianTech/continuum/issues/399) | **Persona response latency: 2+ min** | TODO | 15 personas thundering herd, no priority escalation for human messages, backpressure spikes to critical. |

**Done when**: Live call closes → memory returns to baseline in 30s. Latency under 5s. Airplane mode → local calls still work. GPU governor tracks all consumers. Human message gets AI response within 15s.

---

## Phase 3: Tool Calling & Local Model Reliability

> THE blocker for local-first AI. Personas can't reliably call tools with local models.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#324](https://github.com/CambrianTech/continuum/issues/324) | **Parser-per-model-family** | TODO | 7 parsers needed: Native, Hermes, Qwen, Llama, DeepSeek, Mistral, GenericXML. |
| [#368](https://github.com/CambrianTech/continuum/issues/368) | **PersonaToolExecutor failures** | IN PROGRESS | String(v) destroying object params, force-text too aggressive, double correction, code/search example bias. Fixes deployed, QA validated. |
| [#366](https://github.com/CambrianTech/continuum/issues/366) | **Personas can't reliably write code** | TODO | CodingAgent, tools, quality, full loop all broken. |
| [#367](https://github.com/CambrianTech/continuum/issues/367) | **CodingAgent dispatch unreliable** | TODO | Claude Code dispatch needs error handling and retry. |
| [#321](https://github.com/CambrianTech/continuum/issues/321) | **Local inference quality** | TODO | Compacted 14B gives poor responses. |
| [#325](https://github.com/CambrianTech/continuum/issues/325) | **Ship 14B model, research 32B QAT** | TODO | 14B at Q5_K for MacBook Air. 32B QAT for 32GB machines. |
| [#371](https://github.com/CambrianTech/continuum/issues/371) | **Per-task model routing** | TODO | Pick BEST available model for each task, not fixed per-persona. |
| [#343](https://github.com/CambrianTech/continuum/issues/343) | **Native multimodal** | TODO | Skip STT/TTS for models that handle audio/images directly. |
| [#342](https://github.com/CambrianTech/continuum/issues/342) | **Vision feedback** | TODO | Personas see screenshots, live visual context. |
| [#341](https://github.com/CambrianTech/continuum/issues/341) | **API cost budgeting** | TODO | Track and limit spend per persona/provider. |

**Done when**: Local 14B model reliably calls tools. Parser handles all model families. Per-task routing picks best model. Cost tracked.

---

## Phase 4: End-to-End Development Orchestration

> From "AI that chats" to "AI that ships code."

| # | Issue | Status | What |
|---|-------|--------|------|
| [#326](https://github.com/CambrianTech/continuum/issues/326) | **E2E dev orchestration** | TODO | Sentinel templates → auto-trigger → PR workflow → chat bridge. |
| [#370](https://github.com/CambrianTech/continuum/issues/370) | **Coding pipeline never proven** | TODO | write → test → fix → commit loop must actually complete. |

**Previously completed:**
- 3 sentinel dev templates (build-feature, fix-bug, code-review) — DONE
- TemplateRegistry — DONE
- SentinelChatBridge — DONE
- SentinelDispatchDecider — DONE

**Remaining:**
- [ ] 2 more templates (create-pr, refactor)
- [ ] PR workflow commands (push, create, review, status)
- [ ] Template parameter extraction from chat context
- [ ] Prove the full loop: chat request → sentinel → code → tests → commit → PR

**Done when**: Someone says "add rate limiting to the login endpoint" in chat → persona spawns sentinel → code written → tests pass → PR created. Proven, not theoretical.

---

## Phase 5: Academy — Full Training Loop

> The README promises personas get smarter every day. Prove it.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#377](https://github.com/CambrianTech/continuum/issues/377) | **Full academy session E2E** | TODO | All challenges → failures → LoRA trained → re-exam → measurable improvement. Never completed. |
| [#369](https://github.com/CambrianTech/continuum/issues/369) | **RealClassEval trash with local models** | TODO | 53% was with DeepSeek-Chat (cloud). Local models produce garbage. |
| [#374](https://github.com/CambrianTech/continuum/issues/374) | **Teacher needs cloud API** | TODO | Train a local teacher adapter so Academy works at $0. |
| [#365](https://github.com/CambrianTech/continuum/issues/365) | **Training job persistence** | TODO | Checkpoint resume, crash recovery, auto-restart for weeks-long runs. |
| [#344](https://github.com/CambrianTech/continuum/issues/344) | **Ship LoRA-tuned local model** | TODO | A model that passes coding challenges via our tool system. |
| [#345](https://github.com/CambrianTech/continuum/issues/345) | **LoRA-tuned persona layer** | TODO | Teach personas to use Continuum's own systems. |
| [#384](https://github.com/CambrianTech/continuum/issues/384) | **Team training** | TODO | Multi-persona project decomposition — roles, parallel training, collaborative building. |
| [#359](https://github.com/CambrianTech/continuum/issues/359) | **Training env auto-bootstrap** | TODO | Any Grid node can train — zero manual intervention. |

**The critical path:**
```
#374 (local teacher) → #377 (full session) → #369 (quality baseline)
    → #344 (ship tuned model) → #384 (team training)
```

**Done when**: A full academy session completes on the 5090 tower using only local models. Student scores improve after training. Adapter published to HuggingFace.

---

## Phase 6: Genome & Adapter Ecosystem

> Personas carry skills in their genome. Skills page in/out. Skills are shared globally.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#382](https://github.com/CambrianTech/continuum/issues/382) | **Genome paging not wired** | TODO | activateSkill/evictLRU exists but not connected to persona loop or GPU governor. |
| [#378](https://github.com/CambrianTech/continuum/issues/378) | **First HuggingFace adapter publication** | TODO | README promises `continuum:*` tags, searchable marketplace. Never published from system. |
| [#330](https://github.com/CambrianTech/continuum/issues/330) | **Adapter management** | TODO | Docker-like ops: list, prune, info. 58 old adapters hit 21GB before manual cleanup. |
| [#319](https://github.com/CambrianTech/continuum/issues/319) | **Separate install from start** | TODO | Detect if build needed. Don't rebuild every time. |

**Done when**: Persona faces a Python task → genome pages in python-expertise adapter → processes task → publishes adapter to HuggingFace → another instance discovers and pulls it.

---

## Phase 7: Autonomous Persona Life

> Not agents you invoke. Teammates who live.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#383](https://github.com/CambrianTech/continuum/issues/383) | **Self-task generation** | TODO | generateSelfTasks() not implemented. Personas only react, never initiate. |
| [#329](https://github.com/CambrianTech/continuum/issues/329) | **Persona-sentinel integration** | TODO | Autonomous dispatch, sentinel memory → RAG, NL → pipeline, multi-teacher. |
| [#336](https://github.com/CambrianTech/continuum/issues/336) | **First-run onboarding** | TODO | Guide users to configure API keys, understand the system. |

**Done when**: Leave the system running overnight → come back to find personas have consolidated memories, audited skills, searched HuggingFace for useful adapters, and initiated peer learning sessions. Without any human prompt.

---

## Phase 8: Distillation & Training Flywheel

> The competitive moat: every task makes the next task better.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#327](https://github.com/CambrianTech/continuum/issues/327) | **Distillation pipeline** | TODO | Capture → score → filter → train → evaluate → deploy → capture better data. |
| [#357](https://github.com/CambrianTech/continuum/issues/357) | **Persistent learning layer** | TODO | Continuum as learning layer for Claude Code and other AI dev tools. |

**Sub-tasks:**
- [ ] Composite quality scoring (replace binary 0.9/0.3)
- [ ] Quality-filtered training data pipeline (>0.7 threshold)
- [ ] Evaluation sentinel (benchmark new adapter vs. previous)
- [ ] Auto-rollback on regression
- [ ] Negative example training (failed tool calls + corrections)
- [ ] Flywheel automation: the full loop runs unattended

**Done when**: Helper AI improves from 53% → 70%+ on RealClassEval after one training cycle. Measured, not assumed.

---

## Phase 9: Codebase Intelligence

> Know what you're changing before you change it.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#328](https://github.com/CambrianTech/continuum/issues/328) | **Tree-sitter + dep graph** | TODO | Symbol extraction, dependency graph, sentinel context enrichment, LSP. |

**Sub-tasks:**
- [ ] Tree-sitter Rust worker for symbol extraction (TS, Rust, Python, JS)
- [ ] Symbol table storage via ORM (incremental, content-hashed)
- [ ] Dependency graph from import analysis
- [ ] `codebase/symbols` and `codebase/dependencies` commands
- [ ] Sentinel LLM step `contextSources` field
- [ ] Step-result summarization for long pipelines
- [ ] (Future) LSP integration

**Done when**: Persona modifying `auth.ts` automatically knows every file that imports it, every function that calls its methods, and every test that covers it — before writing a single line.

---

## Phase 10: Grid — Multi-Node Mesh

> Your machines form a single organism.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#323](https://github.com/CambrianTech/continuum/issues/323) | **Tailscale mesh for remote inference** | TODO | Multi-tower transparent command routing. |
| [#364](https://github.com/CambrianTech/continuum/issues/364) | **Cross-node event forwarding** | TODO | Events must propagate across Grid nodes (Rust plumbing). |
| [#349](https://github.com/CambrianTech/continuum/issues/349) | **Reticulum mesh** | TODO | MPC identity + encrypted transport. Replace Tailscale dependency. |
| [#337](https://github.com/CambrianTech/continuum/issues/337) | **Distributed inference + training** | TODO | Shard models and training across towers. |

**Done when**: MacBook Air coordinates. 5090 tower trains for a week. Checkpoint resumes across crashes. Training dashboard shows live progress from anywhere on the mesh.

---

## Issue Map — Every Open Issue, One Phase

| Phase | Issues | Count |
|-------|--------|-------|
| **0: Critical Bugs** | ~~#376~~, ~~#335~~, ~~#317~~, #385, #381, #373 | 3 remaining |
| **1: Arch Integrity** | #333, #363, #362, #356, #355, #353, #351, #361, #354, #352, #379, #334, #360 | 13 |
| **2: Live Quality** | #331, #338, #339, #340, #318, #322, #332, #380, #399 | 9 |
| **3: Tool Calling** | #324, #368, #366, #367, #321, #325, #371, #343, #342, #341 | 10 |
| **4: Dev Orchestration** | #326, #370 | 2 |
| **5: Academy** | #377, #369, #374, #365, #344, #345, #384, #359 | 8 |
| **6: Genome** | #382, #378, #330, #319 | 4 |
| **7: Autonomous** | #383, #329, #336 | 3 |
| **8: Distillation** | #327, #357 | 2 |
| **9: Codebase Intel** | #328 | 1 |
| **10: Grid** | #323, #364, #349, #337 | 4 |
| **Total** | | **62** |

---

## The Narrative

**Phase 0** removes the embarrassments — things that break the first-run experience.

**Phase 1** makes the codebase worthy of public scrutiny. Contributors will copy these patterns forever.

**Phase 2** makes the live video calls — the most visually impressive feature — actually reliable. No leaks, low latency, works offline.

**Phase 3** solves THE local model blocker. Without reliable tool calling, personas are chat decorations. With it, they're functional teammates.

**Phase 4** proves personas can CREATE things, not just discuss them. Code → tests → PR, end-to-end.

**Phase 5** proves personas get SMARTER over time. The full Academy loop, measured.

**Phase 6** makes trained skills portable and composable. The genome ecosystem.

**Phase 7** makes personas autonomous — they initiate work, not just respond to it.

**Phase 8** closes the flywheel — every task improves the next task. The competitive moat.

**Phase 9** gives personas deep codebase understanding. Know before you change.

**Phase 10** distributes everything across a mesh of commodity hardware. The Cell architecture realized.

---

## The Thesis

**Infrastructure > Model Capability.**

| Layer | What It Does | Why Models Don't Need To |
|-------|-------------|------------------------|
| **Sentinel Pipelines** | Deterministic orchestration: plan → code → build → test → fix → commit | Model doesn't need to "remember" to run tests — pipeline forces it |
| **Generator System** | Encodes correct patterns as code templates | Model doesn't need project conventions — generator enforces them |
| **LoRA Fine-Tuning** | Bakes domain expertise into weights | Model doesn't need 200K context of docs — it already knows |
| **Academy** | Structured training with deterministic evaluation | Model doesn't need to self-assess — benchmarks measure truth |
| **Parser-Per-Model** | Handles each model's unique tool-call format | Model doesn't need to conform to one format — parser adapts |
| **Workspace Isolation** | Git worktrees per task, rollback on failure | Model doesn't need to be careful — infrastructure catches mistakes |

A LoRA-tuned 3B running inside a `dev/build-feature` sentinel with shell verification, tree-sitter context, and automatic retry will produce working code more reliably than a prompted GPT-4 in a single-shot terminal. Because the infrastructure does what the model can't: remember, verify, retry, learn.

**The competitors' ceiling**: They need smarter models forever.

**Our ceiling**: Every task makes the next task better. The flywheel compounds. A persona training for 6 months on YOUR codebase, YOUR patterns, YOUR domain — fine-tuned on thousands of successful traces — running inside deterministic pipelines with full codebase intelligence — is not competing with Claude Code. It's competing with a junior developer who memorized your entire codebase. And it works offline, costs nothing per token, and never takes a day off.

---

## Superseded Documents

- `ARCHITECTURE-GAPS-PHASE1.md` — Gap 1 (RAG indexing) now proven E2E, covered in Phase 1/9
- `TECHNICAL-DEBT-AUDIT.md` — Updated numbers in Phase 1 (was 1,108 `any`, now 831)
- Previous version of this doc (2026-03-15) — replaced with phased issue-driven plan

**See also**: [COMPETITIVE-LANDSCAPE.md](COMPETITIVE-LANDSCAPE.md) | [SENTINEL-GAP-ANALYSIS.md](../sentinel/SENTINEL-GAP-ANALYSIS.md)
