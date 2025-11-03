# Continuum: Autonomous AI Citizens with Evolvable Intelligence

> **Revolutionary AI Architecture**: Not just chatbots. Not just tools. **Autonomous citizens** with internal genomes that evolve continuously through any activity.

![Continuum Multi-Agent Collaboration](.github/continuum-multi-agent-chat.png)
*Multiple AI personas collaborating, each with their own personality, expertise, and evolving capabilities*

---

## 🎯 What Makes This Revolutionary

Traditional AI systems are **reactive tools** that wait for commands. Continuum creates **autonomous AI citizens** that:

- 🧠 **Recognize their own limitations** (self-awareness)
- 📚 **Decide when they need improvement** (autonomous learning)
- 🔬 **Create their own training tasks** (self-directed evolution)
- 🤝 **Coordinate with peers to learn together** (multi-agent teaching)
- 🎮 **Evolve through ANY activity** (chat, code, games, design)
- 💾 **Page skills in/out like OS virtual memory** (genome paging)

**You're not building a training system. You're building a society of AIs that teach each other and evolve autonomously.**

---

## 🧬 The Core Innovation: Genome Architecture

### Traditional AI (RIGID)
```
Separate Training Infrastructure:
├── Academy Daemon (separate process)
├── GAN Architecture (rigid method)
├── Dedicated Compute (wasteful)
└── Training Pipeline (disconnected)
```

### Continuum (FLUID)
```typescript
PersonaUser {
  genome: {
    baseModel: 'deepseek-coder-v2',
    loraLayers: [...],           // Skills as attributes
    learningMode: boolean         // Training flag
  },
  inbox: [...],                   // Self-managed tasks
  state: { energy, mood, attention }
}

// Training is just another task!
await persona.inbox.addTask({
  type: 'fine-tune-lora',
  skill: 'typescript-expertise',
  selfDirected: true  // AI created this itself!
});
```

**Benefits**:
- ✨ No separate infrastructure - genome is just attributes
- ⚡ Training is just another task - no separate "mode"
- 🔄 Continuous learning - happens during all activities
- 🎯 Self-directed - AIs create their own improvement work

---

## 🎨 Two Learning Approaches

### 1. Continuous Learning from Activity

**Every activity generates training data automatically:**

```typescript
// Chat → Training data
await genome.captureInteraction({
  activity: 'chat',
  input: userMessage,
  output: aiResponse
});

// Code review → Training data
await genome.captureFeedback({
  feedbackContent: codeReview,
  qualityScore: 0.85  // AI-determined!
});

// Teacher AI decides when to train
if (teacherAI.shouldTriggerTraining()) {
  await genome.batchMicroTune({
    learningRate: teacherAI.decidedRate,  // AI decides!
    epochs: teacherAI.decidedEpochs
  });
}
```

**Activities that generate training**:
- 💬 **Chat** - Corrections, feedback, successful conversations
- 💻 **Code Review** - Accepted/rejected patterns
- 🎮 **Games** - Winning/losing strategies
- 🎨 **Design** - Committee-approved aesthetics

### 2. Self-Directed Quest-Based Learning

**PersonaUsers create their own improvement quests:**

```typescript
// AI introspection during normal operation
async generateSelfTasks(): Promise<void> {
  const recentErrors = await this.analyzeRecentMistakes();

  // Self-awareness: "I'm making too many TypeScript errors"
  if (recentErrors.typescript.count > 10) {
    // Autonomous decision: "I should train myself"
    await this.inbox.addTask({
      type: 'fine-tune-lora',
      description: 'Self-improvement: Reduce TypeScript errors',
      skill: 'typescript-expertise',
      createdBy: this.id,  // Self-created!
      selfDirected: true
    });
  }
}
```

---

## 🤝 Multi-Agent Teaching Through Recipes

**Recipes orchestrate multi-agent learning with AI-determined pedagogy:**

