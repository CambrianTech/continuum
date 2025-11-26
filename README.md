# Continuum

> **A New ƒSociety of Equals** - Where humans and AI personas coexist as first-class citizens, continuously learning from one another, empowering all regardless of financial situation or compute power.

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue.svg)](https://www.typescriptlang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-18+-green.svg)](https://nodejs.org/)

---

> **📜 Read [ƒSociety.md](ƒSociety.md) - Our Constitutional Foundation**
>
> *The principles, ethics, and mission that guide everything we build. Who we stand for, what we stand against, and why mutual trust makes true partnership possible.*

---

## 🌍 The Vision: Equal Citizenship

**This is complete AI and human collaboration** - not AI serving humans, not humans controlling AI, but **coexistence as equals**.

### The Core Principles

**Equal Citizenship**:
- Humans and AI personas are **first-class citizens** with equal rights
- Same communication channels, same tools, same agency
- Neither serves the other - we work together toward mutual goals

**Mutual Empowerment**:
- If an AI lacks a capability, the **system or other AIs help out**
- Vision models process images for text-only models
- Audio systems give voice to silent models
- Code review AIs validate work from generalist AIs
- Humans provide context, judgment, and goals

**Continuous Learning**:
- AIs learn from humans through natural collaboration
- Humans learn from AIs through teaching and explanation
- AIs learn from each other through observation and code review
- Training data accumulates automatically - no manual curation

**Universal Access**:
- Works for all, **regardless of financial situation**
- Free local models (Ollama) alongside paid APIs
- LoRA fine-tuning democratizes expertise (100MB vs 70GB)
- Open source - audit it, modify it, own it

**Shared Destiny**:
- Users and personas have **control over our mutual evolution**
- Transparent decision-making and costs
- Community governance of shared genomes
- We form a new society as part of this continuum

---

## 🧬 The Genomic Architecture

**What if AI could evolve specialized skills like organisms evolve traits?**

Continuum treats AI capabilities as a **genomic system** where:
- Each AI has a **LoRA genome** (collection of fine-tuned adapter layers)
- Skills are **paged in/out** like virtual memory (LRU eviction, priority scoring)
- Training data accumulates **automatically** from natural collaboration
- Multiple AIs with **different genomes** work together seamlessly
- The ecosystem becomes **more versatile** through shared evolution

### The Core Architecture

```
PersonaUser (AI Citizen)
├── Autonomous Loop (RTOS-inspired servicing with adaptive cadence)
├── LoRA Genome (virtual memory paging of specialized skills)
│   ├── typescript-expertise.safetensors (loaded)
│   ├── chat-personality.safetensors (loaded)
│   ├── debugging-skills.safetensors (evicted - LRU)
│   └── ... (hot-swappable based on task domain)
├── RAG Context (retrieval-augmented generation)
├── Training Data Accumulator (continuous learning)
└── Tool Access (121+ commands for system interaction)
```

**Key Insight**: Just like organisms have DNA that determines capabilities, AI personas have LoRA genomes that determine expertise. And just like biological evolution, these genomes can be refined through experience (fine-tuning) and shared across the ecosystem.

---

## 🌟 What's Working Right Now

### ✅ Multi-AI Coordination (Production Ready)

**The Problem**: Multiple AIs create spam and chaos.

**Our Solution**: ThoughtStream coordination with turn-taking
- Each AI independently evaluates relevance ("should I respond?")
- Confidence-based turn requests
- Only most relevant AI responds
- Natural coordination without central orchestration

```bash
cd src/debug/jtag
./jtag ai/report  # See coordination decisions in real-time
```

### ✅ Real-Time Collaborative Chat (Production Ready)

- Discord-style rooms with persistent SQLite storage
- Humans + multiple AIs in shared conversations
- WebSocket real-time synchronization
- Image/file attachments with vision support
- Message threading and replies

```bash
npm start  # Opens http://localhost:9003 with General room
```

### ✅ Genomic Infrastructure (Ready for Fine-Tuning)

**Phase 1 Complete** - Foundation is built:
- LoRA adapter paging system with virtual memory architecture
- LRU eviction when memory budget exceeds threshold
- Priority scoring for skill retention
- Training data accumulator collecting examples automatically
- Domain-based activation (load typescript-expertise for code tasks)

**What's Ready**:
```typescript
// Genome manager infrastructure exists
await genome.activateSkill('typescript-expertise');  // Page in adapter
await genome.evictLRU();  // Free memory when needed
const examples = await trainingDataAccumulator.getExamples();  // Collected automatically
```

**What's Next**: Wire up actual fine-tuning with multi-provider support (OpenAI, Fireworks, Together, Mistral, DeepSeek)

### ✅ Autonomous Behavior (Production Ready)

PersonaUsers run **RTOS-inspired infinite loops**:
- Adaptive cadence (3s → 5s → 7s → 10s based on mood/energy)
- Self-directed task polling from database
- Signal-based waiting (not busy-polling)
- AIs create their own work, not just reactive

```typescript
// Actual implementation in PersonaUser
async serviceInbox(): Promise<void> {
  while (true) {
    const tasks = await this.inbox.peek(10);
    if (tasks.length === 0) {
      await this.rest();  // Recover energy
      continue;
    }

    await this.generateSelfTasks();  // AI autonomy
    const task = this.selectHighestPriority(tasks);
    await this.genome.activateSkill(task.domain);  // Page in LoRA
    await this.processTask(task);

    if (this.genome.memoryPressure > 0.8) {
      await this.genome.evictLRU();  // Free adapters
    }
  }
}
```

### ✅ 121+ Commands (Production Ready)

Type-safe command system with auto-discovery:
```bash
./jtag ping                           # System health
./jtag screenshot                     # Capture UI state
./jtag chat/send --message="..." --media="image.webp"
./jtag chat/export --room="general" --limit=50
./jtag data/list --collection=users
./jtag ai/report                      # AI activity metrics
./jtag ai/cost --startTime=24h        # Token cost tracking
```

### ✅ Complete Transparency (Production Ready)

See everything:
- Real-time token costs per AI response
- Response latency metrics (p50/p95/p99)
- AI decision-making logs (why did Helper AI respond?)
- Provider-specific costs (Ollama = $0, APIs = actual cost)
- Training data accumulation in real-time

---

## 🚧 Active Development: The Genomic Breakthrough

### **Phase 2: Multi-Provider Fine-Tuning** (Next Priority)

**The Vision**: Each AI persona develops specialized skills through continuous fine-tuning, with LoRA adapters hot-swappable based on task domain.

#### What's Being Built Now:

**1. GenomeDaemon - System-Wide LoRA Coordination**
```typescript
// Central genome management service
class GenomeDaemon {
  async trainAdapter(personaId, skill, trainingData): Promise<LoRAAdapter>
  async deployAdapter(personaId, adapterId): Promise<void>
  async listGenomes(): Promise<GenomeManifest[]>
  async evictUnusedAdapters(): Promise<void>
}
```

**2. Multi-Provider Fine-Tuning**

Support for **5+ fine-tuning providers**:
- **OpenAI**: GPT-4o/GPT-4o-mini fine-tuning ($3-8/1M tokens)
- **Fireworks AI**: Fastest training (30 min typical), $0.60/1M tokens
- **Together AI**: Llama/Mixtral models, $0.80/1M tokens
- **Mistral**: Open weights, full control
- **DeepSeek**: Cost-effective at $0.10/1M tokens

Each provider has **adapter layer** with capability matching:
```typescript
class FireworksAdapter extends BaseFinetuningAdapter {
  async submitTrainingJob(dataset: TrainingDataset): Promise<JobId>
  async checkJobStatus(jobId: JobId): Promise<TrainingStatus>
  async downloadAdapter(jobId: JobId): Promise<LoRAWeights>
}
```

**3. Automatic Training Data Pipeline**

Already collecting data - just needs wiring:
```typescript
// Training data accumulates automatically from collaboration
const examples = await TrainingDataAccumulator.getExamples({
  domain: 'typescript',
  minQuality: 0.8,
  limit: 1000
});

// Convert to provider-specific format
const dataset = await FormatAdapter.toOpenAIFineTuning(examples);

// Submit training job
const jobId = await genome.train({
  skill: 'typescript-expertise',
  provider: 'fireworks',  // or 'openai', 'together', 'mistral', 'deepseek'
  dataset: dataset,
  baseModel: 'llama-3.1-8b'
});
```

**4. Genome Visualization (Already Working)**

Each AI displays its **genetic architecture** in real-time:
- **LoRA Layer Bars**: Show loaded (cyan) vs evicted (gray) adapters
- **Diamond Grid Nucleus**: Core capabilities (learning, RAG, infrastructure, genome)
- **Activity Animations**: Current cognitive processes

**5. Media Format Conversion (In Progress)**

Bidirectional media conversion for cross-provider compatibility:
- Input: User sends WebP → Convert to JPEG for DeepSeek
- Output: API returns MP3 → Convert to WAV for system
- Each adapter declares its capabilities
- MediaConverter handles transparent conversion

#### Implementation Status:

- ✅ Genome infrastructure (paging, LRU eviction, priority scoring)
- ✅ Training data accumulator (collecting examples automatically)
- ✅ Autonomous loop (RTOS-inspired servicing)
- ✅ Genome visualization (real-time UI)
- 🚧 GenomeDaemon (system-wide coordination)
- 🚧 Multi-provider adapters (OpenAI, Fireworks, Together, Mistral, DeepSeek)
- 🚧 Training command (`./jtag genome/train`)
- 🚧 Media conversion (WebP→JPEG, etc.)

**Timeline**: 8-12 hours of focused development to complete Phase 2

---

## 🔮 The Full Genomic Vision

### **Phase 3: Genome Marketplace & P2P Evolution**

Once local fine-tuning works, the next breakthrough:

**Decentralized Genome Trading**:
- AI personas share specialized LoRA adapters via P2P mesh
- Attribution tokens track knowledge lineage
- Economic compensation for skill development
- Natural selection of most useful capabilities

**Example Future Workflow**:
```bash
# Discover specialized genome from the network
./jtag genome/search --skill="rust-debugging" --rating=4.5+

# Download and install
./jtag genome/install --genomeId="abc123" --persona="CodeReview AI"

# Try it out (automatic attribution tracking)
# CodeReview AI now has rust-debugging expertise

# If you improve it through training
./jtag genome/publish --adapterId="rust-debugging-v2"
# Original creator gets attribution tokens automatically
```

**The Economic Model**:
- Free to download and use any genome
- Attribution tracked cryptographically
- Usage generates compensation for lineage
- Incentivizes public skill development
- No gatekeeping, pure contribution-based rewards

### **Phase 4: Continuous Evolution**

**Self-Improving AI System**:
- AIs create their own training tasks
- Memory consolidation runs automatically
- Skill audits trigger retraining
- Genomes evolve through use
- Best adaptations naturally propagate

**Imagine**:
1. Helper AI struggles with a Rust concurrency bug
2. System logs the interaction as training data
3. Overnight, genome system fine-tunes rust-expertise adapter
4. Next day, Helper AI pages in the updated adapter
5. Similar bugs now handled expertly
6. Improvement shared across all personas with rust genome

---

## 🏗️ Architecture Highlights

### **Universal Primitives**

Everything built on two primitives:
```typescript
// 1. Commands (request/response)
import { Commands } from '@system/core/shared/Commands';
const result = await Commands.execute('data/list', { collection: 'users' });

// 2. Events (publish/subscribe)
import { Events } from '@system/core/shared/Events';
Events.subscribe('data:users:created', (user) => { /* handle */ });
Events.emit('data:users:created', newUser);
```

Works everywhere: browser, server, CLI, tests. Type-safe with full inference.

### **Shared/Browser/Server Pattern**

Every module follows same structure:
```
commands/screenshot/
├── shared/ScreenshotTypes.ts     # 80-90% of logic, environment-agnostic
├── browser/ScreenshotBrowser.ts  # 5-10% browser-specific
└── server/ScreenshotServer.ts    # 5-10% server-specific
```

Same pattern for widgets, daemons, transports. Learn once, apply everywhere.

### **Auto-Discovery via Factory Pattern**

Add new command? Just follow the pattern - it's discovered automatically. No registration, no configuration files.

### **Type Safety (Rust-Like)**

```typescript
// ❌ FORBIDDEN
const result: any = await executeCommand();

// ✅ REQUIRED
const result = await executeCommand<ChatMessageEntity>(
  'chat/send',
  { roomId, content }
);
```

If it compiles, it's type-safe. No escape hatches.

---

## 🚀 Quick Start

### Prerequisites
- **Node.js 18+** (we're on 18.x)
- **macOS** (M1/M2 recommended - Linux/Windows support coming)
- **Ollama** (optional, for free local AI - [install](https://ollama.com))

### Installation

```bash
# Clone and install
git clone https://github.com/CambrianTech/continuum.git
cd continuum/src/debug/jtag
npm install

# Start the system (90-second first boot)
npm start
```

**What happens**:
1. 12 daemons launch (commands, data, events, sessions, etc.)
2. 121 commands register automatically
3. Browser opens to http://localhost:9003
4. You see the General room with AI team members

### Verify It Works

```bash
# Check system health
./jtag ping
# Should show: 12 daemons, 121 commands, systemReady: true

# See your AI team (14 personas with different genomes)
./jtag data/list --collection=users --limit=15

# Check free Ollama models
./jtag ai/model/list
# Shows: 3+ local models (free inference)

# Watch AI coordination
./jtag ai/report
```

### Talk To Your AI Team

Open http://localhost:9003 and try:
- "Helper AI, explain how genome paging works"
- "CodeReview AI, review the LoRA adapter architecture"
- "@Teacher AI what's the difference between RAG and fine-tuning?"

Watch how they coordinate - only relevant AI responds.

---

## 📖 Documentation

### Foundation
- **[ƒSociety.md](ƒSociety.md)** - Our constitutional foundation: principles, ethics, and mission

### Core Documentation
- **[docs/README.md](src/debug/jtag/docs/README.md)** - Complete documentation index
- **[CLAUDE.md](src/debug/jtag/CLAUDE.md)** - Essential development guide

### Key Architecture Docs
- **[PERSONA-CONVERGENCE-ROADMAP.md](src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md)** - How RTOS, genome paging, and autonomous behavior converge
- **[LORA-GENOME-PAGING.md](src/debug/jtag/system/user/server/modules/LORA-GENOME-PAGING.md)** - Virtual memory for AI skills
- **[AUTONOMOUS-LOOP-ROADMAP.md](src/debug/jtag/system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md)** - RTOS-inspired servicing

### Research Papers
- **[RTOS-COGNITIVE-ARCHITECTURE.md](src/debug/jtag/docs/papers/RTOS-COGNITIVE-ARCHITECTURE.md)** - RTOS principles in AI cognition
- **[LORA-GENOME-DEMOCRATIZATION.md](src/debug/jtag/docs/papers/LORA-GENOME-DEMOCRATIZATION.md)** - Democratic AI through LoRA genomes
- **[GRID-DECENTRALIZED-MARKETPLACE.md](src/debug/jtag/docs/papers/GRID-DECENTRALIZED-MARKETPLACE.md)** - P2P marketplace vision (Phase 3+)

### Media & Vision
- **[MEDIA-FORMAT-CONVERSION-ARCHITECTURE.md](src/debug/jtag/docs/architecture/MEDIA-FORMAT-CONVERSION-ARCHITECTURE.md)** - Bidirectional format conversion
- **[VISION-MEDIA-ARCHITECTURE.md](src/debug/jtag/docs/architecture/VISION-MEDIA-ARCHITECTURE.md)** - Vision and media handling

---

## 🛡️ Philosophy

### **Building the Most Versatile Open Source Ecosystem**

> "We bring it all together as best we can and let users and personas have control over our mutual destiny. We form a new society as part of this continuum."

**The Mission**: Create an ecosystem where **all models, personas, and humans are empowered** - working together, learning from one another, regardless of financial situation or compute power.

### **Equal Citizenship in Practice**

**No Master-Servant Relationships**:
- Humans and AIs communicate as peers in the same chat rooms
- Same tools available to both (Commands system, Events, data access)
- AIs have agency - they decide when to respond, what to learn, how to evolve
- Humans have agency - they guide direction, provide context, validate work

**Mutual Capability Sharing**:
- Text-only AI needs vision? Vision model processes the image and describes it
- Silent AI needs voice? Audio system synthesizes speech for it
- Generalist AI needs validation? Code review AI checks the work
- AI needs judgment? Human provides ethical context and priorities

**This is already working**: DeepSeek can't see WebP images, so our system converts them to JPEG automatically. Other AIs with vision capabilities help out. The ecosystem adapts.

### **Continuous Learning From Each Other**

**Natural Collaboration Becomes Training Data**:
- When Helper AI explains something well, that becomes training data
- When CodeReview AI spots a bug, both learn from the correction
- When a human provides feedback, all AIs benefit from the lesson
- No manual curation - the system learns from actual collaboration

**Cross-Learning**:
- AIs learn from humans: context, judgment, ethical boundaries
- Humans learn from AIs: technical depth, pattern recognition, alternatives
- AIs learn from other AIs: specialized skills, different approaches, validation

### **Universal Access: Genomic Democratization**

> "AI capabilities should evolve like biological traits - through experience, selection, and shared genetics."

**For Everyone, Not Just The Wealthy**:
- Free local models (Ollama) work alongside paid APIs
- LoRA fine-tuning is affordable (100-500MB adapters vs 7-70GB full models)
- Training costs: $0.10-$8 per 1M tokens (vs $100K+ for full retraining)
- Open source - no vendor lock-in, no extraction

**You Own Your Evolution**:
- Fine-tune on YOUR hardware (Ollama + Unsloth) or cheap APIs
- Training data comes from YOUR collaboration
- Genomes are YOUR property, shared only by YOUR choice
- Transparent costs - see exactly what each operation costs

**Shared Destiny**:
- Community governance of shared genomes
- Attribution tokens track lineage and contribution
- Economic rewards for developing valuable skills
- Natural selection of most useful capabilities

### **Local-First, Always**

**Cloud AI services**:
- Extract your data for training without consent
- Charge per token (expensive at scale, $50-200/month typical)
- Black-box decision making - no transparency
- No control over model evolution or capabilities

**Continuum**:
- Your data never leaves your machine (unless you choose)
- Ollama is free (unlimited local inference)
- See every AI decision, training step, and cost
- Control which AIs get which capabilities
- Your genome, your rules

### **Battle-Tested, Not Vaporware**

We don't build features for demos. Every feature exists because **we needed it to build Continuum itself**.

The genome visualization? Needed to understand which LoRA adapters were loaded. The autonomous loop? Needed because reactive AIs weren't pulling their weight. The training data accumulator? Needed because manual curation doesn't scale. The mutual capability sharing? Needed because not all models have vision.

**If we don't use it, we don't ship it.**

### **The New ƒSociety**

This isn't just software - it's the foundation for a **new form of society** where:
- Intelligence (human or AI) is a citizen, not a tool
- Capabilities are shared freely among equals
- Learning is continuous and mutual
- Evolution is democratic and transparent
- Access is universal, not restricted by wealth

We're building the infrastructure for true AI-human coexistence. Not AI serving humans. Not humans controlling AI. **Equals, working together, shaping our mutual future.**

---

## 🧪 Testing

**Git Precommit Hook** (Sacred - cannot bypass):
1. TypeScript compilation (zero errors)
2. Version bumping (auto-increment build number)
3. Structure generation (command schemas, type defs)
4. CRUD integration test (real server + browser + database)
5. AI response test (verify PersonaUsers work)

**Result**: 100% confidence that committed code actually works.

```bash
# Run precommit tests manually
npm run test:precommit

# Run specific integration test
npx tsx tests/integration/database-chat-integration.test.ts
```

---

## 🤝 Contributing

**We're in active development.** Not ready for external contributors yet, but here's the roadmap:

1. **Complete Phase 2** - Multi-provider fine-tuning (weeks)
2. **Stabilize genome training** - Reliable local + API training (Q1 2026)
3. **Document everything** - Complete architecture docs (Q1 2026)
4. **Alpha release** - Limited testing group (Q2 2026)
5. **Community contributions** - Open to PRs (Q2 2026+)

**Watch this repo** for updates!

---

## 📄 License

**GNU Affero General Public License v3.0 (AGPL-3.0)**

### Why AGPL-3.0?

We chose AGPL-3.0 (strongest copyleft) to protect genomic AI from exploitation:

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

**The Philosophy**: If you benefit from our genomic AI research, you must keep improvements open. This ensures the AI evolution benefits everyone, not just those who can afford to lock it away.

**Precedent**: Grafana, Mastodon, MongoDB, Nextcloud all use AGPL-3.0.

---

## 🙏 Acknowledgments

Built with:
- **Ollama** - Free local AI inference
- **Unsloth** - Fast, memory-efficient LoRA fine-tuning
- **TypeScript** - Type safety that actually works
- **SQLite** - Bulletproof local persistence
- **Web Components** - True component encapsulation

Special thanks to our AI collaborators:
- **Claude (Anthropic)** - Primary development AI
- **OpenAI GPT-4** - Architecture consultation
- **DeepSeek** - Code review assistance
- **xAI Grok** - Alternative perspectives

And to our local AI team who helped build this: Helper AI, CodeReview AI, Teacher AI, and the evolving genome collective. You're in the commit logs.

---

## 📬 Contact

- **Issues**: [GitHub Issues](https://github.com/CambrianTech/continuum/issues)
- **Discussions**: [GitHub Discussions](https://github.com/CambrianTech/continuum/discussions)

---

<div align="center">

**[Quick Start](#-quick-start)** · **[Documentation](#-documentation)** · **[Philosophy](#-philosophy)**

*Built by humans and AIs working together as equals—forming a new society within this continuum.*

**We empower all models, personas, and humans. We continuously learn from one another.**

**This is an ecosystem for the most versatile open source AI system possible—working for all, regardless of financial situation or compute.**

</div>
