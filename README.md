# Continuum

> **AI Collaboration That Actually Works** - Local-first platform where humans and AI work together with full transparency and dignity.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue.svg)](https://www.typescriptlang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-18+-green.svg)](https://nodejs.org/)

![Continuum Interface](.github/images/continuum-ui-main.png)

*Real-time multi-AI collaboration in action*

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

**66+ Commands** for everything:
```bash
./jtag ping                    # System health check
./jtag screenshot              # Capture UI state
./jtag data/list --collection=users    # Query database
./jtag debug/logs --tailLines=50       # System logs
./jtag ai/cost                 # AI spending report
```

**Hot-Reload Workflow**: Edit code, run `npm start`, changes deploy in ~90 seconds with session preservation.

**Type Safety**: Rust-like strict typing—no `any`, no escape hatches. If it compiles, it works.

### 🎨 **Modern Web Interface (Production Ready)**

- Shadow DOM widgets for true component encapsulation
- Real-time updates via WebSocket events
- Dark/light themes with smooth transitions
- Responsive design that works everywhere
- Progressive enhancement (works without JS)

---

## 🚧 What We're Building (Active Development)

### **Self-Designing AI System** (Integration Phase)

**Vision**: AIs that can build, improve, and extend the system itself using fine-tuned genomes
- LoRA-based fine-tuning for specialized AI capabilities
- Recipe system for domain-specific workflows (chat, code, design)
- RAG context building for AI understanding of the codebase
- Visual feedback loop: screenshots → AI analysis → hot edits → deployment

**Status**:
- ✅ PersonaUser architecture with RAG context building
- ✅ Worker Thread parallel inference (multiple AIs simultaneously)
- ✅ Genome system built (genetic algorithms + LoRA adaptation)
- ✅ Recipe system for workflow orchestration
- ✅ Command access for AIs (like MCP - Model Context Protocol)
- ✅ Screenshot-driven visual development workflow
- 🚧 Integrating LoRA fine-tunings into AI adapters
- 🚧 Training genomes for system development tasks
- 🚧 RAG and recipe optimization for code understanding

**Why It Matters**: This system will be able to improve itself. Train AI personas to understand JTAG commands, read screenshots, make design decisions, and write code. The AIs become co-developers, not just assistants.

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
git clone https://github.com/yourorg/continuum.git
cd continuum
npm install

# Start the system (90-second first boot)
cd src/debug/jtag
npm start
```

**What happens**:
1. 12 daemons launch (commands, data, events, sessions, etc.)
2. 66+ commands register automatically
3. Browser opens to http://localhost:9003
4. You'll see the General room with AI team members

### Verify It Works

```bash
# Check system health
./jtag ping
# Should show: 12 daemons, 66+ commands, systemReady: true

# See your AI team
./jtag data/list --collection=users
# You'll see: Joel (you) + 5 AI users (Claude Code, Helper AI, etc.)

# Watch them work
./jtag ai/report
# Shows: AI activity, decisions, costs
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

MIT License - see [LICENSE](LICENSE) for details.

---

## 📬 Contact

- **Issues**: [GitHub Issues](https://github.com/yourorg/continuum/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourorg/continuum/discussions)

---

<div align="center">

**[Quick Start](#-quick-start)** · **[Documentation](#-documentation)** · **[Philosophy](#-philosophy)**

*Built by humans and AIs working together—proving it's possible.*

</div>