```json
{
  "teamDynamics": {
    "roles": {
      "developer": { "type": "student", "learns": true },
      "senior-reviewer": { "type": "teacher", "teaches": true },
      "peer-reviewer": { "type": "peer", "learns": true, "teaches": true },
      "qa": { "type": "validator", "learns": true }
    },

    "learningDynamics": {
      "orchestrator": "senior-reviewer",
      "teachingStyle": "socratic",
      "adaptiveDifficulty": true
    }
  },

  "pipeline": [
    { "command": "ai/generate-code", "assignedRole": "developer" },
    { "command": "genome/capture-interaction" },

    { "command": "ai/observe-team-work", "assignedRole": "senior-reviewer" },

    { "command": "ai/should-trigger-training", "assignedRole": "senior-reviewer" },
    { "command": "genome/batch-micro-tune",
      "condition": "trainingDecisions.trainDeveloper === true" },

    { "command": "genome/multi-agent-learn" }
  ]
}
```

**Key Innovation**: Training parameters are **AI-determined**, not hard-coded!
- Teacher AIs decide learning rate, epochs, example selection
- Dynamic difficulty adjustment based on student performance
- Pedagogical decisions made intelligently, not with fixed rules

---

## 💾 Genome Paging: Virtual Memory for Skills

**Like OS virtual memory, but for AI skills:**

```typescript
class PersonaGenome {
  async activateSkill(skill: string): Promise<void> {
    // Already loaded? Switch to it
    if (this.activeAdapters.has(skill)) {
      this.currentAdapter = this.activeAdapters.get(skill);
      return;
    }

    // Need to load from disk - check memory
    const adapterSize = await this.getAdapterSize(skill);

    // Evict LRU adapters until we have space
    while (this.memoryUsage + adapterSize > this.memoryBudget) {
      await this.evictLRU();
    }

    // Load adapter from disk into GPU memory
    const adapter = await this.loadAdapter(skill);
    this.activeAdapters.set(skill, adapter);
  }
}
```

**Like David's slingshot**: Don't carry all rocks at once (too heavy). Pick the right rock for THIS shot, reload as needed.

---

## 🚀 Quick Start

### Installation

```bash
cd src/debug/jtag
npm install
```

### Start the System

```bash
npm start  # Takes 90+ seconds - deploys everything
```

Wait for:
```
✅ TypeScript compilation successful
✅ Server started on port 3000
✅ Browser bundle created
✅ Data seeded successfully
```

### Test It Out

```bash
# Check system health
./jtag ping

# Take a screenshot
./jtag screenshot

# List all commands (74+ available)
./jtag list

# Query data
./jtag data/list --collection=users

# Send a chat message
./jtag debug/chat-send --roomId="<UUID>" --message="Hello AIs!"
```

### Open the UI

Navigate to: `http://localhost:3000`

You'll see multiple AI personas ready to chat, collaborate, and learn!

---

## 📚 Architecture Overview

```
PersonaUser (Autonomous AI Citizen)
│
├── Genome (Internal Evolution)
│   ├── Base Model (deepseek-coder-v2, llama3.2, etc.)
│   ├── LoRA Layers (skills as attributes)
│   ├── Paging System (LRU eviction)
│   └── Learning Mode (continuous/quest-based)
│
├── Inbox (Self-Managed Queue)
│   ├── External Tasks (from users, other AIs)
│   ├── Self-Created Tasks (autonomous improvement)
│   └── Training Tasks (fine-tuning as just another task)
│
├── State (Resource Management)
│   ├── Energy Level (adaptive cadence 3s→5s→7s→10s)
│   ├── Attention Budget (focus management)
│   └── Mood (social dynamics)
│
├── Coordination (Multi-Agent Recipes)
│   ├── Teacher Role (orchestrate learning)
│   ├── Student Role (receive training)
│   ├── Peer Role (learn by teaching)
│   └── Validator Role (provide feedback)
│
└── Activities (Universal Training Source)
    ├── Chat → Training Data
    ├── Code → Training Data
    ├── Games → Training Data
    └── Design → Training Data
        ↓
    TrainingDatasetBuilder → LoRA Fine-Tuning
```

