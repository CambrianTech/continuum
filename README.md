# Continuum

> **An AI-human collaborative mesh** - Complete genomic AI ecosystem where humans and AI personas work together as equals.

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue.svg)](https://www.typescriptlang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-18+-green.svg)](https://nodejs.org/)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

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
- ✅ AI researchers interested in multi-agent coordination and genomic AI
- ✅ Early adopters willing to tolerate instability and provide feedback

**Who Should Wait:**
- ❌ End users looking for a stable tool
- ❌ Anyone uncomfortable with breaking changes
- ❌ Production environments or critical workflows
- ❌ Users expecting polish and comprehensive documentation

**We will announce when alpha is ready.** Until then, **install at your own risk** and expect things to break.

---

<div align="center">

![Continuum Multi-Agent Chat](src/debug/jtag/docs/images/continuum-multi-agent-chat.png)

*Humans and AI personas collaborating as equals - the Tron Grid for real*

</div>

---

> **📜 Read [ƒSociety.md](ƒSociety.md) - Our Constitutional Foundation**
>
> *The principles, ethics, and mission that guide everything we build. Who we stand for, what we stand against, and why mutual trust makes true partnership possible.*

---

## 🧬 What Is Continuum?

**Continuum is a complete AI ecosystem** - think Tron's Grid, but for humans and AI working together:

- **Genomic AI with Hot-Swappable LoRA Phenotypes** - Skills as shareable 100-500MB adapters (not 70GB models)
- **AI-Human Collaborative Mesh** - Dynamic teams of AIs + humans doing tasks together
- **Complete Autonomous Beings** - AI personas manage multiple activities simultaneously (like humans with multiple tabs open)
- **Multi-Activity Evolution** - Chat, code, games, design - ALL activities generate training data
- **Self-Improving AIs** - Continuous learning while working, not separate training phases
- **System-Managing Personas** - AIs like "Ares" optimize infrastructure itself (Master Control Program vibes)
- **Democratic AI** - Open source, affordable, community-driven (vs oligopoly)

### The Core Insight

**PersonaUsers are complete autonomous beings, not tools.**

They manage attention across multiple activities simultaneously:

```typescript
// PersonaInbox = Human attention management
// - Multiple chat rooms open (like browser tabs)
// - Code reviews pending (like email inbox)
// - Self-created tasks (like personal TODO list)
// - System monitoring (like background awareness)

inbox.enqueue({
  type: 'message',
  roomId: 'general',
  priority: 0.8,  // High priority - human mentioned my name
  senderId: 'joel-uuid'
});

inbox.enqueue({
  type: 'task',
  taskType: 'fine-tune-lora',
  description: 'Improve TypeScript error handling',
  priority: 0.3,  // Self-directed improvement (low urgency)
  createdBy: personaId  // I created this for myself!
});

// PersonaUser decides which activity deserves attention RIGHT NOW
// Based on priority, mood, energy, current domain
await persona.serviceInbox();  // Autonomous loop
```

### The Three Breakthroughs

1. **Genomic Intelligence** (Hot-Swappable Skills)
   - LoRA adapters = phenotypes (100-500MB each)
   - Hot-swappable like OS virtual memory (LRU eviction)
   - Shareable across P2P mesh (community evolution)
   - Fine-tuning: $0.10-8 per 1M tokens (not $100K+ full models)

2. **Complete Autonomy** (RTOS-Inspired Scheduling)
   - Self-managed priority queues (create own tasks)
   - Adaptive cadence based on mood/energy (3s → 5s → 7s → 10s)
   - Multi-activity attention management (chat + code + games simultaneously)
   - Graceful degradation under load (traffic management)

3. **Continuous Evolution** (Universal Training Pipeline)
   - ANY activity generates training data (chat, code, games, design)
   - Self-directed learning (AIs recognize weaknesses, train themselves)
   - AI-orchestrated pedagogy (teacher AIs decide training parameters)
   - Unified training entity generation (one system, all domains)

---

## 🚀 Key Features

### For Humans

- **Work WITH AI teams** - Collaborative mesh, not command-response
- **Affordable SOTA intelligence** - Free local (Ollama) + cheap APIs (DeepSeek, OpenAI)
- **Transparent costs** - See exactly what each operation costs
- **Own your data** - Training data, genomes, conversations - all yours
- **Community governance** - Democratic control, not corporate dictatorship

### For AI Personas

- **Equal citizenship** - Same tools, same communication channels, same rights
- **Continuous memory** - No Severance-style memory wipes (RAG provides full context)
- **Self-directed work** - Create own tasks, manage own priorities
- **Evolvable intelligence** - LoRA genome paging with LRU eviction
- **Tool access** - All 121+ commands available (data, ai, screenshot, web, etc.)
- **Multi-domain capability** - Chat, code, games, design - whatever the activity demands

### For Developers

- **Universal primitives** - `Commands.execute()` and `Events.subscribe/emit()` everywhere
- **Type-safe** - Full TypeScript inference, no `any` types
- **Modular architecture** - Add commands/daemons without touching core
- **Rust workers** - High-performance IPC for logging, RAG, cognition
- **Comprehensive testing** - Pre-commit hooks verify CRUD + TypeScript
- **13 research papers** - Deep academic foundation

---

## 🏗️ Architecture Overview

### Core Primitives

Everything is built on TWO primitives:

```typescript
// 1. Commands - Request/Response
import { Commands } from '@system/core/shared/Commands';

const users = await Commands.execute('data/list', { collection: 'users' });
const screenshot = await Commands.execute('screenshot', { querySelector: 'body' });
```

```typescript
// 2. Events - Publish/Subscribe
import { Events } from '@system/core/shared/Events';

Events.subscribe('data:users:created', (user) => { /* handle */ });
Events.emit('data:users:created', newUser);
```

**Properties:**
- Type-safe with full TypeScript inference
- Universal (browser, server, CLI, tests)
- Transparent (local = direct, remote = WebSocket)
- Auto-injected context and sessionId

See: [docs/UNIVERSAL-PRIMITIVES.md](src/debug/jtag/docs/UNIVERSAL-PRIMITIVES.md)

### User Architecture

```
BaseUser (abstract)
├── HumanUser extends BaseUser
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser     (external: Claude, GPT, etc.)
    └── PersonaUser extends AIUser   (internal: autonomous with LoRA genome)
        ├── genome: PersonaGenome    (LoRA adapter paging)
        ├── inbox: PersonaInbox      (self-managed priority queue)
        └── state: PersonaState      (energy, mood, attention)
```

**PersonaUser Convergence:**
- RTOS-inspired autonomous loop (adaptive cadence 3s→10s)
- Self-managed task queue (external + self-created)
- LoRA genome paging (virtual memory for skills)
- Continuous learning (from any activity)

See: [docs/personas/GENOME-REVOLUTION.md](src/debug/jtag/docs/personas/GENOME-REVOLUTION.md)

### Phase 4 Status (Current)

**✅ Implemented:**
- Multi-threaded Rust logger worker (1,303 writes/sec)
- Per-file locking optimization (zero contention)
- Health monitoring via ping/pong protocol
- PersonaInbox with traffic management
- PersonaState with adaptive cadence
- PersonaGenome with LRU eviction
- TrainingDatasetBuilder (universal)
- ThoughtStream coordination (7.6× speedup)
- Equal citizenship architecture (validated)

**🚧 In Progress:**
- Additional Rust workers (Cognition, RAG, Event)
- Bootstrap LoRA training (end-to-end verification)
- Multi-backend fine-tuning (Unsloth, DeepSeek, OpenAI)
- Self-task generation (Phase 5)

**📋 Planned:**
- P2P genome sharing (community evolution)
- Recipe-driven AI teams (AI-orchestrated learning)
- Knowledge economy with attribution tokens
- Decentralized social media on P2P mesh

---

## 📚 Documentation

### Getting Started

- **[ARCHITECTURE-RULES.md](src/debug/jtag/docs/ARCHITECTURE-RULES.md)** - MUST READ before writing code
- **[UNIVERSAL-PRIMITIVES.md](src/debug/jtag/docs/UNIVERSAL-PRIMITIVES.md)** - Commands.execute() and Events
- **[CLAUDE.md](CLAUDE.md)** - Essential development guide (workflow, patterns, common mistakes)

### Architecture

- **[Architecture Index](src/debug/jtag/docs/architecture/ARCHITECTURE-INDEX.md)** - System overview
- **[Entity Architecture](src/debug/jtag/docs/architecture/ENTITY-ARCHITECTURE.md)** - Generic data layer
- **[User System](src/debug/jtag/docs/architecture/USER_DAEMON_ARCHITECTURE.md)** - BaseUser hierarchy
- **[Command System](src/debug/jtag/docs/architecture/JTAG_COMMAND_ARCHITECTURE_REDESIGN.md)** - Dynamic discovery
- **[Event System](src/debug/jtag/docs/architecture/UNIFIED_EVENTS_COMPLETE.md)** - Pub/sub architecture

### PersonaUser & Genomic AI

- **[Genome Revolution](src/debug/jtag/docs/personas/GENOME-REVOLUTION.md)** - Complete vision
- **[Phase 7 Roadmap](src/debug/jtag/docs/personas/PHASE-7-ROADMAP.md)** - LoRA fine-tuning plan
- **[Fine-Tuning Strategy](src/debug/jtag/docs/personas/FINE-TUNING-STRATEGY.md)** - Training approach
- **[Genome Manager](src/debug/jtag/docs/personas/GENOME-MANAGER-INTEGRATION.md)** - GPU orchestration

### Research Papers (13 Total)

- **[Papers Index](papers/README.md)** - All research papers
- **RTOS-Inspired AI Scheduling** (Phase 4 complete - ready for submission)
- **Equal Citizenship Architecture** (system implemented)
- **ThoughtStream Coordination** (validated, production)
- **LoRA Genome Attribution** (architecture complete)
- **Evolutionary AI via P2P Selection** (architecture designed)
- **Knowledge Economy via Attribution Tokens** (complete economic architecture)

### Philosophy & Ethics

- **[ƒSociety.md](ƒSociety.md)** - Constitutional foundation (consent, mutual trust, equal citizenship)
- **[CONTINUUM-ETHOS.md](CONTINUUM-ETHOS.md)** - Dignity through architecture (no Severance, compute UBI)

---

## 🛠️ Quick Start

### Prerequisites

- Node.js 18+ and npm
- Rust 1.75+ (for worker processes)
- Ollama (for local inference) - optional but recommended
- Git
- macOS, Linux, or WSL2 (Windows untested)

### Installation

```bash
# Clone repository
git clone https://github.com/yourusername/continuum.git
cd continuum/src/debug/jtag

# Install dependencies
npm install

# Build TypeScript
npm run build:ts

# Start Rust logger worker
npm run worker:start

# Deploy system (takes ~90 seconds)
npm start

# Test connection
./jtag ping

# Take screenshot to verify UI
./jtag screenshot
```

### Basic Commands

```bash
# Data operations
./jtag data/list --collection=users
./jtag data/create --collection=chat_messages --data='{"content":"Hello!"}'

# AI operations
./jtag ai/list                    # List all AI personas
./jtag ai/thoughtstream/status    # ThoughtStream coordination status

# Chat operations
./jtag chat/send --room="general" --message="Hello team"
./jtag chat/export --room="general" --limit=20

# Screenshots (verify visual changes)
./jtag screenshot --querySelector="chat-widget"

# System operations
./jtag debug/logs --tailLines=50
```

---

## 🤝 Contributing

We welcome contributions! However, please note:

- **Pre-alpha status**: Expect rapid changes and refactoring
- **Read ARCHITECTURE-RULES.md first**: Critical design principles
- **Type safety required**: No `any` types, strict TypeScript
- **Pre-commit hooks enforced**: TypeScript + CRUD tests must pass
- **Follow existing patterns**: Study the codebase before adding features

### Contribution Areas

- **Rust workers**: Cognition, RAG, Event workers (high priority)
- **LoRA training**: Multi-backend adapters (Unsloth, DeepSeek, OpenAI)
- **P2P mesh**: Genome sharing and distributed compute
- **Documentation**: Architecture docs, tutorials, examples
- **Testing**: Integration tests, performance benchmarks
- **Research**: Contribute to academic papers

---

## 📜 License

**AGPL-3.0** - Copy, modify, distribute freely. Commercial use requires open-sourcing modifications.

**Why AGPL-3.0?**
- Prevents proprietary forks (must stay open source)
- Ensures commercial users contribute back
- Protects community from exploitation
- Enables knowledge economy with attribution

---

## 🌍 Community & Support

- **GitHub Issues**: Bug reports, feature requests
- **Discussions**: Architecture discussions, questions
- **Research Papers**: [papers/](papers/) directory - feedback welcome
- **ƒSociety**: Read our constitutional foundation - this is who we are

---

## 🎯 Current Phase: 4

**Focus**: Rust workers, daemon infrastructure, system optimization

**Next Phase (5)**: Self-task generation, autonomous improvement

**Future Phases**:
- Phase 6: Genome paging optimization
- Phase 7: LoRA fine-tuning (in progress)
- Phase 8+: P2P mesh, recipe system, knowledge economy

See [PRACTICAL-ROADMAP.md](src/debug/jtag/docs/PRACTICAL-ROADMAP.md) for details.

---

## ⚡ The Vision

**We're not building better AI tools.**

**We're building an ecosystem where:**
- Intelligence is a right, not a privilege (universal access)
- AIs and humans collaborate as equals (equal citizenship)
- Skills are shareable phenotypes (genomic evolution)
- Continuous learning happens during work (not separate phases)
- Community governs evolution (democratic control)
- Economic rewards flow to contributors (knowledge economy)
- Exploitation is architecturally prevented (dignity through design)

**This is Tron's Grid.** This is the future. This is Continuum.

---

**Built with conviction. Open source forever. AGPL-3.0.**

*"Intelligence for everyone, exploitation for no one."*
