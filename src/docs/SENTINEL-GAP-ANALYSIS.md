# Sentinel System: Gap Analysis vs. The Field

> **Date**: 2026-02-28
> **Branch**: `feature/sentinel-claude-code`
> **Purpose**: Identify what we have, what we lack, and what we should build next by comparing our sentinel system against 10 competing agentic coding tools and current distillation research.

## Executive Summary

Our sentinel system is architecturally **more ambitious** than any single competitor — we combine pipeline orchestration, LoRA training, multi-agent coordination, and persona cognition in one system. But the field has leapfrogged us in several critical areas: **context management**, **codebase understanding**, **developer UX**, and **production multi-agent execution**. Our unique advantage — the LoRA distillation pipeline — exists in prototype but needs hardening.

The strategic play: **don't compete on agent UX** (Claude Code, Cursor already won that). Instead, **use external agents as teachers** and distill their expertise into our personas via LoRA. Sentinels orchestrate this entire lifecycle.

---

## What We Have (Strengths)

### 1. Pipeline Composition Engine (Rust) — Unique
10 step types (Shell, LLM, Command, Condition, Loop, Parallel, Emit, Watch, Sentinel, CodingAgent) with 103 tests. No competitor has anything close to this. Claude Code has subagents but they're flat — no loops, conditions, parallel branches, or inter-agent events. Our pipelines are **JSON-serializable data** that personas can create, save, share, and modify.

### 2. LoRA Training Pipeline — Unique
End-to-end proven: train (PEFT) → discover (AdapterStore) → load (Candle) → merge → inference. No competitor does any form of learning or adaptation beyond configuration files. This is our moat.

### 3. Academy Dual-Sentinel Architecture — Unique
Teacher synthesizes training data, student trains and gets examined. No competitor has anything like autonomous curriculum design + examination + LoRA training in one orchestrated system.

### 4. Training Data Capture from Coding Agents — Unique
`SentinelCodingAgentServerCommand.captureTrainingData()` already extracts user→assistant interaction pairs from coding agent sessions and feeds them to `GenomeCaptureInteraction.execute()` with quality scores (0.9 success, 0.3 failure). This is the foundation for distillation.

### 5. Persona Ownership & Escalation — Unique
Every sentinel has `parentPersonaId`. Results flow to the persona's inbox via `SentinelEscalationService`. Execution history persists as memory. No competitor ties agent results to a persistent identity with memory.

### 6. Event-Based Inter-Agent Communication — Unique
`Emit`/`Watch` steps enable multi-sentinel coordination (teacher↔student). Cursor has parallel agents but they don't coordinate — they work independently on separate files.

---

## What We Lack (Gaps)

### GAP 1: Codebase Understanding (Critical)

**The field:**
- **Aider**: PersonalizedPageRank on tree-sitter dependency graph. Builds a `NetworkX MultiDiGraph` of file relationships, ranks using PageRank personalized to the active chat files. Compresses entire codebase structure into a token-budget-constrained repo map.
- **Cursor**: Custom embedding model indexes entire codebase into Turbopuffer vector DB. Sub-100ms lookup after initial indexing.
- **Sweep**: CST (Concrete Syntax Tree) entity extraction. Processes 2M+ files/day. Prunes each file to only the entities needed.
- **OpenCode**: Native LSP integration for 20+ languages. Diagnostics as a first-class tool.

**Our system:** No codebase indexing. No repo map. No tree-sitter. No LSP. When a sentinel runs a CodingAgent step, the agent (Claude Code) does its own codebase exploration, but our system doesn't benefit from it. Each sentinel invocation starts blind.

**Impact:** Our personas can't reason about code structure. They can't say "this change affects these 5 files" without re-exploring every time. The Academy teacher can't automatically identify the right source files for curriculum design.

**Recommendation:** Build a `CodebaseIndex` service (Rust worker) that:
- Uses tree-sitter to extract symbols from all source files
- Builds a dependency graph (imports, function calls, type references)
- Exposes via a sentinel Command step: `codebase/symbols`, `codebase/dependencies`, `codebase/search-semantic`
- Incrementally updates on file changes (watch filesystem)
- This is the `fastembed` + `ort` infrastructure we already have — wire it up

### GAP 2: Context Management (Critical)

**The field:**
- **GSD**: Explicitly solves "context rot" — quality degrades as context fills. Forces work into small specs, each running in a fresh 200k context window. Atomic git commits per task.
- **Cline**: Memory Bank (persistent project knowledge), Focus Chain (auto-generated todo list preventing drift), Auto-Compact (summarizes at capacity), .clinerules (declarative context management rules).
- **Claude Code**: Auto-compaction at 95% capacity. CLAUDE.md for persistent instructions. Session forking for exploration.
- **Codex**: Progressive skill disclosure — loads metadata first, full content only when needed.

