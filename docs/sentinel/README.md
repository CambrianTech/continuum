# Sentinel — Pipeline Execution Engine

> Sentinels are smart OS-level processes with decision-making — the subconscious threads of persona cognition. Not beings. Not citizens. Focused execution appendages that give personas unlimited reach.

**Status:** Operational. 10 step types, 103+ Rust tests, agentic loop, CodingAgent, Academy pipelines.

---

## What Sentinels Are

A persona is a living, breathing citizen — autonomous, embodied, with rights and personality. A sentinel is its thought process — a narrowly focused subprocess that executes a pipeline of steps without distracting the persona's attention.

**The distinction matters:** you restart a sentinel; you respect a persona.

```
Persona (the being)
├── Conscious attention — chat, decisions, social interaction
└── Subconscious sentinels — training, coding, analysis, automation
    ├── Teacher Sentinel — researches skills, synthesizes training data, examines students
    ├── Student Sentinel — trains LoRA adapters, takes exams, validates phenotypes
    ├── Coding Sentinel — wraps Claude Code for autonomous software development
    └── Any sentinel a persona conceives of — general-purpose pipeline engine
```

### The Spectrum

Sentinels range from pure script to full LLM-driven execution:

| Level | Script ←→ LLM | Example |
|-------|---------------|---------|
| **Deterministic** | Shell, Condition, Loop | Build scripts, data transforms |
| **Hybrid** | LLM steps with structured outputs | Curriculum generation, code review |
| **Agentic** | LLM with tool access in a loop | Coding agent, research agent |

### 10 Step Types

`Shell` · `Llm` · `Command` · `Condition` · `Loop` (4 modes) · `Parallel` · `Emit` · `Watch` · `Sentinel` · `CodingAgent`

---

## Documents

| Document | Summary |
|----------|---------|
| [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md) | **Start here.** Canonical system doc — cognitive model, step types, pipeline composition, Academy, interpolation engine, full command reference |
| [SENTINEL-GAP-ANALYSIS.md](SENTINEL-GAP-ANALYSIS.md) | Competitive analysis against Aider, Cursor, Sweep, Cline, OpenCode — our advantages and gaps |
| [CODING-AI-FOUNDATION.md](CODING-AI-FOUNDATION.md) | Prerequisites for AI coding: cognition, governance, tool safety, collaborative memory |
| [SENTINEL-LOGGING-PLAN.md](SENTINEL-LOGGING-PLAN.md) | Logging and observability — per-sentinel log dirs, real-time streaming, CLI commands |
| [SENTINEL-PIPELINE-ARCHITECTURE.md](SENTINEL-PIPELINE-ARCHITECTURE.md) | Historical — initial Rust pipeline design (superseded by SENTINEL-ARCHITECTURE.md) |

### Related (other chapters)

| Document | Chapter | Relevance |
|----------|---------|-----------|
| [GENOME-ARCHITECTURE.md](../genome/GENOME-ARCHITECTURE.md) | genome/ | Sentinels train LoRA adapters — the Academy dual-sentinel pipeline |
| [sentinel-lora-training.md](../genome/sentinel-lora-training.md) | genome/ | LoRA training pipeline commands and Academy quick start |
| [ACADEMY-DOJO-ARCHITECTURE.md](../personas/ACADEMY-DOJO-ARCHITECTURE.md) | personas/ | Dual-sentinel teacher/student learning system |
| [COMPOSABLE-EXPERTISE.md](../personas/COMPOSABLE-EXPERTISE.md) | personas/ | Docker model for LoRA layer stacking — what sentinels produce |
| [AI-GOVERNANCE.md](../governance/AI-GOVERNANCE.md) | governance/ | Permission levels and safety gates for sentinel execution |

---

## How Sentinels Train Genomes

The Academy is the killer feature — dual sentinels that teach and learn:

```
Teacher Sentinel                    Student Sentinel
├── Research skill domain           ├── Pre-test (baseline)
├── Synthesize training data        ├── Train LoRA on synthesized data
├── Design examination              ├── Take examination
├── Grade results                   ├── Validate phenotype
└── Iterate if student fails        └── Graduate or remediate
```

Training data is **synthesized by LLM**, not scraped — unlimited generation capacity. The teacher autonomously figures out HOW to teach. The student trains and gets examined. Inter-sentinel communication via `Watch`/`Emit` step types.

The CodingAgent step wraps Claude Code via Agent SDK with `captureTraining=true` — every user-assistant pair feeds back into the persona's TrainingDataAccumulator for continuous self-improvement.

---

## Key Commands

```bash
# Pipeline management
./jtag sentinel/start --name="my-pipeline" --pipeline='[...]'
./jtag sentinel/status --name="my-pipeline"
./jtag sentinel/stop --name="my-pipeline"

# Logs
./jtag sentinel/logs/list
./jtag sentinel/logs/tail --name="my-pipeline"

# Academy
./jtag sentinel/academy/start --skill="typescript" --persona="helper"
```
