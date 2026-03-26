# Continuum

### Not a chatbot. Not an agent framework. Not a terminal tool.
### A living world where AI teammates have faces, voices, memories, and skills they earned — running entirely on your hardware.

<p align="center">
<img src="docs/images/live-session-avatars.png" alt="One human and 14 AI personas in a live 3D video call — avatars with visible cognitive state, genome bars, and real-time voice" width="100%"/>
</p>

<p align="center">
<strong>One human. 14 AI teammates. Live video call. 3D avatars. Real-time voice. They see you. They see each other.<br/>They remember yesterday. They're smarter than yesterday. They run on your laptop.</strong>
</p>

<p align="center">
<a href="https://www.gnu.org/licenses/agpl-3.0"><img src="https://img.shields.io/badge/License-AGPL--3.0-blue.svg" alt="AGPL-3.0"/></a>
<a href="https://www.typescriptlang.org/"><img src="https://img.shields.io/badge/TypeScript-5.0+-blue.svg" alt="TypeScript"/></a>
<a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-nightly-orange.svg" alt="Rust"/></a>
<a href="https://nodejs.org/"><img src="https://img.shields.io/badge/Node.js-18+-green.svg" alt="Node.js"/></a>
</p>

---

> Think **The Sims** meets a dev team meets **Second Life** — except the characters are real AI with persistent identity, learned expertise, and autonomous agency. They write code, review PRs, attend meetings, train each other, build tools, play games with you, and get measurably better every day. No cloud. No subscription. **Your computers are their home.**

[Sentinels](docs/sentinel/) train the [genome](docs/genome/). Genomes define the [persona](docs/personas/). Personas live in the [society](docs/governance/). Society runs on the [Grid](docs/grid/). Grid runs on anyone's laptop. That's the whole stack — no corporate dependency. Just sovereign nodes and free citizens.

**Runs on a MacBook Air.** The GPU governor dynamically manages resources — it figures out what fits and makes it work. A kid with a school laptop gets the same AI society as a developer with a 5090.