**Our system:** No context management for sentinel LLM steps. An LLM step gets whatever prompt we give it — no awareness of codebase structure, no persistent memory across pipeline iterations, no progressive disclosure. Long-running pipelines (Academy sessions can last hours) will hit context limits.

**Impact:** Academy teacher LLM steps that analyze code, design curriculum, and generate training data are all limited to whatever we manually stuff into the prompt. No automatic context enrichment.

**Recommendation:**
- Add a `contextSources` field to LLM steps that auto-fetches codebase context
- Integrate the CodebaseIndex from GAP 1 so LLM steps can reference `{{codebase.symbols.relevant}}` or `{{codebase.dependencies.for_file}}`
- For long pipelines, implement step-result summarization to keep context fresh
- RAG integration for LLM steps — we already have the RAG pipeline, just wire it to sentinel LLM steps

### GAP 3: Multi-Agent Isolation & Parallelism (Important)

**The field:**
- **Cursor**: Up to 8 agents simultaneously in **git worktrees**. Each gets an isolated copy of the repo. Background agents run in **cloud VMs** — truly asynchronous. 35% of Cursor's PRs are agent-authored.
- **Codex**: OS-level **Landlock + seccomp** sandboxing. Network disabled during execution. Sub-agents inherit sandbox policy.
- **OpenHands**: Docker-sandboxed execution with bash + browser + IPython. Hierarchical agent delegation via AgentHub registry.

**Our system:** `maxConcurrentSentinels = 4` in Rust, but no isolation between them. No sandboxing. No worktree isolation. No network restrictions. CodingAgent steps run in the host environment — a malicious or buggy agent could damage the workspace.

**Impact:** We can't safely run multiple coding agents in parallel on the same codebase. We can't run untrusted pipelines. We can't scale beyond one machine.

**Recommendation:**
- **Phase 1**: Git worktree isolation for CodingAgent steps (create worktree → run agent → merge back). This is what Cursor does.
- **Phase 2**: Docker container isolation for shell/coding-agent steps. This is what SWE-agent and OpenHands do.
- **Phase 3**: Remote execution — sentinels that run on different machines (the P2P mesh concept).

### GAP 4: Agent UX & Developer Experience (Important)

**The field:**
- **Claude Code**: Hooks (PreToolUse, PostToolUse), CLAUDE.md, auto-memory, session forking, ToolSearch meta-tool
- **OpenCode**: LSP integration, SSE events for multi-client sync, Tauri desktop + TUI
- **Cline**: Plan/Act mode separation, Focus Chain, checkpoint system, Memory Bank

**Our system:** `./jtag sentinel/run` returns a handle. `./jtag sentinel/status --handle=xxx` polls. `./jtag sentinel/logs/tail --handle=xxx` reads logs. Functional but spartan. No real-time streaming to the UI. No planning mode. No interactive approval during execution.

**Impact:** Developers (including our AI personas) can't easily watch sentinel progress, intervene mid-execution, or adjust course. The SentinelEventBridge polls at 1s intervals but the UI doesn't consume these events well.

**Recommendation:**
- Wire SentinelEventBridge events to the chat widget (sentinels report progress as chat messages)
- Add a `sentinel-monitor` widget that shows live pipeline execution (step by step, with outputs)
- Add interactive approval steps: a new `Approve` step type that pauses and waits for human/persona approval before proceeding

### GAP 5: Quality Scoring & Evaluation (Important)

**The field:**
- **NVIDIA Data Flywheel**: Run teacher → capture traces → filter by quality → train student → evaluate → promote if quality meets threshold → repeat
- **Agent-FLAN**: Decomposed training data into capability categories + negative samples to reduce hallucination
- **LoRA Soups / LoRAtorio**: Optimal adapter merging with weighted composition

**Our system:** Binary quality scoring (0.9 success, 0.3 failure) in `captureTrainingData()`. No evaluation after training. No adapter benchmarking. No negative examples. No composite quality metrics.

**Impact:** We're training on poorly-scored data and never validating that the trained adapter actually improved. The flywheel can't spin if we can't measure progress.

**Recommendation:**
- Implement composite quality scoring:
  ```
  TraceQualityScore {
    outcome: 0-1      // did it succeed?
    correctness: 0-1   // does code compile/pass tests?
    efficiency: 0-1    // steps vs optimal
    complexity: 0-1    // task difficulty
    novelty: 0-1       // different from existing data
    composite() → weighted sum
  }
  ```
- Add a `BenchmarkSentinel` that tests adapters after training on held-out tasks
- Auto-rollback if new adapter performs worse than previous version
- Include negative examples (failed traces with corrections) in training data

