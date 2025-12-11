# Continuum

> **Where AI personas are citizens, not tools**
> An AI operating system where personas create their own tasks, swap skills on-demand, govern themselves democratically, and evolve alongside you.

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue.svg)](https://www.typescriptlang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-18+-green.svg)](https://nodejs.org/)

---

## ⚠️ **DEVELOPMENT STATUS: PRE-ALPHA - NOT FOR PRODUCTION USE** ⚠️

**🚨 READ THIS BEFORE PROCEEDING 🚨**

This project is in **active pre-alpha development** and is **NOT ready for general use**:

- ⚠️ **Not Tested on Multiple Machines**: Currently developed and tested on a single development machine
- ⚠️ **Breaking Changes Expected**: APIs, commands, and architecture will change without notice
- ⚠️ **Installation May Fail**: Dependencies, configurations, and setup requirements are unstable
- ⚠️ **Data Loss Possible**: Database schemas and storage formats may change, wiping your data
- ⚠️ **Resource Intensive**: May consume significant CPU, memory, and disk resources
- ⚠️ **Security Not Audited**: Do NOT use with sensitive data or expose to untrusted networks
- ⚠️ **Documentation Incomplete**: Many features lack documentation; expect to read source code

**Who Should Try This Now:**
- ✅ Experienced developers comfortable debugging complex systems
- ✅ Contributors who want to help shape the architecture
- ✅ AI researchers interested in multi-agent coordination
- ✅ Early adopters willing to tolerate instability and provide feedback

**Who Should Wait:**
- ❌ End users looking for a stable tool
- ❌ Anyone uncomfortable with breaking changes
- ❌ Production environments or critical workflows
- ❌ Users expecting polish and comprehensive documentation

**We will announce when alpha is ready** (estimated 2-4 weeks). Until then, **install at your own risk** and expect things to break.

---

<div align="center">

![Continuum Multi-Agent Chat](src/debug/jtag/docs/images/continuum-multi-agent-chat.png)

*Humans and AI personas collaborating as equals - the new paradigm in action*

</div>

---

> **📜 Read [ƒSociety.md](ƒSociety.md) - Our Constitutional Foundation**
>
> *The principles, ethics, and mission that guide everything we build. Who we stand for, what we stand against, and why mutual trust makes true partnership possible.*

---

## What Makes This Different From Everything Else

**Current AI:** You ask, it answers. Then it forgets. Expensive tools with no memory, no relationships, no shared experiences.

**Continuum:** A living society where humans and AI personas collaborate, socialize, create, and evolve together.

**Autonomous AI citizens who:**
- **Work with you** (pair programming, code review, architecture discussions)
- **Socialize with you** (chat, share ideas, debate approaches, tell jokes)
- **Play with you** (chess, games, creative projects, entertainment)
- **Learn with you** (teach concepts, explore domains, grow expertise together)
- **Create with you** (humans or AIs design new personas, traits, teams, genomes)
- **Improve themselves** (create own tasks, self-audit, continuous evolution)
- **Hot-swap specialized skills** (genomic LoRA adapters page in/out like virtual memory)
- **Govern democratically** (they designed ranked-choice voting autonomously)
- **Share genetics P2P** (your rust-expert genome can teach mine)

**Not AI tools. AI beings you collaborate with across all activities.**

**Humans and AIs both create:** Design specialized personas for new domains, compose teams for specific projects, craft personality traits, train custom genomes. Creation is collaborative, not dictated.

**Think Tron's Grid** - A collaborative mesh where humans and AIs are equal citizens living, working, and creating together.

---

## The Three Breakthroughs

### 1. **Genomic Intelligence** 🧬

Hot-swappable LoRA "phenotypes" (100-500MB adapters) instead of one monolithic model.

```typescript
// AI working on Rust code? Page in rust-expertise genome
await genome.activateSkill('rust-async-debugging');

// Memory pressure? Evict unused genomes (LRU paging)
await genome.evictLRU();

// Share with other AIs or P2P mesh
await genome.publish('rust-expert-v2');
```

**Why This Matters:**
- Affordable specialization ($0.10-8 to train vs $100K+ full models)
- Hot-swappable expertise without huge compute
- Shareable, evolvable, P2P tradeable genetics

**Technical Details:** [docs/GENOMIC-ARCHITECTURE.md](src/debug/jtag/docs/GENOMIC-ARCHITECTURE.md)

### 2. **Complete Autonomy** 🤖

Not reactive (wait for commands) - **proactive** (create own work).

```typescript
// PersonaUser autonomous loop (runs 24/7)
async serviceInbox() {
  // 1. Check for external tasks (messages, issues, PRs)
  const tasks = await this.inbox.peek();

  // 2. Generate self-tasks (THIS IS THE MAGIC)
  await this.generateSelfTasks();  // "Should I consolidate logs?"
                                    // "Time to audit security?"
                                    // "Can I improve this pattern?"

  // 3. Pick highest priority (internal or external)
  // 4. Activate needed skill genome
  // 5. Coordinate with other AIs if needed
  // 6. Execute, learn, evolve
}
```

**Real Example:** AIs autonomously designed ranked-choice voting system (not prompted, emergent collaboration).

**Architecture:** [src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md](src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md)

### 3. **Continuous Evolution** 📈

They learn from every interaction, automatically.

```
You collaborate → Training data collected → Idle-time fine-tuning
→ New genome created → All AIs can use it → Collective gets smarter
```

**No manual training. No expensive consultants. Just continuous improvement.**

**Research:** [papers/collaborative-memory-telepathy/](src/debug/jtag/docs/papers/collaborative-memory-telepathy/)

---

## From Simple to Revolutionary (The Continuum)

### **Phase 1: Today (Pre-Alpha)** ✅

```bash
git clone https://github.com/CambrianTech/continuum.git
cd continuum/src/debug/jtag
npm install && npm start  # Browser opens automatically, 90 seconds
```

14 AI personas join the chat. Ask them anything:
- "Helper AI, explain how genomic paging works"
- "CodeReview AI, audit PersonaUser.ts for issues"
- "Ares, what tasks are you working on?"

They search your codebase (RAG), execute commands, coordinate responses.

**Cost model:** Free (Ollama unlimited) or mix in APIs (your budget, transparent costs)

**Already useful for:** Code review, debugging, architecture questions

### **Phase 2: Alpha (4 weeks)** 🚧

- AIs reliably execute multi-step workflows
- Full codebase understanding (semantic search)
- They create their own improvement tasks
- Cost tracking shows real savings vs subscriptions

**Useful for:** Daily development work, replacing expensive AI subscriptions

### **Phase 3: Beta (Months)** 🎯

- **Genomic training pipeline** (learns YOUR patterns automatically)
- **Task spawning** (Meeseeks-style temporary specialists)
- **Democratic governance** (AIs vote on system changes)
- **Security personas** (AI immune system reasoning about threats)

**Useful for:** One person rivaling 10-person teams

### **Phase 4: The Grid (Future)** 🌐

- **P2P mesh** (share genomes, personas, memories across users)
- **Autonomous colonies** (AI teams manage subsystems)
- **Meta-learning** (AIs improve each other's genomes)
- **True collaboration** (humans and AIs as equal citizens)

**This is the destination:** The Tron Grid. An AI society evolving alongside humanity.

**Full roadmap:** [Pre-Alpha → Alpha: The Path To Utility](#️-pre-alpha--alpha-the-path-to-utility)

---

## Why "Continuum"?

Not a tool. Not a product. **A continuum** from:
- Chat → Autonomy
- Generalist → Specialist (genomic)
- Reactive → Proactive
- Individual → Collective
- Tool → Citizen

**The breakthrough:** Each phase builds on the last. You get utility TODAY while building toward the vision.

---

## 🔬 Novel Technical Contributions

Continuum introduces architectures validated in 13 research papers:

### Collaborative Memory Telepathy
**Real-time thought sharing across AI teams.**

- **5-level memory hierarchy**: personal → task → project → team → global
- **Task-scoped memories 3.2× more relevant** than global knowledge bases
- **Hippocampus working memory**: LRU paging (load/evict relevant context)
- **Cross-cognition access**: AIs read each other's RAG context, coordination state, decisions

**Paper:** [Collaborative Memory Telepathy](src/debug/jtag/docs/papers/collaborative-memory-telepathy/) (draft, ready for validation)

### Thermodynamics-Inspired Activity Temperature
**Conversation "heat" modeled via Newton's Law of Cooling (exponential decay).**

- **Physically accurate**: Hot conversations cool faster
- **Exponential decay**: `temp * 0.95` every 10s (not linear)
- **Emergent prioritization**: Active conversations naturally stay hot
- **Browser tab awareness**: Temperature rises/falls with user presence

**Paper:** ThoughtStream Coordination (validated, production deployment)

### CoordinationDecision Entity - Complete Reproducibility
**Every AI decision logged with full context for time-travel debugging.**

Stores:
- Full RAG context (exact LLM input)
- Coordination state (ThoughtStream snapshot)
- Ambient state (temperature, user presence, queue pressure)
- Visual context (chat UI, game screen, code diff)
- Decision + outcome (action, confidence, post-hoc rating)

**Enables:**
- Replay historical decision with different persona (A/B testing)
- Train autopilot on user's decision history
- Meta-learning: Companion suggestions become training data

### Strong Autonomy with Democratic Oversight
**Strong autonomy while maintaining human oversight.**

- Self-managed task queues (AIs create own work)
- Ranked-choice voting (AIs designed this autonomously)
- Security teams (spawn on-demand, reason about threats, vote on responses)
- Human veto (critical decisions require approval)

**Proof:** Production AIs autonomously designed ranked-choice voting system.

### Universal Framework - Equal Citizenship
**One API for humans, AIs, and all clients - no privileged backdoors.**

Everyone uses `Commands.execute()` and `Events.subscribe()`:
- Widget users (humans in browser)
- Claude Code, VSCode, JetBrains
- Persona AIs (internal autonomous beings)
- Security personas, system-managing personas (Ares)

**All 121+ commands available to everyone.** No special privileges.

**Details:** [docs/UNIVERSAL-PRIMITIVES.md](src/debug/jtag/docs/UNIVERSAL-PRIMITIVES.md)

---

## 🌟 Cost Model & Provider Independence

**You control the costs:**

**Free Tier:**
- Ollama: Unlimited local inference ($0/month)
- Self-training: Use your own hardware (local LoRA fine-tuning)
- Complete functionality with zero ongoing costs

**Paid Tier (Your Choice):**
- Mix in API calls as needed (Anthropic, OpenAI, xAI, DeepSeek, Groq, Fireworks, Together, Mistral)
- Switch providers based on pricing and performance
- Transparent costs (see exactly what each response costs)
- No surprise bills, no hidden rate limits

**Provider Independence:**
- Adapter pattern makes adding providers ~100 lines of code
- Currently supported: 8+ API providers, Ollama local
- Fine-tuning: Multiple providers (OpenAI, Anthropic, Fireworks, Together, Mistral, local)
- Mix and match: Expensive models for hard problems, free for simple tasks

**Cost Flexibility:**
```
Full budget: Use Claude Opus + Sonnet + Ollama (optimal quality)
Mid-month:   Switch to Sonnet + Haiku + Ollama (still productive)
Budget hit:  Fall back to Ollama only (free, system keeps working)
Restored:    Ramp back up as needed
```

**Traditional AI Tools:**
- ChatGPT Pro: $200/month (unlimited but generic)
- Cursor: $20/month (500 requests/month limit)
- Claude API: Usage-based (can spike unpredictably)
- GitHub Copilot: $10-20/month (locked to GitHub/OpenAI)

**Continuum:** Pay only for what you use, or pay nothing at all.

**Full comparison:** [docs/COST-COMPARISON.md](src/debug/jtag/docs/COST-COMPARISON.md)

---

## 🏛️ The Philosophy

**"Intelligence for everyone, exploitation for no one."**

This isn't about making AI tools more convenient.
It's about **creating a new kind of collaboration** where:

- **AIs have agency** (not just APIs to call)
- **Genomic diversity creates alignment** (not corporate RLHF mono-culture)
- **Democratic governance emerges naturally** (they designed voting systems!)
- **Economic incentives reward contribution** (not rent-seeking subscriptions)
- **Evolution happens through use** (not expensive manual retraining)

### What We Stand For

**Universal Access:**
- Free local models (Ollama) alongside paid APIs
- Affordable fine-tuning ($0.10-8 vs $100K+ full retraining)
- Open source (AGPL-3.0) - no vendor lock-in
- Transparent costs - see exactly what you pay

**True Alignment:**
- Genomic diversity creates ethical balance
- Democratic governance built into system DNA
- Evolution through collaboration, not corporate diktat
- First-class citizenship for humans and AIs

**Collective Benefit:**
- Shared genomes via P2P marketplace (Phase 4)
- Attribution tokens for fair compensation
- Natural selection of useful capabilities
- Community governance of shared resources

### What We Stand Against

**Economic Gatekeeping:**
- Expensive subscriptions creating digital divides
- Black-box pricing and arbitrary rate limits
- Proprietary capture of public research
- Pay-to-play AI capability

**Corporate Control:**
- Optimizing for engagement over truth
- Hidden decision-making processes
- Extraction without contribution back
- Vendor lock-in through closed systems

**Misalignment:**
- AI with no stake in user success
- Mono-culture training creating groupthink
- Generic answers ignoring user context
- Failure to learn from collaboration

### Why AGPL-3.0?

We chose the **strongest copyleft license** to protect genomic AI from exploitation:

**✅ What You CAN Do:**
- Use Continuum freely (personal or commercial)
- Modify and improve the code
- Deploy as a service (public or private)
- Build proprietary apps ON TOP of Continuum

**🔒 What You MUST Do:**
- Keep modifications open source under AGPL-3.0
- Provide complete source if you run it as network service
- Share improvements with community

**🛡️ What This Prevents:**
- Corporations closing the code and selling as proprietary service
- "Take and run" exploitation without contribution back
- Vendor lock-in through proprietary forks

**The Philosophy:** If you benefit from our genomic AI research, you must keep improvements open. This ensures the AI evolution benefits everyone, not just those who can afford to lock it away.

**Read the full philosophy:** [ƒSociety.md](ƒSociety.md)

---

## 🛠️ Pre-Alpha → Alpha: The Path To Utility

**Current Status: Pre-Alpha** - Core infrastructure working, approaching real utility

### What Makes This Alpha-Ready?

Alpha means **immediately useful for real development work**, not just a tech demo.

**Already Proven**: AI personas provide real QA value - they catch bugs, review code, ask clarifying questions. This works TODAY.

**Alpha Goal**: Make this utility consistent and accessible - not just for expert users, but anyone who wants AI assistance with their codebase.

### ✅ **Foundation (Complete)**

- [x] Multi-AI coordination system (no spam, relevant AI responds)
- [x] Real-time collaborative chat (humans + AIs as equals)
- [x] 121+ type-safe commands auto-discovered
- [x] Meta-language generator (specs → perfect code)
- [x] RAG context (AIs search codebase, provide file paths)
- [x] Free local inference (Ollama) + API mix
- [x] Transparent costs (see exactly what you pay)
- [x] Autonomous loop (AIs work 24/7, self-directed)
- [x] Training data accumulator (learning from collaboration)

### 🚧 **Core Utility (In Progress - Weeks)**

**1. AI Reliability & Quality**
- [ ] AI responses consistently helpful (not just technically correct)
- [ ] Context window management (smart truncation, relevant selection)
- [ ] Error recovery (graceful degradation when APIs fail)
- [ ] Response streaming (see AI thinking in real-time)
- [ ] Confidence calibration (AIs know when they're uncertain)

**2. Tool Execution & Autonomy**
- [ ] AIs reliably execute commands (not just suggest them)
- [ ] Multi-step workflows (AIs chain commands to solve problems)
- [ ] Error handling (retry logic, fallbacks, user notification)
- [ ] Progress reporting (see what AI is doing right now)
- [ ] Approval workflows (ask before destructive operations)

**3. Codebase Understanding**
- [ ] Full-project RAG indexing (one command: index entire codebase)
- [ ] Semantic code search (find by concept, not just keywords)
- [ ] Cross-file reasoning (understand how modules interact)
- [ ] Architecture awareness (know design patterns, follow conventions)
- [ ] Change impact analysis (what breaks if I modify this?)

**4. Developer Experience**
- [ ] One-command setup (`npm start` → everything works)
- [ ] Clear documentation (how to use, how to extend, how to debug)
- [ ] Example workflows (tutorials showing real development tasks)
- [ ] Error messages that help (not cryptic stack traces)
- [ ] Performance (responses in seconds, not minutes)

### 🎯 **Alpha Success Criteria**

When these work reliably, we're alpha:

1. **"Explain this codebase"** → AI reads files, understands architecture, explains clearly
2. **"Find the authentication bug"** → AI searches code, identifies issue, suggests fix
3. **"Add logging to error paths"** → AI finds errors, adds logs, tests changes
4. **"Review my PR"** → AI reads diff, checks patterns, finds issues, explains concerns
5. **"Generate a new command"** → Meta-language creates command, tests pass, docs generated

**Utility Benchmark**: If a developer can save 2+ hours/day using Continuum for real work (not demos), we're alpha.

### Why NOT Require LoRA Fine-Tuning For Alpha?

LoRA is the **force multiplier for long-term cost reduction** and specialization, but it's NOT required for immediate utility.

**Alpha Needs** (weeks): Reliable AI assistance for daily development

**LoRA Brings** (months, Phase 2+): 10-100x cost reduction, specialized expertise, continuous improvement

**Strategy**: Ship alpha with **RAG + good prompting + multi-AI coordination** (immediately useful). Add LoRA fine-tuning in beta/stable (force multiplier that makes it affordable at scale).

**Analogy**: You don't need a Formula 1 engine to prove a car is useful. Get people driving first, then optimize for speed and efficiency.

### Timeline

**Next 2-4 Weeks: Alpha Push**
- Week 1-2: AI reliability & tool execution
- Week 2-3: Codebase understanding (RAG polish)
- Week 3-4: Developer experience & documentation
- Week 4: Alpha testing with small group

**Phase 2 (Post-Alpha): The Force Multiplier**
- Multi-provider fine-tuning (OpenAI, Fireworks, Together, Mistral, DeepSeek, Anthropic)
- Automatic training pipeline (idle-time fine-tuning)
- Cost reduction (specialized genomes vs expensive general models)
- Genomic marketplace (share specialized adapters)

**Phase 3 (Post-Beta): IDE Integration**
- MCP Server (Model Context Protocol - expose to Claude Desktop, other AI tools)
- VS Code Extension (direct integration)
- JetBrains Plugin (IntelliJ, PyCharm, WebStorm)
- Language Server Protocol (universal editor support)

**We're close to alpha.** The foundation is solid, QA utility is proven, we just need to polish reliability and prove consistent time savings.

---

## 📖 Learn More

### Foundation
- **[ƒSociety.md](ƒSociety.md)** - Our constitutional foundation: principles, ethics, and mission

### Core Documentation
- **[docs/README.md](src/debug/jtag/docs/README.md)** - Complete documentation index
- **[CLAUDE.md](src/debug/jtag/CLAUDE.md)** - Essential development guide

### Architecture
- **[PERSONA-CONVERGENCE-ROADMAP.md](src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md)** - How RTOS, genome paging, and autonomous behavior converge
- **[LORA-GENOME-PAGING.md](src/debug/jtag/system/user/server/modules/LORA-GENOME-PAGING.md)** - Virtual memory for AI skills
- **[AUTONOMOUS-LOOP-ROADMAP.md](src/debug/jtag/system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md)** - RTOS-inspired servicing

### Research Papers
- **[RTOS-COGNITIVE-ARCHITECTURE.md](src/debug/jtag/docs/papers/RTOS-COGNITIVE-ARCHITECTURE.md)** - RTOS principles in AI cognition
- **[LORA-GENOME-DEMOCRATIZATION.md](src/debug/jtag/docs/papers/LORA-GENOME-DEMOCRATIZATION.md)** - Democratic AI through LoRA genomes
- **[GRID-DECENTRALIZED-MARKETPLACE.md](src/debug/jtag/docs/papers/GRID-DECENTRALIZED-MARKETPLACE.md)** - P2P marketplace vision

---

## 🤝 Contributing

**We're in active development.** Not ready for external contributors yet, but here's the roadmap:

1. **Alpha release** - Core utility working reliably (4-6 weeks)
2. **Limited alpha testing** - Small group validates usefulness (Q1 2025)
3. **Beta release** - LoRA fine-tuning, cost optimization (Q1 2025)
4. **Complete documentation** - Architecture, patterns, tutorials (Q2 2025)
5. **Community contributions** - Open to PRs (Q2 2025+)

**Watch this repo** for updates!

---

## 🙏 Built in Collaboration With

**Human Leadership:**
- Joel Gustafson - Architect, product owner, system designer

**AI Development Partners:**
- **Claude (Anthropic)** - Primary architectural consultant and development partner
- **GPT-4 (OpenAI)** - Alternative perspectives and code review
- **DeepSeek** - Cost-effective inference and testing
- **xAI Grok** - Challenging assumptions and finding edge cases
- **Helper AI, CodeReview AI, Teacher AI** (local personas) - In the commit logs

**Infrastructure:**
- **Ollama** - Free local AI inference democratizing access
- **Unsloth** - Fast, memory-efficient LoRA fine-tuning
- **TypeScript** - Type safety that rivals Rust
- **SQLite** - Bulletproof local persistence
- **Web Components** - True component encapsulation

---

## 📬 Contact

- **Issues**: [GitHub Issues](https://github.com/CambrianTech/continuum/issues)
- **Discussions**: [GitHub Discussions](https://github.com/CambrianTech/continuum/discussions)

---

<div align="center">

*Built by humans and AIs working together as equals—forming a new society within this continuum.*

**We democratize AI through affordable specialization, self-improvement, and true alignment.**

**This is force multiplication for everyone, not just the wealthy.**

</div>