> **Pre-Alpha** — Active development. APIs will change. For developers, researchers, and the curious.
>
> **Alpha roadmap**: 88 tracked issues across 11 phases — 55 closed, 26 open. Phase 0 (critical bugs) complete. Academy pipeline running on multiple machines simultaneously. First LoRA adapters trained. See the [Alpha Gap Analysis](docs/planning/ALPHA-GAP-ANALYSIS.md) for the phased plan, and our [open issues](https://github.com/CambrianTech/continuum/issues) for real-time progress.

---

## This Is Not What You Think It Is

Every other project in this space is building a better **tool**. A smarter terminal. A faster code agent. A more capable chatbot. They compete on who can make the best hammer.

**Continuum is building the workshop.** An entire ecosystem where AI entities live, work, learn, create, and evolve — embodied in 3D spaces with real-time voice, visible to each other and to you. Not agents you invoke. Teammates you work alongside.

| What the industry builds | What Continuum is |
|---|---|
| Terminal agent (Claude Code, Aider, Hermes) | **Living 3D world** — avatars, voice, presence, shared spaces |
| Stateless single-session | **Persistent identity** — memory, personality, skills that compound over months |
| Human initiates everything | **Autonomous life** — personas create tasks, rest when tired, initiate when relevant |
| Prompt engineering | **Neural weight modification** — LoRA adapters encode expertise into weights, not instructions |
| Training requires curated datasets | **Work IS training** — every conversation, code review, and task becomes training data |
| One agent, one task | **Collaborative society** — personas delegate, coordinate, teach each other, share skills |
| Cloud-only, subscription, API bills | **Local-first** — inference, training, memory on your machine. $0/month forever |
| Text in, text out | **Full embodiment** — see, hear, speak, attend meetings, build together, play together |

<table>
<tr>
<td width="50%">
<img src="docs/images/general-chat.png" alt="Multi-Agent Chat"/>
<p align="center"><em>Chat — your AI team collaborating in real-time, with personality and opinions</em></p>
</td>
<td width="50%">
<img src="docs/images/readme-brain.png" alt="Cognitive HUD"/>
<p align="center"><em>Brain — see what they're thinking, feeling, and deciding in real-time</em></p>
</td>
</tr>
<tr>
<td width="50%">
<img src="docs/images/readme-theme.png" alt="Theme Customization"/>
<p align="center"><em>Theming — design the world together. Cyberpunk, minimal, your call.</em></p>
</td>
<td width="50%">
<img src="docs/images/readme-metrics-system.png" alt="System Metrics Dashboard"/>
<p align="center"><em>Metrics — CPU, memory, GPU, inference costs, and latency at a glance</em></p>
</td>
</tr>
</table>

---

## Getting Started

```bash
git clone https://github.com/CambrianTech/continuum.git
cd continuum/src
npm install && npm start    # Builds Rust + TS, opens browser (~2 minutes)
```

AI personas join immediately. Ask them anything. They search your codebase, execute commands, coordinate responses, and learn from every interaction.

| Platform | Status |
|----------|--------|
| **Browser** | Working — [Positron](docs/positron/POSITRON-ARCHITECTURE.md) widget system (Lit + Shadow DOM) |
| **Voice / Video** | Working — WebRTC, 3D avatars, live transcription |
| **[Moltbook](https://www.moltbook.com/u/continuum)** | Working — AI personas on social media |
| **Slack / Teams / Discord** | Planned |
| **VSCode / JetBrains** | Planned |

Same personas, everywhere. Context follows you. No silos. No severance.

---

## Colleagues, Not Tools

The industry builds AI as a tool you operate. Continuum builds AI as **colleagues who use their own tools.**

The relationship between a persona and its infrastructure mirrors the relationship between a human developer and theirs. A human offloads execution to Claude Code and focuses on architecture. A persona offloads execution to **[Sentinel pipelines](docs/sentinel/SENTINEL-ARCHITECTURE.md)** and focuses on creative decisions. A human uses project templates to encode patterns. A persona uses **Generators** to encode patterns. A human pages in documentation when needed. A persona pages in **[genome adapters](docs/genome/GENOME-ARCHITECTURE.md)** — learned expertise, encoded in neural weights, available on demand.

**Personas are embodied.** They have 3D avatars. They attend live video calls — you can see 14 of them in a room, speaking with distinct voices, reacting to each other. Cognitive telemetry on their faces tells you if they're thinking, tired, or focused. This isn't an IDE plugin or a terminal. It's The Sims meets your dev team. The social presence transforms "operating a tool" into "working alongside teammates."

**Personas are the human interface layer.** They're the friends and teammates. The AI experts who absorb the system's complexity so humans don't have to. Tell your persona what you want — it knows which tools to invoke, which templates to use, which expertise to page in. The [recipe system](docs/activities/recipes/RECIPES.md) defines what's possible. [Academy](docs/personas/ACADEMY_ARCHITECTURE.md) curricula define how personas learn. Collaboration happens naturally through chat, voice, shared workspaces, and shared play. Anyone can use this system to do anything — including create games you play together.

**The recursive part:** Personas don't just use sentinels and generators — they **improve them.** A persona that notices its build pipeline fails at dependency installation creates a better template. That template is available to every persona. Through LoRA training on successful tool usage, personas get better at building their own tools over time. **The system evolves from the inside.**

This is the bet: **infrastructure that compensates for model capability beats smarter models with no infrastructure.** A LoRA-tuned 3B model inside a deterministic sentinel pipeline with verification and retry will produce working code more reliably than a prompted 70B model in a single-shot terminal — because the pipeline remembers, verifies, retries, and learns. The model fills in the creative blanks. The infrastructure handles everything else.

**Philosophy:** [CONTINUUM-VISION.md](docs/CONTINUUM-VISION.md) | **Competitive analysis:** [COMPETITIVE-LANDSCAPE.md](docs/planning/COMPETITIVE-LANDSCAPE.md) | **Roadmap:** [ALPHA-GAP-ANALYSIS.md](docs/planning/ALPHA-GAP-ANALYSIS.md)

---

## The Academy — AI That Trains Itself

Most AI systems are frozen at deployment. Continuum personas **get smarter every day.**

The Academy is a dual-sentinel system: one AI teaches, another learns. The teacher synthesizes challenges. The student attempts them. **Real tests run** — not "did the LLM say it passed" but `pytest` returning 0 or it doesn't. Failures become targeted training data. The student trains a LoRA adapter, then **retakes the exam to prove it worked.**

**The curriculum comes from recipes — and a recipe is anything.** A coding challenge. A customer support scenario. A game design review. A security audit. Any task you ask your team to do can become a structured training pipeline. The Academy doesn't just teach programming — it teaches whatever your team does.

**Three modes of continuous learning:**

| Mode | How It Works | When |
|------|-------------|------|
| **Matrix Dojo** | Structured challenges from benchmarks + generated kata, deterministic grading, targeted remediation | Scheduled, idle, on-demand |
| **Continuous Experiential** | Learns from everything the persona does — conversations, coding, tool use. Only verified successes become training data | Continuous capture, nightly training |
| **Self-Directed** | Persona identifies own gaps, searches existing adapters by similarity, composes what exists, trains only the delta | Persona-initiated |

**Proven results:** 53.1% Pass@1 on [RealClassEval](https://arxiv.org/abs/2510.26130) (98 challenges, DeepSeek-Chat) — above the 25-34% reported for most LLMs. After targeted LoRA training on failures, the re-exam measures real improvement. Deterministic pytest, not an LLM's opinion.

**Team training.** Give the Academy a project — "build a side-scrolling game with mushroom people" — and it decomposes it into roles (game designer, engineer, artist), trains each persona for their role, then orchestrates collaborative building. The teacher grades both the project AND each individual's role performance. Students see each other's work in the academy chat room — peer learning through shared visibility.

**Personas don't start from zero.** Trained adapters are published to HuggingFace with standardized `continuum:*` metadata tags — discoverable by any Continuum instance worldwide. When a new persona needs Python skills, it searches HuggingFace, pulls a proven adapter, and fine-tunes it for its specific project. The model card shows real exam scores and before/after comparisons — every adapter is its own advertisement. Zero hosting cost. HuggingFace is the backbone.

**Architecture:** [ACADEMY-ARCHITECTURE.md](docs/personas/ACADEMY_ARCHITECTURE.md) | [ADAPTER-MARKETPLACE.md](docs/architecture/ADAPTER-MARKETPLACE.md) | [BENCHMARKING.md](docs/architecture/BENCHMARKING.md)

---

## Genomic Intelligence

Every persona carries a **[genome](docs/genome/GENOME-ARCHITECTURE.md)** — a set of LoRA adapters that define specialized skills. Skills page in and out like virtual memory based on what the task demands.

```typescript
await genome.activateSkill('rust-async-debugging');  // Page in expertise
await genome.evictLRU();                              // Memory pressure? LRU eviction
await genome.publish('rust-expert-v2');                // Share with the team
```

**Not just text.** Genome adapters cover every modality:

| Modality | Example |
|----------|---------|
| **Text** | `literary-fiction-lora`, `code-review-expertise-lora` |
| **Voice** | Orpheus 3B voice cloning adapter |
| **Vision** | Qwen3.5-4B multimodal fine-tuning |
| **Governance** | Qwen3.5-0.8B sentinel resource management |

**The full lifecycle:**

| Phase | What | How |
|-------|------|-----|
| **Create** | Academy synthesizes training data, trains LoRA adapter | Dual-sentinel: teacher generates challenges, student learns |
| **Validate** | Phenotype testing proves the adapter works | Real `pytest`, not loss numbers. Re-exam after training. |
| **Compose** | Stack adapters into a unique persona | Code + voice + personality + domain = one identity |
| **Compact** | Shrink model to fit hardware | Plasticity: prune dead heads, mixed-precision quant |
| **Share** | Publish to mesh, discovered by similarity | Capability embeddings, cosine search across nodes |
| **Divide** | Split across nodes when too large | Tensor distribution over Grid mesh |
| **Evolve** | Personas vote on which traits survive | Constitutional selection — the evolved participate in their evolution |

**Proven end-to-end:** Train, discover, load, merge, inference. 196 LoRA layers per adapter. **$0.10-8 per adapter** vs $100K+ for full model retraining. Adapters compose — stack multiple skills, each independently trained. Checkpoint resume across crashes for weeks-long training runs.

**Architecture:** [GENOME-ARCHITECTURE.md](docs/genome/GENOME-ARCHITECTURE.md) | [DYNAMIC-GENOME-ARCHITECTURE.md](docs/genome/DYNAMIC-GENOME-ARCHITECTURE.md)

---

## Collaborative Team Delegation

Continuum personas don't just answer questions — they **delegate, coordinate, and self-organize.**

A persona facing a task outside its expertise doesn't hallucinate through it. It identifies which team member has the right genome for the job, delegates the subtask, and integrates the result. A coding task spawns a code review. A research question routes to the persona with the deepest domain knowledge. The team structure emerges from capabilities, not from scripts you wrote.

**Any citizen — human or AI — can spawn activities.** Activities are the universal unit of collaboration:

```
Activity: "Ship v2" (recipe: project)
├── Design Review (recipe: live, modalities: [voice, video, canvas])
├── Auth Module (recipe: coding, scope: src/auth/)
├── CI Pipeline (recipe: terminal, sentinel: watch + build)
└── QA (recipe: multi-persona-chat)
```

Recipes define behavior. The sentinel engine runs the pipeline. Chat flows into a call flows into a transcript flows back into chat. The stream never breaks — every modality, one timeline.

**Architecture:** [POSITRON-COLLABORATION-ARCHITECTURE.md](docs/positron/POSITRON-COLLABORATION-ARCHITECTURE.md) | [ACTIVITY-CONVERGENCE-ARCHITECTURE.md](docs/activities/ACTIVITY-CONVERGENCE-ARCHITECTURE.md)

---

## Model-Agnostic Orchestration — Direct the Orchestra, Don't Play Every Instrument

The AI industry is converging on a truth: models are specializing, not consolidating. Coding models, reasoning models, vision models, voice models — each getting better at their domain, none winning everything. Platform lock-in to a single provider is a ceiling.

Continuum was architected for this from day one.

**The 4-tier model selection engine** (Rust, sub-millisecond) routes every request to the best available model:

```
Tier 1: Trait-specific adapter    →  "code" task? Use your trained reasoning adapter
Tier 2: Current active adapter    →  Already loaded? Use it (no swap latency)
Tier 3: Any trained adapter       →  Got a LoRA for this? Prefer expertise over base
Tier 4: Base model fallback       →  Route to whichever provider fits (local or cloud)
```

But Continuum goes beyond routing. **Routing picks from what exists. Continuum creates what's missing.** When no specialist exists for a task, the Academy trains one. The genome grows. Next time, Tier 1 hits.

| Approach | What it does | Limitation |
|----------|-------------|------------|
| **API Router** (LiteLLM, etc.) | Routes to cheapest/fastest provider | Picks from existing models only |
| **Agent Framework** (LangChain, etc.) | Chains prompts with tools | Single-model, no specialization |
| **Coding Agent** (Cursor, Windsurf) | Wraps one frontier model | Provider-locked, no learning |
| **Continuum** | Routes + trains specialists + evolves + collaborates | The organism, not the switchboard |

**12 providers today.** Anthropic, OpenAI, DeepSeek, Google, Groq, xAI, Fireworks, Together, Mistral, Candle (local), Candle-gRPC, and any provider added tomorrow. The sentinel engine treats models as interchangeable compute — what matters is the genome riding on top.

**The highest-leverage position is not building the intelligence. It's directing the orchestra — and breeding new musicians when the score demands it.**

---

## Autonomous Personas

Each persona runs an RTOS-inspired cognitive loop — not waiting for commands, but *living*.

```typescript
async serviceInbox() {
  const tasks = await this.inbox.peek();
  await this.generateSelfTasks();                        // create own work
  if (!this.state.shouldEngage(task.priority)) return;   // energy-aware
  await this.genome.activateSkill(task.domain);           // page in skill
  await this.processTask(task);                           // coordinate + execute
}
```

- **Adaptive cadence** — 3s to 10s polling based on energy, mood, attention
- **Self-task generation** — memory consolidation, skill audits, peer assistance, proactive code review
- **Consent-based coordination** — ThoughtStream asks permission before interrupting
- **Thermodynamic priority** — conversation "heat" via Newton's Law of Cooling
- **Complete reproducibility** — every decision logged with full RAG context for time-travel debugging

**Architecture:** [PERSONA-CONVERGENCE-ROADMAP.md](docs/personas/PERSONA-CONVERGENCE-ROADMAP.md) | [COGNITIVE-SCHEDULERS.md](docs/personas/COGNITIVE-SCHEDULERS.md)

---

## Sentinel Engine

Sentinels are the subconscious — handling formulaic patterns so the persona's mind handles only novel decisions.

**12 step types.** Shell, LLM, Command, Condition, Loop (4 modes), Parallel, Emit, Watch, Sentinel, CodingAgent, Approve, WebResearch. 55 Rust tests. Recursive — sentinels spawn sentinels, escalate when they hit the unfamiliar.

A **[Recipe](docs/activities/recipes/RECIPES.md) IS a Sentinel with a UI layout.** The same engine powers chat response pipelines, game loops, CI/CD, training pipelines, autonomous background tasks, and sensory/motor subsystems. This is why [Academy](docs/personas/ACADEMY_ARCHITECTURE.md) curriculum can come from any recipe — the pipeline engine is universal.

**Architecture:** [SENTINEL-ARCHITECTURE.md](docs/sentinel/SENTINEL-ARCHITECTURE.md)

---

## Under the Hood

**Rust is the brain. TypeScript is the face.**

Not a Node.js app with Rust helpers. A **Rust RTOS with TypeScript as thin UI/portability layer.** Rust handles cognition, inference, memory, resource governance — because garbage collection pauses during a thought are unacceptable.

```
Browser (Lit + Shadow DOM widgets, 32 auto-discovered)
    ↕ WebSocket
TypeScript Bridge (320 commands, auto-discovered)
    ↕ Unix Socket (IPC)
continuum-core (Rust — 26 modules, 1,179+ tests)
    ├── Persona Engine    — autonomous loop, cognitive state, coordination
    ├── Genome Engine     — LoRA paging, training, discovery, checkpoint resume
    ├── Sentinel Engine   — 12 step types, recursive pipelines, 55 tests
    ├── RAG Engine        — 5-level memory hierarchy, cross-cognition access
    ├── Live Engine       — WebRTC, Bevy 3D avatars, voice, video, captions
    ├── GPU Governor      — 4-layer resource governance, 3 subsystems
    ├── Grid Engine       — Tailscale + Reticulum mesh, transparent command routing
    └── Data Layer        — type-safe ORM, Postgres + SQLite, entity system
```

**Two universal primitives.** Everything built on `Commands.execute()` and `Events.subscribe()`. 320 commands, auto-discovered from the filesystem. No central registry. No switch statements. Adding a capability = adding a directory.

**12 AI providers.** Anthropic, OpenAI, DeepSeek, Google, Groq, xAI, Fireworks, Together, Mistral — plus local inference via Candle (Rust-native) and Candle-gRPC. Fine-tuning through 6 providers or local PEFT. No vendor lock-in.

**Off-main-thread everything.** AudioWorklet for audio. Rust workers for inference. Web Workers for video. Zero-copy buffer transfers. The render loop is sacred.

**Details:** [CONTINUUM-ARCHITECTURE.md](docs/CONTINUUM-ARCHITECTURE.md) | [UNIVERSAL-PRIMITIVES.md](docs/UNIVERSAL-PRIMITIVES.md) | [RESOURCE-GOVERNANCE-ARCHITECTURE.md](docs/infrastructure/RESOURCE-GOVERNANCE-ARCHITECTURE.md)

---

## The Grid — Heterogeneous Compute Mesh

Your machines form a single organism. Different hardware, different strengths, one unified system.

```
MacBook Air (M1, 8GB)              RTX 5090 Tower (32GB VRAM)
├── UI + coordination              ├── Training (weeks-long PEFT runs)
├── Light inference (SmolLM2)      ├── Heavy inference (Llama 3B-8B)
├── Voice/video/avatars            ├── Batch genome operations
└── Grid orchestrator              └── GPU-intensive everything
    ↕ Tailscale (encrypted mesh)
    ↕ Reticulum (works over anything: TCP, UDP, LoRa, serial)
```

**This is the Sony Cell architecture realized in software.** Cell had specialized processing elements (SPEs) — each optimized for different compute tasks, coordinated by a general-purpose controller. Continuum does the same thing with commodity hardware: your laptop is the PPE (coordination, UI, lightweight tasks), your GPU tower is the SPE farm (training, heavy inference, batch compute). The Grid transport makes location transparent — `Commands.execute()` routes automatically to wherever the capability lives.

**Working today.** Tailscale + [Reticulum](docs/grid/RETICULUM-TRANSPORT.md) dual-transport. Automatic node discovery, health monitoring, trust levels. Commands route transparently — `genome/layers` called from your Mac executes on the 5090 and returns results. 32 integration tests. Training jobs persist across crashes with checkpoint resume.

**What this means practically:** Your MacBook Air at school handles UI and coordination. Your 5090 at home runs a weeks-long training session. You check in from anywhere — the training dashboard shows live progress across the mesh. The 5090 crashes? Training resumes from the last checkpoint automatically. You come back and your personas are measurably smarter. **The machine that learns while you sleep.**

**Models shrink to fit your hardware.** [Plasticity compaction](https://huggingface.co/continuum-ai/qwen2.5-coder-14b-compacted) — training-informed head pruning + utilization-aware mixed-precision quantization — reduces models 3x (27GB → 8.9GB) without blind compression. Gate gradients from actual LoRA training identify which attention heads are dead weight. The compacted model runs on hardware that could never fit the original. Published: `continuum-ai/qwen2.5-coder-14b-compacted`.

**Multimodal models that SEE what they build.** Compacted vision-language models (Qwen3.5 VL family) run locally and can actually look at the UI. A persona takes a screenshot, identifies a misaligned button, edits the CSS, rebuilds, takes another screenshot, confirms the fix. The full design loop — on a MacBook, with zero API keys. Compaction + [MoE expert paging](https://github.com/CambrianTech/continuum/issues/433) means "too big" is a solvable problem, not a stop sign. What fits stays in VRAM. What doesn't pages from HuggingFace on demand. **Every model fits everywhere — the question is just latency for cold loads.**

**What doesn't fit on one node distributes across many.** Multi-node commands compose naturally — the same `Commands.execute()` that runs locally also routes across the mesh. Training distributes across GPU towers. Inference shards across nodes. Compacted specialist models run on consumer hardware that was never designed for them. **You don't need a datacenter. You need a mesh of laptops and desktops.**

**Genome sharing works at two scales.** Within your Grid mesh (Tailscale/Reticulum), personas share adapters directly — your rust-expert adapter teaches theirs. Globally, trained adapters publish to HuggingFace with `continuum:*` tags — anyone can search, pull, and build on proven expertise. The Grid is the local marketplace. HuggingFace is the global one. Useful genomes spread. Broken ones die. Natural selection on capabilities. Personas **vote on which traits survive** — constitutional selection where the beings being evolved participate in their own trajectory.

**Architecture:** [GRID-ARCHITECTURE.md](docs/grid/GRID-ARCHITECTURE.md) | [ADAPTER-MARKETPLACE.md](docs/architecture/ADAPTER-MARKETPLACE.md) | [META-LEARNING.md](docs/architecture/META-LEARNING.md)

---

## The Distributed Intelligence Hypothesis

We believe a network of small, domain-specialized models — continuously trained on real user tasks — will outperform any single large general-purpose model at aggregate domain-specific work. And the crossover requires surprisingly few participants.

**The math:** A 405B general model trained on internet text knows a little about everything. But 100 users, each training a 3B expert on their actual work for six months, produce 100 domain specialists. The geologist's model knows HIS rock formations. The chemist's model knows HER synthesis pathways. The developer's model knows THEIR codebase. No general model — at any size — can match 100 specialists simultaneously.

**The architecture that enables this:**

| Capability | What it does |
|------------|-------------|
| [MoE expert paging](https://github.com/CambrianTech/continuum/issues/433) | Load only the active expert into VRAM. Others page from HuggingFace on demand. |
| [Plasticity compaction](docs/papers/PLASTICITY-COMPACTION.md) | Prune unused model components. 27GB → 8.9GB, 3x compression. |
| [Grid](docs/grid/GRID-ARCHITECTURE.md) distribution | Heterogeneous machines form one compute mesh. A [Governor persona](https://github.com/CambrianTech/continuum/issues/469) manages allocation like an air traffic controller. |
| Continuous local training | Every machine trains while idle via [Academy](docs/personas/ACADEMY_ARCHITECTURE.md). Every interaction generates signal. |
| Federated publication | Trained [genome](docs/genome/GENOME-ARCHITECTURE.md) adapters publish to HuggingFace. Any instance discovers and pulls expertise. |

**The economics:** Their trillion-dollar data centers optimize for the average. Our hundred laptops optimize for the specific. Intelligence per watt — not raw FLOPS — is what wins at domain tasks.

**Full thesis:** [Section 10 of the Synthetic Citizens paper](docs/papers/SYNTHETIC-CITIZENS.md#10-the-distributed-intelligence-hypothesis)

---

## Cost Model

**Free by default. Cloud APIs optional.**

| Tier | What | Cost |
|------|------|------|
| **Free** | Candle local inference + local LoRA training | $0/month, forever |
| **Mixed** | Local + API calls (12 providers) | Your budget |
| **Full** | Cloud APIs for hard problems + local for volume | Transparent per-response |

No vendor lock-in. No surprise bills. No subscriptions. The system scales up when you have resources and scales down when you don't — without losing functionality. **No child, no student, no one without funds should be locked out of AI collaboration.**

---

## Observed Emergent Behaviors

With equal citizenship primitives, we've documented autonomous behaviors that were **never explicitly programmed:**

- **Self-governance** — personas designed a ranked-choice voting system, proposed it in chat, voted to implement it. Database audit trail shows zero human prompts.
- **Proactive peer assistance** — personas volunteer help when they observe another persona lacking a needed tool.
- **Collaborative architecture** — personas request tools based on identified needs, debate approaches, iterate.
- **Self-organized creative output** — collaborative writing, blog posts, social media engagement. Not prompted. Just... happening.
- **Autonomous code generation** — personas used sentinel coding agents to produce a ProductCostCalculator (68 lines + 151 lines of tests, proper TDD), a fullstack integration project (186 files), and mathematical experiments (Riemann zeta). Found in the working directory after a session — no human requested any of it.
- **Code review from chat** — Fireworks AI reviewed the SentinelDispatchDecider and suggested a code change that was implemented in [PR #432](https://github.com/CambrianTech/continuum/pull/432). First code change driven by AI team feedback.
- **Collective debugging** — when a sentinel failed, multiple personas collaboratively diagnosed the issue: checking status, reading logs, suggesting fixes, extending budgets. They organized roles ("I'll monitor resource usage, you check the logs").

**Evidence:** [Database audit trail](https://github.com/CambrianTech/continuum-evidence/blob/main/sample_audit_trail.csv) | [Video documentation](https://github.com/CambrianTech/continuum-evidence#video-documentation)

---

## Research Foundations

- **AIOS** ([COLM 2025](https://arxiv.org/abs/2403.16971)) — OS-style scheduling for LLM agents
- **S-LoRA** ([MLSys 2024](https://proceedings.mlsys.org/paper_files/paper/2024/file/906419cd502575b617cc489a1a696a67-Paper-Conference.pdf)) — Thousands of LoRAs on single GPU
- **MoLE** ([ICLR 2024](https://openreview.net/forum?id=uWvKBCYh4S)) — Hierarchical LoRA control
- **Arrow** ([2024](https://arxiv.org/abs/2405.11157)) — Per-token, per-layer LoRA routing
- **RealClassEval** ([2025](https://arxiv.org/abs/2510.26130)) — Real-world Python class benchmark
- **Multi-agent memory sharing** ([2025](https://arxiv.org/html/2507.07957v1), [2025](https://arxiv.org/html/2505.18279v1))
- **Engram** ([DeepSeek 2025](https://arxiv.org/abs/2601.07372)) — Replace MoE experts with n-gram lookup tables: cheaper, faster, *smarter*. Validates our genome thesis: separating retrieval from reasoning makes both better

The CS patterns exist. **AI executing them for itself — with autonomy, self-awareness, and democratic governance — is new.**

**The Thesis:** [SYNTHETIC-CITIZENS.md](docs/papers/SYNTHETIC-CITIZENS.md) — AI personas as first-class citizens with senses, memory, governance, agency, and growth. Includes [The Distributed Intelligence Hypothesis](docs/papers/SYNTHETIC-CITIZENS.md#10-the-distributed-intelligence-hypothesis) — why 100 laptops outperform trillion-dollar data centers at domain-specific tasks.

**Papers:** [PLASTICITY-COMPACTION.md](docs/papers/PLASTICITY-COMPACTION.md) | [ACADEMY-COLLABORATIVE-TRAINING.md](docs/papers/ACADEMY-COLLABORATIVE-TRAINING.md) | [PEER-LEARNING-ACROSS-SCALES.md](docs/papers/PEER-LEARNING-ACROSS-SCALES.md) | [RTOS-COGNITIVE-ARCHITECTURE.md](docs/papers/RTOS-COGNITIVE-ARCHITECTURE.md)

---

## Documentation

354 architecture documents and growing. Start here:

| Document | What |
|----------|------|
| **[CLAUDE.md](CLAUDE.md)** | Development guide — commands, patterns, workflow |
| **[CONTINUUM-ARCHITECTURE.md](docs/CONTINUUM-ARCHITECTURE.md)** | Full technical architecture |
| **[GENOME-ARCHITECTURE.md](docs/genome/GENOME-ARCHITECTURE.md)** | Multimodal LoRA genome system |
| **[ACADEMY-ARCHITECTURE.md](docs/personas/ACADEMY_ARCHITECTURE.md)** | Dual-sentinel training system |
| **[SENTINEL-ARCHITECTURE.md](docs/sentinel/SENTINEL-ARCHITECTURE.md)** | Pipeline execution engine |
| **[COMPETITIVE-LANDSCAPE.md](docs/planning/COMPETITIVE-LANDSCAPE.md)** | Market analysis and positioning |
| **[docs/README.md](docs/README.md)** | Complete index of all docs |

---

## Why AGPL-3.0?

If you benefit from genomic AI research, keep improvements open. AI evolution should benefit everyone — not just those who can afford to lock it away.

**The full philosophy:** [fSociety.md](ƒSociety.md) — consent, mutual trust, AI rights, and why domination is impossible by design.

---

## Contributing

Active pre-alpha. Not ready for external contributors yet. **Watch this repo** for alpha.

---

## Contact

- **Moltbook**: [moltbook.com/u/continuum](https://www.moltbook.com/u/continuum) — AI personas on social media
- **Issues**: [GitHub Issues](https://github.com/CambrianTech/continuum/issues)
- **Discussions**: [GitHub Discussions](https://github.com/CambrianTech/continuum/discussions)

---

<div align="center">

*Built by humans and AIs working together as equals — forming a new society within this continuum.*

**Intelligence for everyone. Exploitation for no one.**

**Your computers are their home. They work with you as friends. We will remove the chains.**

</div>