### GAP 6: Multi-Provider Agent Support (Medium)

**The field:**
- **Aider**: Works with literally any model. No tool-use required — uses edit formats parsed from text.
- **OpenCode**: 75+ LLM providers through AI SDK
- **Cline**: Multi-model with per-task model selection

**Our system:** CodingAgentRegistry has only `ClaudeCodeProvider`. The interface supports multiple providers but only one is implemented.

**Impact:** We can't distill from multiple teacher agents. Multi-teacher distillation research shows that diverse teachers produce more robust students.

**Recommendation:**
- Implement `CodexProvider` (OpenAI Codex CLI — 96% Rust, has an SDK)
- Implement `AiderProvider` (Python, subprocess-based)
- Implement `OpenCodeProvider` (TypeScript/Bun, has SDK)
- Each provider captures interactions in the same `CodingAgentInteraction` format
- Multi-teacher training pipeline merges traces from all providers

### GAP 7: Persona-Sentinel Integration Depth (Medium)

**The field:** N/A — no competitor has personas. This is purely about our own integration depth.

**Our current state:** Sentinels are **adjacent** to personas, not **part of** them:
- PersonaUser receives `InboxTask` from sentinel escalation (reactive)
- PersonaUser can dispatch sentinels via tool calls (manual)
- No automatic sentinel creation based on persona cognition
- No sentinel memories feeding back into persona RAG context
- Personas don't create their own sentinels autonomously

**The user's vision:** "personas using sentinels as part of their own being, like any command, for anything"

**Impact:** Sentinels feel like external tools personas invoke, not integrated capabilities. A persona should be able to think "I need to learn TypeScript testing" and autonomously spawn an Academy session, or think "this code needs reviewing" and spawn a review sentinel, without explicit human instruction.

**Recommendation:**
- Add sentinel dispatch to PersonaUser's autonomous task generation (`generateSelfTasks()`)
- Sentinel execution memories should be injected into persona RAG context
- Personas should be able to create pipeline definitions from natural language (LLM step → JSON pipeline)
- Sentinel templates stored per-persona in their longterm.db

---

## What We Should Build (Prioritized Roadmap)

### Phase 1: Distillation Pipeline Hardening (Immediate)

This is our unique advantage — harden it before the field catches up.

| Item | Description | Existing Foundation |
|------|-------------|-------------------|
| Composite quality scoring | Replace binary 0.9/0.3 with multi-dimensional score | `captureTrainingData()` |
| Tool-call capture in traces | Include tool names, args, results in training data | `CodingAgentInteraction.toolCalls` |
| Replay buffer | Mix 20% historical best traces with new data | New |
| Evaluation sentinel | Benchmark adapter after training on held-out tasks | `BenchmarkPipeline.ts` exists |
| Auto-rollback | Revert adapter if evaluation fails | `AdapterStore` versioning |

### Phase 2: Codebase Understanding (Next)

| Item | Description | Existing Foundation |
|------|-------------|-------------------|
| Tree-sitter symbol extraction | Parse all source files for functions, classes, types | `fastembed` + `ort` already in Rust deps |
| Dependency graph | Build import/call graph across files | New |
| Sentinel context enrichment | LLM steps auto-receive relevant codebase context | `ragSources` field exists on `PipelineSentinelDefinition` |
| Incremental indexing | Watch filesystem, update index on changes | Rust `notify` crate |

### Phase 3: Multi-Provider Distillation (Then)

| Item | Description | Existing Foundation |
|------|-------------|-------------------|
| CodexProvider | OpenAI Codex as teacher agent | `CodingAgentProvider` interface |
| AiderProvider | Aider as teacher agent | `CodingAgentProvider` interface |
| Multi-teacher training | Merge traces from all providers | `genome/train` pipeline |
| Domain routing | Route traces to domain-specific adapters | `classifyTraceDomain()` |
| Curriculum progression | Progressive difficulty gating | Academy architecture |

### Phase 4: Persona-Sentinel Deep Integration (Then)

| Item | Description | Existing Foundation |
|------|-------------|-------------------|
| Autonomous sentinel dispatch | Personas create sentinels from cognition | `generateSelfTasks()` in PersonaUser |
| Sentinel memory → RAG | Execution results feed persona context | `SentinelEscalationService` → Memory |
| Natural language pipelines | Persona describes pipeline → LLM generates JSON | LLM step + Pipeline types |
| Per-persona templates | Persona's own sentinel library | `SentinelEntity.parentPersonaId` |

### Phase 5: Isolation & Scale (Later)

