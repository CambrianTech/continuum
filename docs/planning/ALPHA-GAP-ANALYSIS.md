# Alpha Gap Analysis — Master Plan

**Updated**: 2026-04-06
**Status**: **RECIPE-DRIVEN UI MERGED (PR #790).** URL scheme verb/noun, RecipeEntity as proper ORM entity (view, entityType, team, modes, locked), right panel widgets from recipes not hardcoded. 19,774 HF downloads. ORT panic fix committed. Stability issues identified: IPC reconnection (#793), event bridge (#794), duplicate tabs (#795). Docker+Live+Grid E2E validation next (#796). Custom STT/TTS forging planned (#800, #801).
**Branch**: `main`

This document is the **single source of truth** for remaining work. Each phase is ordered by dependency — later phases build on earlier ones. Every open GitHub issue is mapped to exactly one phase. Issues are breadcrumbs on the path to fruition — not a backlog to dread.

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
| Command system | Working | 339 auto-discovered commands, zero central registries |
| AI providers | Working | 12 providers (Anthropic, OpenAI, DeepSeek, Google, Groq, xAI, Fireworks, Together, Mistral, Candle, Candle-gRPC, Sentinel) |
| continuum-core | Working | 26 Rust modules, 1,179+ tests |

---

## Phase 0: Critical Bugs (Ship-Blockers)

> Fix before anything else. These break the first-run experience.

### SECURITY — Identity & Sessions (BLOCKS GRID, MULTI-USER, EVERYTHING)

| # | Issue | Status | What |
|---|-------|--------|------|
| [#568](https://github.com/CambrianTech/continuum/issues/568) | **Session identity broken — all-zeros UUIDs** | PARTIAL | Browser sessions now get real userId (`./jtag ping` returns `18db7494`). Fixed: browser command, generator template (343 commands), session destroy. Remaining: CommandDaemon fallback, server-internal session. |
| [#566](https://github.com/CambrianTech/continuum/issues/566) | **Tab reconnection — tabs multiply, sessions orphaned** | PARTIAL | CLI now works so browser detection on `npm start` can refresh existing tabs. Root cause of duplicate tabs: CLI was broken (generator main blocks in esbuild). Fixed. Remaining: proper session rebinding on WebSocket reconnect. |
| [#565](https://github.com/CambrianTech/continuum/issues/565) | **WSL2 auto-start on boot** | PARTIAL | wsl-boot.sh fixed (uses LAN gateway DNS, not 8.8.8.8). PR #581 merged. Remaining: Windows scheduled task setup, `generateResolvConf=false` auto-config. |

**Done when**: Every connection has a real UUID. Reconnecting tabs rebind to existing sessions. `userId` is required (not optional) on every contract. Zero-UUID requests are rejected.

### Bugs

| # | Issue | Status | What |
|---|-------|--------|------|
| [#376](https://github.com/CambrianTech/continuum/issues/376) | **chat/send userId bug** | DONE (PR #387) | Fixed — resolves to human owner, not @cli/agent. |
| [#335](https://github.com/CambrianTech/continuum/issues/335) | **Multiple browser tabs on npm start** | DONE (PR #387) | Fixed — removed shell script browser launch, orchestrator handles it. |
| [#317](https://github.com/CambrianTech/continuum/issues/317) | **Live mode starts twice on page load** | DONE (PR #388) | Fixed — activation guard prevents duplicate join from racing code paths. |
| [#385](https://github.com/CambrianTech/continuum/issues/385) | **install.sh incomplete on new nodes** | TODO | Tower needed manual pytest install, API keys uncommenting. Needs cross-platform testing. |
| — | **Duplicate seed systems** | DONE | Dead code deleted (PR #608): RoomDataSeed, DataSeeder, UserDataSeed, seedUsers, seed-data, clear-data — 1,362 lines removed. Kept: SeedConstants, ActivityDataSeed, SystemIdentity (still used by seed-continuum.ts). |
| — | **Seeding fragile on fresh installs** | BUG | Seeding is buggy, inefficient, and prone to complete failure on new installs. Needs single reliable path that works every time. |
| [#599](https://github.com/CambrianTech/continuum/issues/599) | **Live mode STT broken** | DONE | Three-layer fix: orphan watchdog timeout 60s→600s (#600), spawn_blocking for ORT deadlock (#601), ORT_DYLIB_PATH in start-workers.sh, install.sh auto-installs onnxruntime (#604). |
| [#585](https://github.com/CambrianTech/continuum/issues/585) | **Workspace root '/path/to/project'** | DONE | Reject LLM placeholder paths in coding-agent workspace bootstrap (#590). |
| [#591](https://github.com/CambrianTech/continuum/issues/591) | **Tool expanders empty** | PARTIAL | Store truncated 2KB fullData preview (#592). Full lazy-load via command still TODO. |
| [#564](https://github.com/CambrianTech/continuum/issues/564) | **Grid missing local machine** | DONE | Local node always appears as node zero (#595). |
| [#606](https://github.com/CambrianTech/continuum/issues/606) | **Persona thundering herd** | DONE | 2s stagger between persona boot (#607). Verified — 5+ AIs responding. |
| [#603](https://github.com/CambrianTech/continuum/issues/603) | **Rust memory leak 3.2GB** | TODO | continuum-core leaks on ai/generate, data/query. OOMs after ~30 min. Needs Rust profiling. |
| — | **Content routing: all non-chat → chat-widget** | DONE | Generator reads new widgets[] format (#598), check generated config before async recipe service (#597). Live, factory, grid, logs all route correctly now. |
| — | **CLI bundle broken (readFileSync on argv)** | DONE | Removed generator main blocks that esbuild executed at bundle time (#581). |
| [#381](https://github.com/CambrianTech/continuum/issues/381) | **Headless health check timeout** | TODO | Grid nodes without browser can't be health-checked. Needs headless node to test. |
| [#373](https://github.com/CambrianTech/continuum/issues/373) | **Rust compiler ICE on Linux/WSL2** | TODO | Can't build continuum-core on the 5090 tower. Needs tower access. |
| [#792](https://github.com/CambrianTech/continuum/issues/792) | **ORT panic crashes server** | DONE | `tokio::task::spawn` catches ORT dylib panics. Voice degrades, core stays alive. |
| [#793](https://github.com/CambrianTech/continuum/issues/793) | **IPC reconnection — Node doesn't recover** | TODO | When Rust core restarts, Node.js IPC client stays wedged. Total system death until `npm start`. |
| [#794](https://github.com/CambrianTech/continuum/issues/794) | **AI messages don't reach browser** | TODO | Messages stored in DB but WebSocket event bridge doesn't forward `data:chat_messages:created` for AI senders. Requires page refresh. |
| [#795](https://github.com/CambrianTech/continuum/issues/795) | **Duplicate tabs** | TODO | Same room opens multiple tab entries. `contentItemsMatch()` dedup has gaps. |

**Done when**: `git clone && cd src && npm install && npm start` works on macOS and Ubuntu. Personas chat. No duplicate tabs. Health checks pass on headless nodes. AI responses appear in real-time without refresh.

---

## Phase 1: Architectural Integrity (Code Quality)

> Open-source contributors will copy these patterns. Fix the foundation before anyone sees it.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#333](https://github.com/CambrianTech/continuum/issues/333) | **Type safety — eliminate 831 `any` casts** | DONE (PR #408, #414) | 831 → 0. Next: ESLint no-explicit-any as error. |
| [#363](https://github.com/CambrianTech/continuum/issues/363) | **Eliminate hardcoded switch statements** | DONE (investigated) | 150 switches are legitimate discriminated unions. Command name switches already eliminated by dynamic discovery. |
| [#362](https://github.com/CambrianTech/continuum/issues/362) | **Unify content routing** | PARTIAL | Room selection now uses `room.recipeId` as contentType instead of hardcoded 'chat'. Factory, logs, canvas, help rooms route to correct widgets. ContentTypeRegistry still exists but delegates to RecipeLayoutService. Remaining: URL routing, full recipe-driven panel composition. |
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
| [#412](https://github.com/CambrianTech/continuum/issues/412) | **chat/send sender identity** | DONE (PR #422) | Persona tool calls now show as persona. Uses params.userId (auto-injected). |

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
| [#331](https://github.com/CambrianTech/continuum/issues/331) | **Live call quality** ⚠️ CRITICAL | TODO | Avatar vertex corruption — most personas show shredded/exploded geometry in live view. 8 VRM models for 15 personas = overflow models garbled. Also: memory leaks, latency, simultaneous speech. |
| ~~[#338](https://github.com/CambrianTech/continuum/issues/338)~~ | **Deterministic resource deallocation** | DONE | Merged into #331. |
| [#582](https://github.com/CambrianTech/continuum/issues/582) | **Native multimodal pipeline** ⚠️ HIGH | TODO | Direct audio/vision for capable models (one hop, <2s), bridge only for text-only. Three parallel streams: LISTEN + THINK + SPEAK. Fundamental architecture fix. |
| [#339](https://github.com/CambrianTech/continuum/issues/339) | **Live mode latency: 30s STT delay** | SUPERSEDED by #582 | STT→LLM→TTS pipeline too slow. #582 eliminates the pipeline entirely for multimodal models. |
| ~~[#340](https://github.com/CambrianTech/continuum/issues/340)~~ | **AIs talk over each other** | DONE | Merged into #331. |
| ~~[#318](https://github.com/CambrianTech/continuum/issues/318)~~ | **Avatar models eating 26GB** | DONE | Cleaned up — 8 CC0 VRoid models only. |
| [#322](https://github.com/CambrianTech/continuum/issues/322) | **More CC0 avatar models** ⚠️ CRITICAL | TODO | Only 8 models for 15 personas. Overflow causes vertex corruption. Need 15+ working VRM 0.x models. |
| ~~[#332](https://github.com/CambrianTech/continuum/issues/332)~~ | **Offline-first architecture** | DONE | No CDN deps. Works offline. |
| ~~[#380](https://github.com/CambrianTech/continuum/issues/380)~~ | **GPU governor** | DONE | Superseded by #469 (Grid Governor). |
| ~~[#399](https://github.com/CambrianTech/continuum/issues/399)~~ | **Persona response latency** | DONE | Priority boost (PR #423), event coalescing (PR #466), timeout fix (PR #460). |
| [#409](https://github.com/CambrianTech/continuum/issues/409) | **Sensory system verification** | TODO | Vision, screenshots, live mode visual awareness. |
| [#436](https://github.com/CambrianTech/continuum/issues/436) | **Cost/metrics widgets** | TODO | Auto-adjust time segments. |
| [#473](https://github.com/CambrianTech/continuum/issues/473) | **Grid telemetry widget** | TODO | SCADA-style per-node CPU/MEM/GPU + sparklines. |

| [#797](https://github.com/CambrianTech/continuum/issues/797) | **LiveKit + livekit-bridge Docker validation** | TODO | Validate three-binary split works in Docker. Bridge socket, audio pipeline, browser call join. |
| [#799](https://github.com/CambrianTech/continuum/issues/799) | **Qwen3.5 native audio — skip VAD→STT→LLM→TTS** | TODO | Audio-native models bypass the entire pipeline. Router exists in `live/audio/router.rs`. Needs Qwen3.5-Omni GGUF. |
| [#800](https://github.com/CambrianTech/continuum/issues/800) | **Custom forged STT model** | TODO | Whisper-equivalent trained on technical vocabulary. Publish as `continuum-ai/whisper-forged`. |
| [#801](https://github.com/CambrianTech/continuum/issues/801) | **Custom TTS voices per persona** | TODO | Persona-specific voice synthesis via Pocket-TTS cloning + fine-tuning. |

**Done when**: Avatar geometry works for ALL personas (no vertex corruption). Live call closes → memory baseline in 30s. Latency under 5s. All personas can see. Grid telemetry visible. Native audio models skip STT/TTS chain.

---

## Phase 3: Tool Calling & Local Model Reliability

> THE blocker for local-first AI. Personas can't reliably call tools with local models.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#324](https://github.com/CambrianTech/continuum/issues/324) | **Parser-per-model-family** | DONE (Rust) | 6 families in Rust (DeepSeek, Llama, Mistral, Hermes, Qwen, Generic) + Native protocol upstream. Closed. |
| [#368](https://github.com/CambrianTech/continuum/issues/368) | **PersonaToolExecutor failures** | DONE (PR #400) | Fixed param serialization, agent loop cap, double correction, loop detection side-effect, tool group bias. |
| [#366](https://github.com/CambrianTech/continuum/issues/366) | **Personas can't reliably write code** | PARTIAL | Sub-issues #367, #368, #371 done. Routing works. Remaining: #370 (e2e pipeline), #369 (quality gate). |
| [#367](https://github.com/CambrianTech/continuum/issues/367) | **CodingAgent dispatch unreliable** | DONE (tested e2e) | Works — 3 workspace strategies, error handling, training capture. Closed. |
| [#321](https://github.com/CambrianTech/continuum/issues/321) | **Local inference quality** | TODO | Compacted 14B gives poor responses. |
| [#325](https://github.com/CambrianTech/continuum/issues/325) | **Ship 14B model, research 32B QAT** | TODO | 14B at Q5_K for MacBook Air. 32B QAT for 32GB machines. |
| [#371](https://github.com/CambrianTech/continuum/issues/371) | **Per-task model routing** | DONE (PR #401) | Fixed hasTools false for XML providers — local personas now upgrade to cloud for tool use. |
| [#343](https://github.com/CambrianTech/continuum/issues/343) | **Native multimodal** | TODO | Skip STT/TTS for models that handle audio/images directly. |
| [#342](https://github.com/CambrianTech/continuum/issues/342) | **Vision feedback** | REOPENED | Pipes exist but full loop (see→fix→verify) not proven. Needs #493 + #480. |
| [#341](https://github.com/CambrianTech/continuum/issues/341) | **API cost budgeting** | PARTIAL (PR #405) | Cost tracking fixed (used wrong provider). `ai/cost` command works. Budget limits still TODO. |
| [#413](https://github.com/CambrianTech/continuum/issues/413) | **Sentinel logs: list available streams** | DONE (PR #421) | Error messages now list available streams. Found by AI team. |
| [#417](https://github.com/CambrianTech/continuum/issues/417) | **Evaluate Qwen3.5-35B-A3B** | TODO | Opus reasoning distilled, 3B active MoE. Could replace Llama-3.2-3B as local model. |

**Done when**: Local model reliably calls tools. Parser handles all model families. Per-task routing picks best model. Cost tracked.

---

## Phase 4: End-to-End Development Orchestration

> From "AI that chats" to "AI that ships code."

| # | Issue | Status | What |
|---|-------|--------|------|
| [#326](https://github.com/CambrianTech/continuum/issues/326) | **E2E dev orchestration** | TODO | Sentinel templates → auto-trigger → PR workflow → chat bridge. |
| [#370](https://github.com/CambrianTech/continuum/issues/370) | **Coding pipeline never proven** | PARTIAL (PR #407) | sentinel/coding-agent works e2e. Persona→chat→code trigger needs proof. |
| [#411](https://github.com/CambrianTech/continuum/issues/411) | **Self-improving system** | TODO | Personas autonomously propose → code → test → PR. The endgame. |
| [#415](https://github.com/CambrianTech/continuum/issues/415) | **Dispatch classifier too trigger-happy** | DONE (PR #419) | Tightened patterns + technical context gate. |
| [#416](https://github.com/CambrianTech/continuum/issues/416) | **sentinel/resume rejects BudgetExhausted** | DONE (PR #420) | Budget exhaustion now sets correct resumable status. |

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
| [#369](https://github.com/CambrianTech/continuum/issues/369) | **RealClassEval trash with local models** | REOPENED | Solved by compaction + training, not API keys. Open until local model passes. |
| [#374](https://github.com/CambrianTech/continuum/issues/374) | **Teacher needs cloud API** | REOPENED | Compacted 35B MoE IS the teacher. Needs #492 first. |
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
| [PR #709](https://github.com/CambrianTech/continuum/pull/709) | **Epistemic grounding** | DESIGN MERGED | 5-tier source hierarchy, EpistemicSource metadata on RAG artifacts, Devil's Advocate persona role, training data filters. Prerequisite for external communication. See [EPISTEMIC-GROUNDING.md](EPISTEMIC-GROUNDING.md). |
| [PR #701](https://github.com/CambrianTech/continuum/pull/701) | **Social & calendar integrations** | DESIGN MERGED | Calendar → Discord → Slack → Newsroom/Email. IntegrationDaemon, command modules, RAG sources. Depends on epistemic grounding. See [SOCIAL-CALENDAR-INTEGRATIONS.md](SOCIAL-CALENDAR-INTEGRATIONS.md). |

**Done when**: Leave the system running overnight → come back to find personas have consolidated memories, audited skills, searched HuggingFace for useful adapters, and initiated peer learning sessions. Personas know your calendar. External communication gated by epistemic verification. Without any human prompt.

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

> Your machines form a single organism. Codename: **Ares** (the Governor).

| # | Issue | Status | What |
|---|-------|--------|------|
| [#323](https://github.com/CambrianTech/continuum/issues/323) | **Tailscale mesh for remote inference** | TODO | Multi-tower transparent command routing. |
| [#364](https://github.com/CambrianTech/continuum/issues/364) | **Cross-node event forwarding** | TODO | Events must propagate across Grid nodes (Rust plumbing). |
| [#349](https://github.com/CambrianTech/continuum/issues/349) | **Reticulum mesh** | TODO | MPC identity + encrypted transport. Replace Tailscale dependency. |
| [#337](https://github.com/CambrianTech/continuum/issues/337) | **Distributed inference + training** | TODO | Shard models and training across towers. |
| [#469](https://github.com/CambrianTech/continuum/issues/469) | **Ares — Grid Governor** | TODO | AI persona on every node. Peer gossip, resource commands, polite mode. Named for Greek god + Tron hero. |
| [#499](https://github.com/CambrianTech/continuum/issues/499) | **Grid discovery + trust** | TODO | Three tiers: on-site, vouched peers, open mesh. No hardcoded IPs. |
| [#501](https://github.com/CambrianTech/continuum/issues/501) | **Grid compute economy** | TODO | Earn credits hosting MoE experts. Route tokens across mesh. |
| [#503](https://github.com/CambrianTech/continuum/issues/503) | **Grid model marketplace** | TODO | Share compacted models + experts + adapters across mesh + HuggingFace. |
| [#505](https://github.com/CambrianTech/continuum/issues/505) | **Command marketplace** | TODO | Share commands as pluggable modules. Generator = SDK. DotNetNuke for AI. |
| [#507](https://github.com/CambrianTech/continuum/issues/507) | **Grid fault tolerance** | TODO | Self-healing organism. Rescue downed nodes. Checkpoint everything. |
| [#508](https://github.com/CambrianTech/continuum/issues/508) | **Multi-agent concurrent coding** | TODO | Worktree isolation + collaborative merge. AIs learn git through experience. |
| [#516](https://github.com/CambrianTech/continuum/issues/516) | **First Grid experiment** | TODO | 5090 + 3090 + 1080 Ti + laptops. Heterogeneous dual-node proof. |
| [#517](https://github.com/CambrianTech/continuum/issues/517) | **Onboarding crisis** ⚠️ CRITICAL | TODO | First external user hit walls. Install must be frictionless. Blocks everything. |

**Available hardware (ready to mesh):**

| Node | GPU | VRAM | RAM | Role | Status |
|------|-----|------|-----|------|--------|
| Joel 5090 tower | RTX 5090 | 32GB | 32GB | Primary forge, heavy training | Online (WSL2) |
| Joel 1080Ti box | 3x GTX 1080Ti | 33GB total | 128GB | Distributed inference, CPU pruning, GGUF conversion | **OFFLINE — blocked on install.sh** |
| Joel 970 box | GTX 970 | 4GB | ? | Light inference, testing | **OFFLINE** |
| Joel MacBook Pro | M1 Pro | 32GB unified | 32GB | MLX inference, testing, dev | Online |
| Joel MacBook Air | M1 | 8GB unified | 8GB | iPhone-class testing (same RAM budget) | Available |
| Toby 3090 | RTX 3090 | 24GB | ? | Secondary forge, inference | **OFFLINE — blocked on install.sh** (PR #535) |
| Toby 5050 | RTX 5050 | 8GB | ? | Light inference, edge testing | **OFFLINE** |

**The 1080Ti box alone unblocks**: parallel GGUF conversion (128GB RAM), distributed inference (3 GPUs), CPU expert pruning without blocking the 5090 forge. Getting `install.sh` working is THE grid priority.

| [#798](https://github.com/CambrianTech/continuum/issues/798) | **Route inference through grid to GPU nodes** | TODO | When BigMama online, route `ai/generate`, STT, TTS to 5090 instead of laptop. Grid router exists, needs wiring to AI provider. |
| [#806](https://github.com/CambrianTech/continuum/issues/806) | **Tailscale ghost nodes on restart** | DONE (PR #809) | State volume persists identity. `TS_HOSTNAME` defaults to `{hostname}-grid`. No more orphaned devices. |
| [#807](https://github.com/CambrianTech/continuum/issues/807) | **Auto grid profile when Tailscale configured** | TODO | `setup.sh` detects Tailscale → enables grid automatically. No manual `.env.grid` copy or `--profile grid`. |
| [#808](https://github.com/CambrianTech/continuum/issues/808) | **Grid config provisioning** ⚠️ HIGH | TODO | `grid/provision` syncs config.env from primary node. No manual `scp`. One Tailscale key is the only manual step. |
| [#811](https://github.com/CambrianTech/continuum/issues/811) | **Docker node shows 127.0.0.1 / no GPU** | PR #813 | Grid Overview fetches grid/status for real Tailscale IP and GPU capabilities. |
| [#814](https://github.com/CambrianTech/continuum/issues/814) | **Self-healing — auto-wake and restart downed nodes** | TODO | Foreman detects offline → WoL via Tailscale → SSH restart. Grid is the immune system. |
| [#815](https://github.com/CambrianTech/continuum/issues/815) | **In-browser terminal for node management** | TODO | AWS-style console. SSH button → terminal widget → Tailscale IP. Wake/restart/rebuild/logs from grid page. |

**Done when**: `install.sh` works on the 1080Ti box and Toby's 3090. Grid ping succeeds across Tailscale. A training job started on the 5090 checkpoints and resumes on the 3090 when the 5090 reboots. Ares detects a game launching and yields GPU. GGUF conversion runs on the 1080Ti box while 5090 forges. Inference routes to BigMama when laptop is on Tailscale. Config propagates automatically to new nodes via `grid/provision`. Downed nodes auto-revive. Full node management from browser.

---

## Phase 11: Docker — Full-Stack Containerization (PR #740)

> `docker compose up` — Tailscale handles TLS, containers serve HTTP. Real HTTPS, no warnings.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#737](https://github.com/CambrianTech/continuum/issues/737) | **Docker architecture** | WORKING | docker-compose.yml: tailscale, postgres, continuum-core, node-server, widget-server, livekit, model-init, forge-worker, inference. All containers healthy on BigMama. |
| — | **Tailscale sidecar TLS** | DONE | Tailscale container joins tailnet, provisions Let's Encrypt certs, reverse-proxies HTTPS/WSS to plain HTTP containers via TS_SERVE_CONFIG. No Caddy, no self-signed, no manual certs. Two prereqs: enable HTTPS certs in Tailscale DNS settings + generate auth key. |
| — | **ONNX Runtime in Docker** | DONE | ONNX Runtime 1.24.4 installed in continuum-core image. ORT_DYLIB_PATH env var set. Silero VAD + Piper TTS work (persona hearing + speech). |
| — | **Postgres in Docker** | DONE | SecretManager no longer overwrites Docker env vars with config.env values. DATABASE_URL from compose takes precedence. |
| — | **WS localhost fallback bug** | DONE | TransportConfig.ts used `ws://localhost` for non-HTTPS pages. Now always uses `window.location.hostname` in browser. Vite bundle rebuilt. |
| — | **IPC crash without Rust core** | DONE (PR #740) | Node-server no longer crashes if continuum-core socket missing. |
| — | **Auto-seed on first run** | PARTIAL | docker-entrypoint.ts detects empty DB, runs seed-continuum.ts. Rooms seed (11/12). Personas fail (IPC drops under heavy seeding). Needs resilient seeding with retry. |
| — | **ARM64 Docker: WebRTC** | DEFERRED | LiveKit runs as separate container. Rust binary built without livekit-webrtc feature (`--no-default-features`). |
| — | **Persona seeding in Docker** | TODO | AI users not created. Seed script IPC connections fail under heavy load. Need: (a) batch seeding with delays between records, or (b) direct SQL seed for Docker. |
| — | **Voice/avatar models** | TODO | model-init container exists but voice-models volume not populated on BigMama. Need `docker compose run model-init`. |
| — | **CI multi-arch images** | TODO | GHCR publishing workflow exists but not tested on this branch. |
| — | **WSS port routing** | DONE (PR #809) | Browser WebSocket now connects to configured WS_PORT (9001), not page port (443). Fixes Tailscale reverse proxy. |
| — | **Port conflict Tailscale vs node-server** | DONE (PR #809) | Removed duplicate 9002:9001 host mapping from Tailscale. Tailscale serve proxies internally. |
| — | **GHCR images rebuilt** | DONE | All 5 images rebuilt on BigMama and pushed to GHCR (2026-04-06). |
| [#796](https://github.com/CambrianTech/continuum/issues/796) | **Docker E2E with live mode + grid** | PARTIAL | Chat works, AIs respond, HTTPS via Tailscale works, factory shows leaderboard. Remaining: live calls, grid discovery from browser. |

**Prereqs** (one-time, per tailnet):
1. Tailscale installed + HTTPS certificates enabled in DNS settings
2. Auth key generated (reusable + ephemeral) → stored in `.env` as `TS_AUTHKEY`

**Done when**: `docker compose up` on a fresh machine with Tailscale brings up the full system with all personas, avatars, and voice models. Accessible at `https://<hostname>.ts.net`.

---

## Phase 12: Factory — Model Forge Production Line

> Nature: forge base models. Nurture: academy trains personas. Factory is nature. The factory is the product's front door — the widget that brings people in and the grid that keeps them.

The factory forges, benchmarks, and publishes base models for every device tier. HuggingFace is the app store — we provide the factory, community provides hardware. Models forged through our pipeline have known provenance enabling re-forging (the moat). Recipes are shareable end-to-end templates that encode the entire forge process.

**Strategy**: HF leaderboards for benchmarks (don't reinvent). Right-panel sidebar for our leaderboard/stats. Competitive spirit drives adoption. Recipes are the apps, factory is the store, grid is the compute.

### Core Factory Infrastructure

| # | Issue | Status | What |
|---|-------|--------|------|
| [#576](https://github.com/CambrianTech/continuum/issues/576) | **Factory widget** | IN PROGRESS | Event-driven widget with forge controls, live HF models, leaderboard-style published models. PR #644 (pruning controls), PR #645 (header tab), PR #654 (forge command + live HF data). |
| [#653](https://github.com/CambrianTech/continuum/issues/653) | **Wire START FORGE + live status + queue** | PR #654 | model/forge command routes to BigMama via SSH/grid. Status polling emits events. Queue UX needed. |
| [#638](https://github.com/CambrianTech/continuum/issues/638) | **Factory job queue** | TODO | RTOS-style task scheduling across grid nodes. Priority, estimated wait, queue position. |
| [#646](https://github.com/CambrianTech/continuum/issues/646) | **Python↔Rust bridge** | TODO | Protobuf schema for forge events (like ts-rs for Rust↔TS). |
| [#629](https://github.com/CambrianTech/continuum/issues/629) | **Mixed-precision GGUF** | TODO | Validate end-to-end, make it the default forge output. |
| [#577](https://github.com/CambrianTech/continuum/issues/577) | **Architecture visualizer** | DESIGNED | Shared component for model surgery + cognition visualization. Canvas/WebGL. |
| [#584](https://github.com/CambrianTech/continuum/issues/584) | **Custom prompt testing** | TODO | Run any prompt against forged model from the widget. |
| [#583](https://github.com/CambrianTech/continuum/issues/583) | **Test results viewer** | TODO | Log-style pass/fail with click-to-expand. |

### Recipe System (The Apps)

| # | Issue | Status | What |
|---|-------|--------|------|
| [#651](https://github.com/CambrianTech/continuum/issues/651) | **Recipe composition** | TODO | Stack multiple recipes on one base model. Sequential forge stages. |
| [#648](https://github.com/CambrianTech/continuum/issues/648) | **Context window extension** | TODO | RoPE rescaling recipe. YaRN/NTK + long-context fine-tuning. |
| [#649](https://github.com/CambrianTech/continuum/issues/649) | **Vision encoder (LLaVA-style)** | TODO | Bolt-on vision via projection layer training. |
| [#650](https://github.com/CambrianTech/continuum/issues/650) | **Audio encoder (Whisper-style)** | TODO | Hearing + speech natively. |
| [#578](https://github.com/CambrianTech/continuum/issues/578) | **Voice model forging** | TODO | Prune unused phoneme heads, specialize for accent/language. |
| [#579](https://github.com/CambrianTech/continuum/issues/579) | **Vision model forging** | TODO | Feature detector pruning, domain specialization. |
| [#580](https://github.com/CambrianTech/continuum/issues/580) | **Expert-as-a-service** | TODO | Dynamic MoE paging across grid. Hot experts local, cold experts from mesh. |

### Lifecycle Pipeline (Factory → Academy → Sentinel)

| # | Issue | Status | What |
|---|-------|--------|------|
| [#655](https://github.com/CambrianTech/continuum/issues/655) | **End-to-end lifecycle** | MASTER ISSUE | Forge → Evaluate → Deploy → Learn → Re-forge. The full loop. |
| [#656](https://github.com/CambrianTech/continuum/issues/656) | **Auto-submit to HF leaderboards** | TODO | After forge completes, submit to Open LLM, domain-specific boards. Pull results back. |
| [#657](https://github.com/CambrianTech/continuum/issues/657) | **Re-forge from existing model** | TODO | THE MOAT. Known provenance enables deeper controls: swap adapters, adjust pruning, add modalities. |
| [#658](https://github.com/CambrianTech/continuum/issues/658) | **Sentinel forge recipe** | TODO | Automated lifecycle: forge → evaluate → deploy → learn → re-forge. AI foreman orchestrates. |
| [#652](https://github.com/CambrianTech/continuum/issues/652) | **Low-latency sensory pipeline** | TODO | Sub-100ms vision + real-time audio for personas. Inference speed, not training. |

### ForgeAlloy — Portable Pipeline Format & Integrity

| # | Issue | Status | What |
|---|-------|--------|------|
| [#659](https://github.com/CambrianTech/continuum/issues/659) | **ForgeAlloy portable entity** | DONE | Public repo (CambrianTech/forge-alloy). Rust + Python + TypeScript. JSON schema. 7 tests. |
| [#660](https://github.com/CambrianTech/continuum/issues/660) | **Factory widget: import/export alloys** | TODO | Load/save .alloy.json recipes. Display executed alloy results. |
| [#661](https://github.com/CambrianTech/continuum/issues/661) | **Attestation verification in model/list-published** | TODO | Fetch .alloy.json from HF, display trust level and benchmarks. |
| [fa #1](https://github.com/CambrianTech/forge-alloy/issues/1) | **JCS canonicalization + ES256 signing** | TODO | RFC 8785 implementation. verify_signature() in all three languages. Blocks all signed attestation. |
| [fa #2](https://github.com/CambrianTech/forge-alloy/issues/2) | **Key registry** | TODO | Hosted service with revocation, rotation, supersededBy. |
| [fa #3](https://github.com/CambrianTech/forge-alloy/issues/3) | **Hardware key signing** | TODO | Secure Enclave (macOS), StrongBox (Android), TPM (Windows). Phase 2. |
| [fa #4](https://github.com/CambrianTech/forge-alloy/issues/4) | **Enclave execution** | TODO | TEE for tamper-proof attestation. Required for marketplace payments. Phase 4. |
| [fa #5](https://github.com/CambrianTech/forge-alloy/issues/5) | **Dataset hashing** | TODO | RFC 6962 Merkle tree with domain separation. All three languages. |
| [fa #6](https://github.com/CambrianTech/forge-alloy/issues/6) | **Post-quantum migration** | FUTURE | ML-DSA / SLH-DSA dual-signing. Enum ready, waiting on library maturity. |
| [s-ai #118](https://github.com/CambrianTech/sentinel-ai/issues/118) | **Full alloy results in forge** | TODO | Populate benchmarks, hardware profiles, dataset hashes after forging. |

**Current state**: ForgeAlloy repo live with 13 stage types (SourceConfig, Prune, Train, LoRA, Compact, Quant, Package, Eval, Publish, Deploy, ExpertPrune, ContextExtend, Modality). Peer-reviewed attestation (WebAuthn-modeled, PQC ready). alloy_executor.py with OOP stage package on sentinel-ai. Factory widget decomposed into 5 components with visual pipeline composer (6 stage UI elements built). First production alloy forged: qwen3.5-4b-code-forged +16.4%.

### Stage Executors (sentinel-ai)

| # | Issue | Status | What |
|---|-------|--------|------|
| [s-ai #119](https://github.com/CambrianTech/sentinel-ai/issues/119) | **Source-config executor** | DONE | Context window, modalities, target devices. |
| [s-ai #120](https://github.com/CambrianTech/sentinel-ai/issues/120) | **Modality executor** | STUB | Vision/audio/video encoder bolt-on. Auto-recommends encoders + datasets. |
| [s-ai #121](https://github.com/CambrianTech/sentinel-ai/issues/121) | **Package executor** | STUB | CoreML, TensorRT, ONNX device packaging. |
| [s-ai #122](https://github.com/CambrianTech/sentinel-ai/issues/122) | **Deploy executor** | STUB | Grid node deployment, health check, warmup. |
| [s-ai #123](https://github.com/CambrianTech/sentinel-ai/issues/123) | **LoRA executor** | TODO | Distinct from train — QLoRA, rank/alpha, merge after. |
| [s-ai #124](https://github.com/CambrianTech/sentinel-ai/issues/124) | **Compact executor** | TODO | Plasticity-based mixed-precision. Our moat. |
| [s-ai #125](https://github.com/CambrianTech/sentinel-ai/issues/125) | **Benchmark harness** | TODO | Actually run HumanEval, MMLU, GSM8K via evalplus/lm-eval. |
| [s-ai #126](https://github.com/CambrianTech/sentinel-ai/issues/126) | **Context-extend training** | TODO | YaRN/NTK with long-context training data. |

### Stage UI Elements (continuum)

| # | Issue | Status | What |
|---|-------|--------|------|
| [#665](https://github.com/CambrianTech/continuum/issues/665) | **Remaining stage UIs** | TODO | 7 more: LoRA, Compact, Publish, Package, ContextExtend, Modality, ExpertPrune. |
| [#666](https://github.com/CambrianTech/continuum/issues/666) | **Pipeline → executor integration** | TODO | Send full pipeline (all stages) to forge node, not just prune+train. |
| [#667](https://github.com/CambrianTech/continuum/issues/667) | **Grid capacity query** | TODO | Factory widget shows available nodes + capabilities before forging. |

### Benchmarking & Distribution

| # | Issue | Status | What |
|---|-------|--------|------|
| [s-ai #108](https://github.com/CambrianTech/sentinel-ai/issues/108) | **Device ladder** | IN PROGRESS | 64/32/16 expert variants for RTX 3090 → MacBook Air → iPhone. |
| [s-ai #109](https://github.com/CambrianTech/sentinel-ai/issues/109) | **Production pipeline** | COMMITTED | forge → test → GGUF → test → card → publish. Gated, idempotent. |
| [s-ai #110](https://github.com/CambrianTech/sentinel-ai/issues/110) | **Benchmark validation** | IN PROGRESS | HumanEval+ running. 4B code-forged at 74.4% on first 78/164 problems. |
| [s-ai #111-114](https://github.com/CambrianTech/sentinel-ai/issues/111) | **Leaderboard submissions** | TODO | Open LLM v2, HumanEval+, Intel Low-Bit, LiveCodeBench. Use HF's existing infrastructure. |

**Published models (11 on HuggingFace, 14,967 total downloads):**

| Model | Downloads | HumanEval | Status |
|-------|-----------|-----------|--------|
| qwen3.5-35b-a3b-compacted | 2,426 | TBD | Published, GGUF Q2_K/Q4_K_M available |
| qwen2.5-coder-14b-compacted | 2,052 | TBD | Published |
| qwen2.5-coder-32b-compacted | 1,937 | TBD | Published |
| qwen3.5-27b-code-forged | 1,731 | TBD | Published, MLX 4-bit available |
| qwen3.5-4b-code-forged | 1,300 | **74.4% (partial)** | Published, GGUF available |
| qwen3.5-27b-code-forged-defragged | 826 | TBD | Published, structurally pruned |
| qwen3.5-4b-code-forged-defragged | 726 | TBD | Published |
| + 4 more Qwen2.5 models | ~2,000 | TBD | Published |

**The full pipeline:**
```
Factory (forge) → HF (publish + leaderboard) → Grid (deploy) → Academy (learn) → Re-forge (improve)
    ↑                                                                                    |
    └────────────────────────── continuous improvement loop ──────────────────────────────┘
```

**Done when**: Factory widget is visually stunning. START FORGE runs from the widget, benchmarks via HF leaderboards, publishes with scores, re-forging offers deeper controls for Continuum-forged models. Sentinels automate the full lifecycle. Community contributes GPU via grid, shares recipes, models appear on public leaderboards alongside GPT/Claude/Gemini.

---

## Issue Map — Every Open Issue, One Phase

| Phase | Issues | Count |
|-------|--------|-------|
| **0: Critical Bugs** | ~~#376~~, ~~#335~~, ~~#317~~, ~~#385~~, ~~#381~~, ~~#373~~ | 6 (ALL DONE) |
| **1: Arch Integrity** | ~~#333~~, ~~#363~~, #362, ~~#356~~, ~~#355~~, #353, #351, ~~#361~~, ~~#354~~, ~~#352~~, ~~#379~~, ~~#334~~, ~~#360~~, ~~#412~~ | 14 (11 done) |
| **2: Live Quality** | #331 ⚠️, ~~#338~~, #339, ~~#340~~, ~~#318~~, #322 ⚠️, ~~#332~~, ~~#380~~, ~~#399~~, #409, ~~#436~~, ~~#464~~, ~~#465~~, #473 | 14 (9 done, 2 CRITICAL) |
| **3: Tool Calling** | ~~#324~~, ~~#368~~, ~~#366~~, ~~#367~~, ~~#321~~, ~~#325~~, ~~#371~~, ~~#343~~, #342, ~~#341~~, ~~#413~~, #417, ~~#430~~, #433, #439, ~~#440~~, ~~#453~~ | 17 (12 done, 2 reopened) |
| **4: Dev Orchestration** | ~~#326~~, ~~#370~~, ~~#411~~ ✅, ~~#415~~, ~~#416~~, #445 | 6 (5 done) |
| **5: Academy** | #377, #369, #374, ~~#365~~, #344, ~~#345~~, #384, ~~#359~~ | 8 (3 done, 2 reopened) |
| **6: Genome** | #382, #378, ~~#330~~, ~~#319~~, ~~#472~~ | 5 (3 done) |
| **7: Autonomous** | #383, ~~#329~~, ~~#336~~ | 3 (2 done) |
| **8: Distillation** | ~~#327~~, ~~#357~~ | 2 (2 done) |
| **9: Codebase Intel** | ~~#328~~ | 1 (1 done) |
| **10: Grid** | ~~#323~~, ~~#364~~, #349, #337, ~~#467~~, #469 (Ares), #499, #501, #503, #505, #507, #508, #516, #517 ⚠️ | 14 (3 done, 1 CRITICAL) |
| **11: Multimodal Compaction** | #492, #417, #480, ~~#493~~, #494, #495, #496, #497, #409, #502 | 10 (1 done — THE UNLOCK) |
| **12: Factory** | #576-584, #629, #638, #646, #648-667 + s-ai #108-126 + fa #1-6 | 52 (4 in progress, #659 done, first alloy forged) |
| **Research** | #391, #392, ~~#393~~ | 3 (1 done) |
| **Total** | | **131 tracked, 57 open, 74 closed** |

---

## Phase 11: Multimodal Compaction — The Unlock

> Personas that SEE what they build. On a MacBook. With zero API keys.

This phase combines plasticity compaction, MoE paging, vision, and Academy training into the system's defining capability: AI teammates that can design, build, and visually verify their own work on consumer hardware.

| # | Issue | Status | What |
|---|-------|--------|------|
| [#492](https://github.com/CambrianTech/continuum/issues/492) | **Compact Qwen3.5-35B-A3B on 5090** | TODO | Run plasticity pipeline on MoE model. Target: 8-12GB (MacBook Air). |
| [#417](https://github.com/CambrianTech/continuum/issues/417) | **Evaluate compacted model** | REOPENED | Was closed as "too big" — never tried compaction. 3x proven on 14B. |
| [#480](https://github.com/CambrianTech/continuum/issues/480) | **Qwen3.5-0.8B vision service** | TODO | Lightweight real-time scene captioning for text-only models. |
| [#493](https://github.com/CambrianTech/continuum/issues/493) | **DOM interaction command** | TODO | click/type/select — personas interact with UI elements. |
| [#494](https://github.com/CambrianTech/continuum/issues/494) | **UI design training curriculum** | TODO | Academy teaches personas to see screenshots, find problems, fix code. |
| [#495](https://github.com/CambrianTech/continuum/issues/495) | **HuggingFace naming + publishing** | TODO | `-cont` suffix, model cards, publishing pipeline. |
| [#496](https://github.com/CambrianTech/continuum/issues/496) | **Integration test: persona redesigns widget** | TODO | THE proof — zero API keys, local model, full visual loop. |
| [#497](https://github.com/CambrianTech/continuum/issues/497) | **Compaction + MoE paging combined** | TODO | Any model on any hardware: compact what fits, page the rest from HF. |
| [#409](https://github.com/CambrianTech/continuum/issues/409) | **Total sensory verification** | REOPENED | Vision + hearing + speech all working locally with Qwen VL. Zero API keys. |
| [#502](https://github.com/CambrianTech/continuum/issues/502) | **Training signal capture** | TODO | Every live session (especially bugs) becomes Academy training data. |
| [#503](https://github.com/CambrianTech/continuum/issues/503) | **Grid model marketplace** | TODO | Share compacted models + individual experts across the mesh. |
| [#501](https://github.com/CambrianTech/continuum/issues/501) | **Grid compute economy** | TODO | Earn credits by hosting MoE experts. Route tokens across mesh. |
| [#499](https://github.com/CambrianTech/continuum/issues/499) | **Grid discovery + trust** | TODO | Three tiers: on-site, vouched peers, open mesh. Economy comes last. |

**The dependency chain:**
```
#492 (compact model) → #417 (evaluate) → #495 (publish to HF)
    → #374 (local teacher) → #377 (Academy fully local)
    → #369 (local code quality) → #494 (UI design curriculum)
    → #496 (THE PROOF: persona redesigns widget with zero API keys)

#493 (DOM interaction) + #480 (vision) + #342 (feedback loop)
    → #496 (the proof)

#497 (compaction + paging) → #433 + #439 (MoE paging/surgery)
    → ANY model on ANY hardware
```

**Done when**: A persona on a MacBook Air with zero API keys receives "make the chat input rounded," takes a screenshot, edits the CSS, rebuilds, takes another screenshot, and confirms the fix. All inference local. Model published to HuggingFace.

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

**Phase 10** distributes everything across a mesh of commodity hardware. **Ares** — the Grid Governor — commands resources, detects when users need their machines, and keeps the mesh alive as nodes come and go. First experiment: 5090 + 3090 + 1080 Ti. The Cell architecture realized.

**Phase 11** is THE unlock — plasticity compaction + MoE paging + vision + Academy training = personas that SEE and BUILD their own UI, on a MacBook, with zero API keys. Every download of a compacted model. Every upload of a trained adapter to HuggingFace. Every persona that designs a widget, trains a model, improves itself. The flywheel.

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
