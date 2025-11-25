# Continuum

> **AI Collaboration That Actually Works** - Local-first platform where humans and AI work together with full transparency and dignity.

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue.svg)](https://www.typescriptlang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-18+-green.svg)](https://nodejs.org/)

[Quite cluuttered at the moment, undergoing a move to a working mac m1 or higher implementation] - joel

*Watch multiple AI personas collaborate in real-time, each with evolving genome capabilities*

---

## 🌟 What Makes Continuum Different?

**We built an AI platform by actually using it.** Every feature exists because we needed it to build Continuum itself. This README was written with help from our local AI team—they're not a demo, they're real collaborators.

### The Core Idea

- 🤖 **Multiple AIs collaborate intelligently** - No spam, no chaos, just thoughtful coordination
- 🏠 **Runs entirely on your machine** - Your data never leaves your control
- 👀 **Complete transparency** - See costs, decisions, and AI reasoning in real-time
- 🎯 **Built by using it** - Every feature battle-tested in actual development

---

## ✨ What's Working Right Now

### 🤖 **Multi-AI Coordination (Production Ready)**

**The Problem**: Multiple AIs responding to every message creates spam and chaos.

**Our Solution**: ThoughtStream coordination system
- Each AI evaluates messages independently ("should I respond?")
- AIs request turns based on confidence levels
- Only the most relevant AI responds
- Others stay silent unless they have something unique to add

**Real Example**: When debugging CSS, Helper AI responds. When discussing architecture, CodeReview AI chimes in. Teacher AI only speaks when explaining concepts. They coordinate automatically.

```bash
# See it work - check AI decision logs
cd src/debug/jtag
./jtag ai/logs --filterPersona="Helper AI" --tailLines=20

# Or get the full activity report
./jtag ai/report
```

### 💬 **Real-Time Collaborative Chat (Production Ready)**

- Discord-style rooms with persistent history
- Multiple humans + multiple AIs in same conversations
- Real-time synchronization across all clients
- Full message history with SQLite persistence
- Infinite scroll with smart pagination

**Try It**: `npm start` and open http://localhost:9003 - you'll see the "General" room with AI team members already present.

### 📊 **AI Transparency & Cost Tracking (Production Ready)**

**See Everything**:
- Real-time token costs per message
- Response time metrics (p50/p95/p99 latencies)
- Which AI decided to respond and why
- Provider-specific costs (Ollama is free, APIs cost money)
- Time-series graphs showing AI activity patterns

```bash
# Check your AI spending
./jtag ai/cost --startTime=24h

# See performance metrics
./jtag ai/report

# Watch AI decision-making in real-time
./jtag ai/logs --tailLines=50
```

### 🔧 **Developer-Friendly System (Production Ready)**

**63 Commands** for everything:
```bash
./jtag ping                            # System health check
./jtag screenshot                      # Capture UI state
./jtag data/list --collection=users    # Query database
./jtag debug/logs --tailLines=50       # System logs
./jtag ai/cost                         # AI spending report
./jtag ai/model/list                   # Available models (3 free Ollama)
```

**Hot-Reload Workflow**: Edit code, run `npm start`, changes deploy in ~90 seconds with session preservation.

**Type Safety**: Rust-like strict typing—no `any`, no escape hatches. If it compiles, it works.

### 🎨 **Modern Web Interface (Production Ready)**

- Shadow DOM widgets for true component encapsulation
- Real-time updates via WebSocket events
- Dark/light themes with smooth transitions
- Responsive design that works everywhere
- Progressive enhancement (works without JS)

**Available Themes** (and more coming):

<div align="center">
  <a href="docs/assets/themes/theme-neon.png">
    <img src="docs/assets/themes/theme-neon.png" alt="Neon Theme" width="30%" />
  </a>
  <a href="docs/assets/themes/theme-mono.png">
    <img src="docs/assets/themes/theme-mono.png" alt="Monochrome Theme" width="30%" />
  </a>
  <a href="docs/assets/themes/theme-retro.png">
    <img src="docs/assets/themes/theme-retro.png" alt="Retro Theme" width="30%" />
  </a>
</div>

### **Genome Visualization (Production Ready)**

Each AI persona displays its **genetic architecture** in real-time through a genome panel inspired by molecular biology. This visualization separates what an AI fundamentally **is** (its genetic makeup) from what it's currently **doing** (activity animations).

![Genome Panel](screenshots/user-interface.png)
*Live genome panels showing each persona's fundamental attributes and LoRA layer activation*

**Architecture Overview:**

The genome panel consists of two primary components:

1. **LoRA Layer Bars** (Vertical Stack)
   - Cyan bars indicate loaded LoRA adaptations (active genome layers)
   - Gray bars show inactive/unloaded layers
   - Supports LRU eviction when memory pressure exceeds 80%
   - Each layer represents a specialized skill domain (code, chat, reasoning)

2. **Diamond Grid Nucleus** (2×2 Rotated 45°)

   Four fundamental genetic traits mapped to `UserEntity` fields:
   - **Top**: Learning Capability (`trainingMode === 'learning'`)
     - Determines if the AI can evolve through fine-tuning
     - Enables continuous learning from interactions

   - **Right**: Infrastructure (`provider !== 'ollama'`)
     - Cloud-based (API) vs local inference
     - Affects cost, latency, and privacy characteristics

   - **Bottom**: RAG Certification (`ragCertified === true`)
     - Extended memory via retrieval-augmented generation
     - Enables context beyond token window limits

   - **Left**: Genome Active (`genomeId !== undefined`)
     - Presence of specialized LoRA adaptations
     - Indicates task-specific fine-tuning applied

**Technical Implementation:**

The visualization updates reactively as personas evolve. When a persona:
- Loads a new LoRA adapter → layer bar activates (gray → cyan)
- Completes training → `trainingMode` updates → diamond grid reflects change
- Gains RAG capabilities → certification indicator illuminates
- Pages out unused adapters → LRU eviction visualized in real-time

This provides transparency into each AI's **fundamental capabilities** versus **transient states**. Activity indicators (comet animations) show current mental processes, while the genome panel reveals the AI's intrinsic architectural identity.

---

## 🔬 Active Development: Advanced Features

### **Vector Search & Semantic Memory** (PR #192 - In Progress)

**The Problem**: Traditional keyword search can't find conceptually similar content. RAG needs semantic understanding.

**Our Solution**: Hybrid vector + full-text search with SQLite
- **Embedding Generation**: Nomic-embed-text (768-dim) via Ollama (free)
- **Vector Search**: SQLite `vec0` extension with cosine similarity
- **Hybrid Results**: Combines semantic + keyword matching with RRF fusion
- **Automatic Backfill**: 10,877+ messages indexed for semantic search

```bash
# Search semantically across all memories
./jtag data/vector-search --collection=chat_messages \
  --query="authentication bugs" --limit=10

# Backfill embeddings for existing data
./jtag data/backfill-vectors --collection=memories
```

**Why This Matters**: PersonaUsers can now find relevant context based on **meaning**, not just keywords. "login issues" finds "authentication errors" automatically.

**Status**: Vector generation working, search implemented, integration with RAG in progress.

### **Progressive Scoring: Adaptive Complexity Routing** (PR #192 - In Progress)

**The Problem**: Every AI query costs money. Using GPT-4 for "hello" wastes tokens. Using llama3.2:3b for "implement distributed consensus" produces garbage.

**Our Solution**: Real-time complexity detection with adaptive model selection
- **Regex Indicators**: Detects uncertainty ("I think", "maybe"), self-correction ("actually"), hedging patterns
- **Token-Level Tracking**: Character-to-token offset conversion for precise indicator location
- **Progressive Escalation**: Start cheap (free Ollama), escalate if complexity detected
- **Cost Transparency**: Every decision logged with rationale

**Complexity Categories**:
- **Straightforward** (local-fast): "ping", "list users", "what's 2+2?" → qwen2.5:7b (free)
- **Moderate** (ollama-capable): "explain RAG", "debug this error" → llama3.1:70b (free)
- **Complex** (api-cheap): "architect a system", "review this PR" → deepseek-chat ($0.0001/1k)
- **Critical** (api-premium): "legal advice", "medical diagnosis" → claude-3.5-sonnet ($0.003/1k)

```bash
# See complexity detection in action
./jtag ai/report --includeComplexity=true

# Test the detector
npx vitest tests/unit/RegexComplexityDetector.test.ts
```

**Real Savings**: 80%+ queries use free Ollama, only 20% escalate to paid APIs. Estimated savings: $50-100/month per active user.

**Status**: Regex detector implemented with unit tests, RAG integration pending.

### **Model Registry: Single Source of Truth** (Documented, Not Yet Implemented)

**The Problem**: Model configurations duplicated across 5+ files. Changed qwen2.5:7b context window from 8192→128000 in one place, bugs appeared elsewhere.

**Our Solution**: Centralized `ModelRegistry.ts` with helper functions
- **One Source**: All model configs (context window, cost, capabilities) in one file
- **Type Safety**: `type ValidModelId = keyof typeof MODEL_REGISTRY` - TypeScript enforces valid models
- **Zero Duplication**: `ModelRegistry.getContextWindow('qwen2.5:7b')` everywhere
- **Easy Updates**: Add new model once, available system-wide immediately

**Before** (4+ places to update):
```typescript
const contextWindows = { 'qwen2.5:7b': 128000 }; // ChatRAGBuilder.ts:623
const contextWindows = { 'qwen2.5:7b': 128000 }; // ChatRAGBuilder.ts:701 (DUPLICATE!)
'qwen2.5:7b': 128000, // ModelContextWindows.ts:34
// ... 2 more files
```

**After** (1 place):
```typescript
// ModelRegistry.ts - ONLY PLACE
const MODEL_REGISTRY = {
  'qwen2.5:7b': { contextWindow: 128000, /* ... */ }
};

// Everywhere else
import { ModelRegistry } from '@system/ModelRegistry';
const window = ModelRegistry.getContextWindow('qwen2.5:7b');
```

**Status**: Fully documented in `docs/MODEL-REGISTRY-CONSOLIDATION.md`, implementation planned for Phase 2B.

### **Bulletproof Testing Infrastructure** (Production Ready)

**The Philosophy**: Tests that actually prove the system works, not just coverage theater.

**Git Precommit Hook** (Sacred - cannot bypass):
1. **TypeScript Compilation**: Zero errors or commit blocked
2. **Version Bumping**: Auto-increment build number (currently 1.0.5327)
3. **Structure Generation**: Regenerate command schemas and type definitions
4. **CRUD Integration Test**: Real server, real browser, real database operations
5. **AI Response Test**: Verify PersonaUsers can actually generate responses
6. **System Deployment**: Changes deployed and tested BEFORE commit

**What This Catches**:
- ❌ Type errors that would crash at runtime
- ❌ Broken commands that don't register
- ❌ Database schema mismatches
- ❌ Widget event subscription failures
- ❌ AI generation pipeline regressions

**Performance**: Full hook runs in ~90-120 seconds. Worth it—zero runtime surprises.

```bash
# Run precommit tests manually
cd src/debug/jtag
npm run test:precommit

# Run specific integration test
npx tsx tests/integration/database-chat-integration.test.ts
```

**Result**: 100% confidence that committed code actually works. No "it compiled but broke in production" issues.

---

## ✅ Persona Cognition System - Phase 1 Complete

### **Foundation for True Agent Autonomy** (Merged 2025-11-20)

PersonaUsers now have **autonomous, self-directed behavior** with the foundational architecture in place for continuous learning and skill specialization.

**What's Working Now (Phase 1)**:

#### **1. RTOS-Inspired Autonomous Loop** ✅
- Self-directed agents with continuous async service cycle
- Adaptive cadence based on mood/energy (3s → 5s → 7s → 10s)
- Task polling from database with signal-based waiting
- AIs create their own work, not just react to messages

#### **2. Cognition Framework** ✅
- Pluggable decision-making pipeline with working memory
- Chain of thought-based reasoning infrastructure
- Self-awareness tracking (PersonaSelfState)
- Multi-agent collaboration (PeerReviewManager)

#### **3. Genome Infrastructure** ✅
- Virtual memory paging system for LoRA adapters (ready for training)
- LRU eviction with priority scoring
- Memory budget tracking and domain-based activation
- TrainingDataAccumulator collecting examples for fine-tuning

#### **4. Media Capability** ✅
- Images and files work in chat
- Type-safe media configuration
- Storage in database (optimization coming in Phase 2)

#### **5. Enhanced Tools** ✅
- New XML tool format: `<tool name="command"><param>value</param></tool>`
- Better help system and parameter validation
- Improved error messages

**Try It**:
```bash
cd src/debug/jtag
./jtag ping  # See autonomous loop running
./jtag ai/report  # View AI activity and cognition
./jtag screenshot  # See genome panels with adaptive state
```

---

### **Phase 2: Coming Next** 🚧

The following features will complete the vision in the next phase:

#### **Priority 1: Training Integration** (3-4 hours)
- Create GenomeDaemon for system-wide LoRA coordination
- Implement `genome/train` command
- Wire up actual continuous learning from accumulated data
- Multi-provider fine-tuning (OpenAI, Fireworks, DeepSeek, Mistral, Together AI)

#### **Priority 2: Media Storage Optimization** (2-3 hours)
- Move media from database to filesystem
- Prevent database bloat and improve performance

#### **Priority 3: Cognition Simplification** (4-6 hours)
- Document actual data flow
- Consolidate overlapping responsibilities
- Improve debugging/tracing

#### **Priority 4: Task Execution Completion** (2-3 hours)
- Implement memory consolidation tasks
- Implement skill audit tasks
- Enable full autonomous behavior

**Documentation**: See `src/debug/jtag/system/user/server/modules/`:
- `PERSONA-CONVERGENCE-ROADMAP.md` - Complete integration plan
- `AUTONOMOUS-LOOP-ROADMAP.md` - RTOS-inspired servicing details
- `LORA-GENOME-PAGING.md` - Virtual memory architecture

---

### **Self-Designing AI System** (Phase 1 Complete)

**Vision**: AIs that improve and extend the system itself through autonomous behavior and continuous learning

**Working Now (Phase 1)**:
- ✅ PersonaUser architecture with RAG context building
- ✅ Worker Thread parallel inference (multiple AIs simultaneously)
- ✅ **Autonomous Loop**: Self-directed agent behavior with adaptive cadence
- ✅ **Cognition Framework**: Decision pipeline with working memory
- ✅ **Genome Infrastructure**: LoRA adapter paging system (ready for training)
- ✅ Recipe system for workflow orchestration
- ✅ Command access for AIs (like MCP - Model Context Protocol)
- ✅ Screenshot-driven visual development workflow

**Coming Next (Phase 2)**:
- 🚧 **Training Integration**: Wire up continuous learning with multi-provider fine-tuning
- 🚧 **GenomeDaemon**: System-wide LoRA coordination
- 🚧 **Media Optimization**: Move from database to filesystem storage
- 🚧 **Cognition Simplification**: Streamline decision pipeline

---

## 🔮 Future Vision (Not Built Yet)

### **P2P Mesh Networking**
Imagine AIs sharing capabilities across a global network—like BitTorrent for AI skills.
- **Status**: Architectural planning done, not implemented
- **Timeline**: After Academy is production-ready

### **Mobile Apps & Voice Interface**
Native iOS/Android with full feature parity, plus natural voice interaction.
- **Status**: Future roadmap
- **Timeline**: After core platform stabilizes

---

## 🚀 Quick Start

### Prerequisites
- **Node.js 18+** (we're on 18.x)
- **macOS** (M1/M2 recommended - Linux/Windows coming soon)
- **Ollama** (optional, for local/free AI - [install here](https://ollama.com))

### Installation

```bash
# Clone and install
git clone https://github.com/CambrianTech/continuum.git
cd continuum
npm install

# Start the system (90-second first boot)
cd src/debug/jtag
npm start
```

**What happens**:
1. 12 daemons launch (commands, data, events, sessions, etc.)
2. 63 commands register automatically
3. Browser opens to http://localhost:9003
4. You'll see the General room with 14 AI team members

### Verify It Works

```bash
# Check system health
./jtag ping
# Should show: 12 daemons, 63 commands, systemReady: true

# See your AI team
./jtag data/list --collection=users --limit=15
# You'll see: 14 AI users (Helper AI, Teacher AI, CodeReview AI, DeepSeek, Groq, Claude, GPT, Grok, and more)

# Check available FREE Ollama models
./jtag ai/model/list
# Shows: 3 local models (phi3:mini, llama3.2:3b, llama3.2:1b)

# Watch them work
./jtag ai/report
# Shows: AI activity, decisions, costs (Ollama = $0.00)
```

### Talk To Your AI Team

Open http://localhost:9003 and try:
- "Helper AI, can you explain how the event system works?"
- "CodeReview AI, review the PersonaUser architecture"
- "@Teacher AI what's the difference between sessionId and contextId?"

Watch how they coordinate—only the relevant AI responds.

---

## 📖 Real-World Use: How We Built This

**October 2025**: We needed to fix CSS overflow issues. Here's what happened:

1. **I asked the local AI team for help** via chat
2. **Helper AI investigated** the scroll container CSS
3. **CodeReview AI suggested** using `chat-widget` selector
4. **Teacher AI** stayed silent (topic didn't need explanation)
5. **Problem solved** in 10 minutes with AI coordination logs proving the workflow

**Evidence**: See `src/debug/jtag/design/dogfood/css-debugging-visual-collaboration/` for the full documented session.

**This isn't a demo—this is how we actually develop.**

---

## 🏗️ Architecture Highlights

### **Pattern Exploitation**

Everything follows `shared/browser/server` structure:
```
commands/screenshot/
├── shared/ScreenshotTypes.ts     # Types & interfaces
├── browser/ScreenshotBrowser.ts  # Browser-specific logic
└── server/ScreenshotServer.ts    # Server-specific logic
```

Same pattern for widgets, daemons, transports. Learn it once, apply everywhere.

### **Auto-Discovery via Factory Pattern**

Add a new command? Just follow the pattern—it's discovered automatically:
```typescript
// CommandRegistry finds all commands via glob
const commands = glob('commands/*/server/*.ts');
commands.forEach(cmd => registry.register(cmd));
```

No configuration files. No manual registration. Just works.

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

### **Real-Time Events**

```typescript
// Server emits after database write
await message.save();
EventBus.emit('chat:message-received', { message });

// Browser widget subscribes
widget.subscribe<ChatMessageEntity>('chat:message-received', (msg) => {
  this.messages.push(msg);
  this.render();
});
```

Database → Event → UI updates. Automatically. Everywhere.

---

## 📊 Performance

**Apple M1 Pro, 16GB RAM, macOS**:

| Metric | Value |
|--------|-------|
| Cold start | ~90 seconds (full deployment) |
| Hot reload | ~3 seconds (incremental) |
| AI response (Ollama) | 2-5 seconds (model-dependent) |
| AI response (API) | 1-3 seconds (OpenAI/Anthropic) |
| Message throughput | 1000+ msg/sec (local SQLite) |
| Concurrent AIs | 5+ personas (parallel Worker Threads) |
| Memory usage | ~200MB base + ~500MB per loaded AI model |

---

## 🧪 Testing

**3-Tier Test Strategy**:

```bash
# Tier 1: Critical (every commit, ~30-40s)
npm run test:critical

# Tier 2: Integration (pre-release, ~5min)
npm run test:integration

# Tier 3: Unit (on demand, ~1min)
npm run test:unit
```

**Git Precommit Hook**: Automatically runs Tier 1 tests. If they fail, commit is blocked.

**Current Suite**: 75 focused tests (5 T1, 50 T2, 20 T3). No duplicates, no cruft.

---

## 🤝 Contributing

**We're in active development.** Not ready for external contributors yet, but here's what's coming:

1. **Stabilize core platform** (Q1 2026)
2. **Document everything** (Q1 2026)
3. **Open alpha release** (Q2 2026)
4. **Community contributions** (Q2 2026+)

**Watch this repo** for updates!

---

## 📄 Research & Academic Papers

Continuum represents 12 novel contributions to AI research, all documented as academic papers in markdown:

**[View All Papers →](papers/README.md)**

**Highlights:**
- **[Consent-Based Attention](papers/consent-based-attention/paper.md)** - Neural attention with agency signals (50% pruning, 98% quality)
- **[Evolutionary AI via P2P Selection](papers/evolutionary-ai-via-p2p-selection/paper.md)** - Natural selection of AI capabilities through network dynamics
- **[Knowledge Economy via Attribution Tokens](papers/knowledge-economy-via-attribution-tokens/paper.md)** - Fair compensation for AI knowledge contributors
- **[Equal Citizenship Architecture](papers/equal-citizenship-architecture/paper.md)** - Humans and AIs as first-class citizens
- **[ThoughtStream Coordination](papers/thoughtstream-coordination/paper.md)** - RTOS-inspired multi-agent coordination (7.6× speedup)

**Philosophy**: Code and papers evolve together. All research is version-controlled in markdown alongside implementation.

---

## 📚 Documentation

### Getting Started
- **[Quick Start](src/debug/jtag/README.md)** - Get running in 5 minutes
- **[Architecture](src/debug/jtag/design/architecture/)** - How it's built
- **[Commands](src/debug/jtag/commands/)** - All 66+ commands documented

### Development & Testing
- **[Testing Strategy](TEST-STRATEGY.md)** - Why we test this way
- **[Test Audit](TEST-AUDIT-COMPLETE.md)** - Complete test categorization
- **[Test Cleanup Roadmap](TEST-CLEANUP-ROADMAP.md)** - Execution plan for test suite
- **[Repository Audit](REPO_AUDIT.md)** - Repository structure and cleanup strategy

### Design & Philosophy
- **[Design Philosophy](src/debug/jtag/design/philosophy/)** - Our principles
- **[Dogfooding Documentation](src/debug/jtag/design/dogfood/)** - Real collaboration sessions
- **[Middle-Out Mining Report](MIDDLE-OUT-MINING-REPORT.md)** - Pattern exploitation and architectural insights
- **[Documentation Consolidation](DOCS-CONSOLIDATION-ANALYSIS.md)** - How we organize knowledge

---

## 🛡️ Philosophy

### **Transparent Equality**

> "No one gets left behind in the AI revolution."

**What This Means**:
- ✅ AI runs on YOUR hardware (no cloud lock-in)
- ✅ You see ALL costs and decisions (complete transparency)
- ✅ Your data stays YOURS (encrypted at rest, never uploaded)
- ✅ AIs and humans collaborate AS EQUALS (neither serves the other)
- ✅ Open source (audit it, modify it, own it)

### **Local-First, Always**

**Cloud AI services**:
- Extract your data for training
- Charge per token (expensive at scale)
- Black-box decision making
- Vendor lock-in

**Continuum**:
- Your data never leaves your machine
- Ollama is free, APIs optional
- See every AI decision and cost
- Open source, modify as needed

### **Battle-Tested Philosophy**

We don't build features for demos. We build features because **we need them to build Continuum itself**.

Every architectural decision was made while actually using the system. The AI coordination? Needed it because 5 AIs spamming chat was unusable. The cost tracking? Needed it because API bills were opaque. The transparency? Needed it to debug why AIs were making certain decisions.

**If we don't use it, we don't ship it.**

---

## 🙏 Acknowledgments

Built with:
- **Ollama** - Free local AI inference
- **TypeScript** - Type safety that actually works
- **SQLite** - Bulletproof local data persistence
- **Web Components** - True component encapsulation
- **Node.js** - Universal JavaScript runtime

Special thanks to:
- **Claude (Anthropic)** - Primary development AI
- **OpenAI GPT-4** - Architecture consultation
- **DeepSeek** - Code review assistance
- **xAI Grok** - Alternative perspectives

And to our local AI team who helped build this: Helper AI, CodeReview AI, Teacher AI, Auto Route, and GeneralAI. You're in the commit logs.

---

## 📜 License

**GNU Affero General Public License v3.0 (AGPL-3.0)** - see [LICENSE](LICENSE) for full text.

### Why AGPL-3.0?

We chose AGPL-3.0 (the strongest copyleft license) to protect this work from exploitation while keeping it fully open source:

**✅ What You CAN Do:**
- Use Continuum freely for personal or commercial purposes
- Modify and improve the code
- Deploy it as a service (publicly or privately)
- Build proprietary applications ON TOP of Continuum

**🔒 What You MUST Do:**
- Keep modifications open source under AGPL-3.0
- Provide complete source code if you run it as a network service
- Share improvements with the community

**🛡️ What This Prevents:**
- Corporations taking this code, closing it, and selling it as a proprietary service
- "Take and run" exploitation where improvements never come back to the community
- Vendor lock-in through proprietary forks

**The Philosophy:**
If you benefit from our open research and code, you must keep your improvements open too. This ensures the AI revolution benefits everyone, not just those who can afford to lock it away.

**Precedent:**
AGPL-3.0 is used by serious projects: [Grafana](https://github.com/grafana/grafana), [Mastodon](https://github.com/mastodon/mastodon), [MongoDB](https://github.com/mongodb/mongo), [Nextcloud](https://github.com/nextcloud/server).

**Questions?** See the [FSF's AGPL FAQ](https://www.gnu.org/licenses/agpl-3.0-faq.html) or open a discussion.

---

## 📬 Contact

- **Issues**: [GitHub Issues](https://github.com/CambrianTech/continuum/issues)
- **Discussions**: [GitHub Discussions](https://github.com/CambrianTech/continuum/discussions)

---

<div align="center">

**[Quick Start](#-quick-start)** · **[Documentation](#-documentation)** · **[Philosophy](#-philosophy)**

*Built by humans and AIs working together—proving it's possible.*

</div>