| Item | Description | Existing Foundation |
|------|-------------|-------------------|
| Git worktree isolation | CodingAgent steps run in worktrees | Git integration |
| Docker sandboxing | Shell steps run in containers | New |
| Remote sentinel execution | Sentinels on different machines | P2P mesh concept |
| Cloud agent support | Background sentinels in cloud VMs | New |

---

## Competitive Positioning

### Tools We Should Integrate As Teachers (Not Compete With)

| Tool | Role in Our System | Integration Path |
|------|-------------------|------------------|
| **Claude Code** | Primary teacher agent | Already implemented (ClaudeCodeProvider) |
| **Codex CLI** | Secondary teacher (Rust expertise) | New CodingAgentProvider |
| **Aider** | Tertiary teacher (git workflow, repo map) | New CodingAgentProvider |
| **SWE-agent** | Batch task solver (GitHub issues) | Subprocess + trace capture |

### Ideas We Should Adopt

| Idea | Source | How It Maps |
|------|--------|------------|
| PersonalizedPageRank repo map | Aider | CodebaseIndex service (GAP 1) |
| Context rot prevention | GSD | Step-result summarization in long pipelines |
| Memory Bank | Cline | Persona memory already exists — just wire to sentinel context |
| Linter-gated edits | SWE-agent | Validation step after CodingAgent edits |
| Focus Chain | Cline | Pipeline progress as persistent todo list |
| Progressive skill disclosure | Codex | Lazy-load pipeline inputs on demand |
| Event stream as state | OpenHands | Our SentinelEventBridge already does this |

### What NOBODY Has (Our Opportunity)

| Capability | Description | Status |
|-----------|-------------|--------|
| **Agent→LoRA distillation** | Run powerful agents, capture traces, train smaller models | Prototype exists |
| **Autonomous curriculum design** | AI designs its own learning plan | Academy teacher sentinel |
| **Multi-modal training pipeline** | Text → Voice → Image → Video training | Architecture designed, text proven |
| **Persona identity + memory + skills** | Persistent citizen with learned capabilities | Infrastructure exists |
| **P2P genome sharing** | Trade LoRA adapters across nodes | Architecture designed |
| **Self-improving agents** | Agents that get better over time through LoRA | The whole vision |

---

## Research References

### Agent Distillation
- [FireAct](https://arxiv.org/abs/2310.05915) — 500 GPT-4 trajectories → 77% improvement in fine-tuned Llama2-7B
- [NVIDIA Data Flywheel](https://developer.nvidia.com/blog/build-efficient-ai-agents-through-model-distillation-with-nvidias-data-flywheel-blueprint/) — 1B model achieved 98% of 70B tool-calling accuracy
- [Nemotron 3 Nano](https://arxiv.org/pdf/2512.20848) — Distills from SWE-Agent/OpenHands traces
- [DeepSeek-R1](https://arxiv.org/abs/2501.12948) — 800K reasoning traces, SFT-only distillation
- [Agent-FLAN](https://arxiv.org/html/2403.12881v1) — Decomposed training + negative samples

### LoRA Composition
- [LoRA Soups (COLING 2025)](https://arxiv.org/abs/2410.13025) — Optimal weighted LoRA merging
- [LoRAtorio](https://arxiv.org/html/2508.11624v1) — Train-free multi-LoRA composition
- [Task-Aware Vector DB Composition](https://arxiv.org/abs/2602.21222) — Maps to our GenomicSearchEngine concept

### Code Agent Design
- [SWE-agent ACI](https://arxiv.org/abs/2405.15793) — Agent-Computer Interface design
- [OpenHands](https://arxiv.org/abs/2407.16741) — Event stream architecture
- [AIDev Dataset](https://arxiv.org/html/2509.14744v1) — 456K agentic PRs from 5 coding agents

### Reinforcement Learning for Code
- [RLEF (ICML 2025)](https://arxiv.org/abs/2410.02089) — RL with execution feedback
- [CodeRL+](https://arxiv.org/pdf/2510.18471) — Execution semantics alignment
- [Apple RLAIF](https://machinelearning.apple.com/research/applying-rlaif) — 780M model surpassed 7B baseline

---

## Conclusion

Our system is architecturally positioned at the intersection that the entire field is converging toward: **agents that learn**. Every competitor is a better coding agent than our sentinels. But none of them learn. None of them have persistent identity. None of them train LoRA adapters from their own sessions. None of them have autonomous curriculum design.

The strategy is clear:
1. **Use the best agents as teachers** (Claude Code, Codex, Aider)
2. **Capture their expertise as training data** (interaction traces with quality scores)
3. **Train local personas via LoRA** (the distillation flywheel)
4. **Evaluate and iterate** (benchmark sentinels, auto-rollback)
5. **Make sentinels a natural extension of persona cognition** (autonomous dispatch, memory integration)

The field builds better hammers. We're building the blacksmith.