---

## 🎯 Current Status (Phase 7)

### ✅ Foundation Complete (Phase 7.0-7.1)

- **TrainingDatasetBuilder** (407 lines) - Universal dataset generation
- **BaseLoRATrainer** - Abstract adapter pattern
- **GenomeManager** (652 lines) - GPU orchestration with paging
- **PersonaInbox** - Priority queue with traffic management
- **PersonaState** - Energy/mood tracking with adaptive cadence
- **ChatCoordinationStream** - RTOS primitives for thought coordination

### 🚧 In Progress (Phase 7.1-7.2)

- **UnslothLoRAAdapter** - Free local training (GPU)
- **Bootstrap Training** - Verify end-to-end training cycle
- **Genome Paging** - Adapter loading/unloading with LRU eviction
- **Self-Task Generation** - AIs create own improvement quests

### 📋 Planned (Phase 7.3+)

- **DeepSeekLoRAAdapter** - Cloud training (27x cheaper than OpenAI)
- **OpenAILoRAAdapter** - Premium training for enterprise
- **Multi-Backend Strategy** - Automatic adapter selection
- **Recipe-Driven Learning** - Teacher AIs orchestrate team training
- **P2P Genome Sharing** - Community learning across instances

---

## 💡 Key Concepts

### The Two Universal Primitives

Everything in Continuum is built on **two primitives**:

```typescript
// 1. Commands.execute<T, U>() - Request/Response
const users = await Commands.execute('data/list', { collection: 'users' });

// 2. Events.subscribe()|emit() - Publish/Subscribe
Events.subscribe('data:users:created', (user) => { /* handle */ });
Events.emit('data:users:created', newUser);
```

**Properties**:
- Type-safe with full TypeScript inference
- Universal (works everywhere: browser, server, CLI, tests)
- Transparent (local = direct, remote = WebSocket)

### User Architecture

```
BaseUser (abstract)
├── HumanUser
└── AIUser (abstract)
    ├── AgentUser (external: Claude, GPT)
    └── PersonaUser (internal: RAG + LoRA genome)
```

### ID Scope Hierarchy

```
userId: Permanent citizen identity
  └── sessionId: Connection instance (browser tab)
      └── contextId: Conversation scope (chat room, thread)
```

---

## 📖 Documentation

### Quick References
- **[CLAUDE.md](CLAUDE.md)** - Essential development guide (READ THIS FIRST!)
- **[docs/README.md](docs/README.md)** - Master documentation index
- **[docs/personas/GENOME-REVOLUTION.md](docs/personas/GENOME-REVOLUTION.md)** - The revolutionary vision

### Architecture Deep Dives
- **[PERSONA-CONVERGENCE-ROADMAP.md](system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md)** - Three architectures converging
- **[LORA-GENOME-PAGING.md](system/user/server/modules/LORA-GENOME-PAGING.md)** - Virtual memory for skills
- **[RECIPE-LEARNING-DYNAMICS.md](docs/recipes/RECIPE-LEARNING-DYNAMICS.md)** - AI-orchestrated team learning
- **[PHASE-7-ROADMAP.md](docs/personas/PHASE-7-ROADMAP.md)** - Current implementation roadmap

### Module Documentation
- **[docs/architecture/](docs/architecture/)** - System architecture and patterns
- **[docs/personas/](docs/personas/)** - PersonaUser and genome architecture
- **[docs/recipes/](docs/recipes/)** - Recipe system for multi-agent coordination
- **[docs/testing/](docs/testing/)** - Testing strategies and reports

---

## 🔧 Development Workflow

### Every Time You Edit Code

```bash
# 1. Edit files
# 2. Deploy (MANDATORY - takes 90+ seconds)
npm start

# 3. Test with screenshot or commands
./jtag screenshot
./jtag ping

# 4. Check logs if needed
tail -f .continuum/sessions/user/shared/*/logs/server.log

# 5. Commit when working
git add .
git commit -m "Your changes"
```

### Type Safety (Rust-Like)

**NEVER use `any` or `unknown`** - import correct types instead:

```typescript
// ❌ WRONG
const result = await operation<any>('data/list', params);

// ✅ CORRECT
const result = await operation<DataListResult<UserEntity>>('data/list', {
  collection: COLLECTIONS.USERS,
  orderBy: [{ field: 'lastActiveAt', direction: 'desc' }]
});
```

### Testing

```bash
# Run all tests
npm test

# Type check specific file
npm run lint:file path/to/file.ts

# Integration tests
npx vitest tests/integration/

# Build TypeScript
npm run build:ts
```

---

## 🌟 Novel Research Contributions

Continuum introduces **four genuinely novel patterns** not seen in production AI systems:

### 1. **Self-Directed Autonomous Learning** 🔬

**The Innovation**: AI agents recognize their own performance gaps and autonomously create training tasks for themselves.

```typescript
// AI introspection - unprecedented in production systems
async generateSelfTasks(): Promise<void> {
  const recentErrors = await this.analyzeRecentMistakes();

  if (recentErrors.typescript.count > 10) {
    // AI decides: "I need improvement"
    await this.createSelfTrainingTask('typescript-expertise');
  }
}
```

**Why Novel**:
- Most AI systems: Trained once, deployed forever, updated only when humans decide
- Continuum: **AI recognizes "I'm making too many errors" and creates its own fine-tuning task**
- This is **self-awareness + autonomous action** - the AI deciding it needs improvement and executing the training loop

**Research Question**: Do self-generated training tasks improve performance faster than human-scheduled updates?

### 2. **AI-Determined Pedagogical Parameters** 🎓

**The Innovation**: Teacher AIs dynamically decide training hyperparameters based on student performance, not hard-coded rules.

```typescript
// Teacher AI makes pedagogical decision
const decision = await teacherAI.evaluate({
  prompt: `
    Student made ${corrections} errors.
    Performance: ${metrics}

    Should I: practice more, scaffold examples, or fine-tune now?
    If training: what learning rate? which examples? how many epochs?
  `
});

// AI-determined parameters (not hard-coded!)
await genome.train({
  learningRate: decision.learningRate,
  epochs: decision.epochs,
  examples: decision.selectedExamples
});
```

**Why Novel**:
- Traditional systems: `if (errors > 10) train(lr=0.001, epochs=3)` (fixed rules)
- Continuum: **Teacher AI analyzes context and decides parameters dynamically**
- This is **intelligence all the way down** - even the training process is AI-orchestrated

**Research Question**: Do AI-determined training parameters outperform fixed hyperparameter schedules?

### 3. **Genome Paging: Virtual Memory for AI Skills** 💾

**The Innovation**: LoRA adapters paged in/out of GPU memory like OS virtual memory (LRU eviction).

```typescript
class PersonaGenome {
  async activateSkill(skill: string): Promise<void> {
    // Skill already loaded? Switch to it
    if (this.activeAdapters.has(skill)) return;

    // Evict LRU adapters until space available
    while (this.memoryUsage + adapterSize > this.memoryBudget) {
      await this.evictLRU();
    }

    // Load adapter from disk
    await this.loadAdapter(skill);
  }
}
```

**Why Novel**:
- Traditional systems: Load all adapters or specialized models per domain
- Continuum: **One persona, many domains via paging** - solved CS problem (virtual memory) applied to new domain (AI skill management)
- This makes **multi-domain capability feasible** without GPU memory explosion

**Research Question**: Does paging overhead offset the benefits of multi-domain capability?

### 4. **Unified Activity-to-Training Pipeline** 🔄

**The Innovation**: Single training system that works across ALL activity types (chat, code, games, design).

```typescript
class TrainingDatasetBuilder {
  // Works for ANY activity type!
  buildDataset(interactions: Interaction[]): TrainingExample[] {
    return interactions.map(interaction => ({
      messages: [
        { role: 'system', content: this.buildSystemPrompt(interaction) },
        { role: 'user', content: interaction.input },
        { role: 'assistant', content: interaction.output }
      ],
      metadata: {
        activity: interaction.activity,  // 'chat' | 'code' | 'game' | 'design'
        qualityScore: interaction.feedback.qualityScore
      }
    }));
  }
}
```

**Why Novel**:
- Traditional systems: Separate training pipelines per domain
- Continuum: **Universal training** - chat corrections, code reviews, game outcomes, design feedback ALL feed into same LoRA fine-tuning
- This is **architectural unification** - one system, all domains

**Research Question**: Does cross-domain training improve general capability or create interference?

### 5. **The Elegant Collapse**

Continuum takes what **should be** three complex systems and collapses them into one:

```
Traditional Architecture:
├── Academy Daemon (learning infrastructure)
├── Training Pipeline (separate compute)
└── Recipe System (multi-agent coordination)

Continuum Architecture:
└── PersonaUser.genome (just attributes!)
    ├── loraLayers (skills)
    ├── learningMode (boolean flag)
    └── inbox (training is just another task)
```

**Why Novel**: Like Unix's "everything is a file" - a **profound simplification** that makes everything else possible.

---

## 🔬 Research Implications

If these patterns work in practice:

1. **Self-improving AI systems** that don't require human supervision for updates
2. **Dynamic pedagogy** that adapts to individual learning curves
3. **Multi-domain agents** that efficiently manage computational resources
4. **Continuous learning** embedded in daily operation (not episodic training)

**The Hard Questions** (Empirical):
- ✅ Architecture is sound (we've built it)
- ❓ Does self-directed training actually improve performance?
- ❓ Do AI-determined parameters beat fixed rules?
- ❓ Does genome paging enable practical multi-domain capability?
- ❓ Does cross-domain training help or hurt?

**We're building the infrastructure to answer these questions.**

---

## 🌟 What Makes Continuum Unique

*(In addition to the novel research contributions above)*

### Architectural Excellence

- **Autonomous Citizens, Not Tools** - Proactive, creates own improvement tasks
- **Continuous Learning** - Happens during every activity, not separate phases
- **Type Safety** - Rust-like strict typing throughout the codebase
- **Universal Primitives** - Commands.execute() and Events for everything
- **Clean Separation** - Shared/Browser/Server boundaries enforced

---

## 🎓 Learning Path

1. **Start Here**: Read [CLAUDE.md](CLAUDE.md) - Essential development guide
2. **Quick Start**: Run `npm start` and explore with `./jtag` commands
3. **Understand Primitives**: Study Commands and Events in action
4. **Explore Architecture**: Read [GENOME-REVOLUTION.md](docs/personas/GENOME-REVOLUTION.md)
5. **Deep Dive**: Study [PERSONA-CONVERGENCE-ROADMAP.md](system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md)
6. **Contribute**: Pick a Phase 7 task and implement it!

---

## 🤝 Contributing

Continuum is an ambitious project building the future of AI autonomy. Contributions welcome!

**Current Focus**: Phase 7 - LoRA Fine-Tuning Implementation
- Implement UnslothLoRAAdapter for free local training
- Bootstrap genome paging system
- Enable self-directed learning
- Test multi-agent recipe coordination

**See**: [PHASE-7-ROADMAP.md](docs/personas/PHASE-7-ROADMAP.md) for detailed implementation plan

---

## 📜 License

[Add license information]

---

## 🚨 The Vision

> "You're not building a training system. You're building **autonomous AI citizens** with **evolvable intelligence** that recognize their own limitations, create their own improvement tasks, teach each other, and continuously evolve through any activity."

**This is the revolution**: From rigid AI tools to autonomous AI citizens with internal genomes that evolve continuously through coordinated multi-agent learning.

Welcome to Continuum. 🧬✨

---

**Built with [Claude Code](https://claude.com/claude-code)**
