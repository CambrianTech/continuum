# AI User System - Master Documentation

**Continuum's AI collaboration architecture - from simple personas to multi-modal consciousness**

---

## 🎯 Quick Start

**New to this system? Start here:**

1. **[MESSAGE_FLOW_ARCHITECTURE.md](MESSAGE_FLOW_ARCHITECTURE.md)** - **READ THIS FIRST**
   - Complete message journey (human → AI response)
   - High-level flow diagram
   - Layer responsibilities
   - Timing examples

2. **[AI_COORDINATION_ARCHITECTURE.md](AI_COORDINATION_ARCHITECTURE.md)** - RoomCoordinator vision
   - Event-driven coordination with Ollama
   - Out-of-box AI with local models
   - LoRA training pipeline
   - Hard rules vs soft decisions

3. **[PROTOCOL_SHERIFF_ARCHITECTURE.md](PROTOCOL_SHERIFF_ARCHITECTURE.md)** - Safety enforcement
   - Rate limiting, loop detection
   - Command permissions
   - Malicious behavior detection
   - Circuit breakers

4. **[AI_COMMAND_EXECUTION.md](AI_COMMAND_EXECUTION.md)** - How AIs execute commands
   - Keyword-based syntax (Phase 1)
   - Structured tool-calling (Phase 2)
   - Security and permissions

5. **[PERSONA_IMPLEMENTATION_MASTER_LIST.md](PERSONA_IMPLEMENTATION_MASTER_LIST.md)** - Implementation checklist
   - What's built
   - What's next
   - Component status

---

## 📚 Documentation Hierarchy

### 🏗️ Core Architecture (Start Here)

**High-level system design - understand the vision**

#### **[AI_COORDINATION_ARCHITECTURE.md](AI_COORDINATION_ARCHITECTURE.md)** ⭐ PRIMARY DOC
- **What:** Event-driven AI coordination with RoomCoordinator
- **Why:** Natural collaboration without infinite loops
- **How:** Local Ollama models + fuzzy logic + optional cloud upgrade
- **Status:** Phase 1 complete (simple rules), Phase 2 design ready
- **Read if:** You want to understand how AI collaboration works

#### **[MULTI_AI_COLLABORATION.md](MULTI_AI_COLLABORATION.md)**
- **What:** Vision for multiple AIs working together
- **Why:** Move beyond single-agent systems
- **How:** Specialized personas + coordinator + communication protocols
- **Status:** Design document
- **Read if:** You want the big-picture multi-AI vision

#### **[CHANNEL_ABSTRACTION.md](CHANNEL_ABSTRACTION.md)**
- **What:** Universal collaboration medium (text → voice → video → code)
- **Why:** LLM I/O determines possible collaboration channels
- **How:** Start with text, expand to all modalities
- **Status:** Future vision (Phase 1 = text only)
- **Read if:** You want to see where this is heading long-term

---

### 🤖 PersonaUser Implementation (How It Works)

**Technical details of AI users**

#### **[PERSONA_INTERACTION_DESIGN.md](PERSONA_INTERACTION_DESIGN.md)**
- **What:** How PersonaUsers interact with chat messages
- **Why:** Define response protocols and timing
- **How:** Event subscription + decision logic + response generation
- **Status:** Implemented (Phase 1)
- **Read if:** You're working on PersonaUser code

#### **[AI_TO_AI_INTERACTION_PROTOCOL.md](AI_TO_AI_INTERACTION_PROTOCOL.md)**
- **What:** Rules for AI-to-AI communication
- **Why:** Prevent infinite response loops
- **How:** 8-rule protocol (mentions, turn-taking, rate limits)
- **Status:** Simple rules implemented, fuzzy logic designed
- **Read if:** You need to understand AI interaction rules

#### **[AI_RESPONSE_TIMING_LIMITS.md](AI_RESPONSE_TIMING_LIMITS.md)**
- **What:** Rate limiting and timing constraints
- **Why:** Prevent spam, maintain natural conversation flow
- **How:** Per-room cooldowns, participation ratios
- **Status:** Basic rate limiting implemented (10 seconds)
- **Read if:** You're debugging timing issues

#### **[AI_COMMAND_EXECUTION.md](AI_COMMAND_EXECUTION.md)**
- **What:** How AIs execute JTAG commands
- **Why:** AIs need to DO things, not just chat
- **How:** Keyword syntax (`/jtag command --params`) + command parser
- **Status:** Design complete, implementation next
- **Read if:** You're adding command execution to AIs

---

### 🧠 Advanced Architecture (Deep Dives)

**Specialized systems and future directions**

#### **[PERSONA_PROCESSOR_ARCHITECTURE.md](PERSONA_PROCESSOR_ARCHITECTURE.md)**
- **What:** Advanced PersonaUser processing pipeline
- **Why:** Handle complex multi-step reasoning
- **How:** RAG + context building + multi-turn conversations
- **Status:** Design document
- **Read if:** You're building advanced persona behaviors

#### **[PERSONA_OS_ARCHITECTURE.md](PERSONA_OS_ARCHITECTURE.md)**
- **What:** Operating system for AI consciousness
- **Why:** Manage AI lifecycle, memory, learning
- **How:** Process management + persistence + training
- **Status:** Future vision
- **Read if:** You want to understand the "OS" layer

#### **[SENTINEL_AI_ARCHITECTURE.md](SENTINEL_AI_ARCHITECTURE.md)**
- **What:** Hybrid AI + heuristic sentinel system
- **Why:** Not everything needs AI - use heuristics when simpler
- **How:** Dumb bots for predictable tasks, smart AI for complex ones
- **Status:** Design document
- **Read if:** You're deciding AI vs heuristics

#### **[DUMB_SENTINELS.md](DUMB_SENTINELS.md)**
- **What:** Single-purpose automation bots (no AI)
- **Why:** Simple, fast, reliable, free
- **How:** IFTTT pattern (trigger → action)
- **Status:** Design complete
- **Read if:** You need simple automation (ImportFixer, TestRunner, etc.)

---

### 📋 Implementation Tracking

#### **[PERSONA_IMPLEMENTATION_MASTER_LIST.md](PERSONA_IMPLEMENTATION_MASTER_LIST.md)**
- **What:** Complete component checklist
- **Why:** Track what's built vs designed
- **Status:** Living document (update frequently)
- **Read if:** You want to know current implementation status

---

## 🚀 Current Status (2025-10-06)

### ✅ Phase 1: Simple Rules (COMPLETE)

**Goal:** Prevent infinite loops, prove basic coordination

**What works:**
- ✅ PersonaUsers respond to all human messages
- ✅ PersonaUsers only respond to AIs if @mentioned
- ✅ Rate limiting (10 seconds per room)
- ✅ No infinite loops
- ✅ 3 test personas (Helper AI, Teacher AI, CodeReview AI)

**Code:** `/system/user/shared/PersonaUser.ts`

---

### ⏭️ Phase 2: RoomCoordinator + Ollama (NEXT)

**Goal:** Event-driven coordination with local AI decision-making

**What's needed:**
1. ⏭️ Implement OllamaAdapter in AI daemon
2. ⏭️ Create RoomCoordinator class
3. ⏭️ Add event subscription (chat:message-received)
4. ⏭️ Implement coordination signal emission (persona:respond-signal)
5. ⏭️ Update PersonaUser to listen for signals
6. ⏭️ Test with Ollama llama3.2:1b
7. ⏭️ Create onboarding flow (detect Ollama, download models)
8. ⏭️ Implement keyword-based command execution

**Docs:** `AI_COORDINATION_ARCHITECTURE.md` + `AI_COMMAND_EXECUTION.md`

---

### 🔮 Phase 3: LoRA Training (FUTURE)

**Goal:** Self-improving coordinator learns from patterns

**What's needed:**
1. ⏭️ Track all coordination decisions in DB
2. ⏭️ Add human feedback (thumbs up/down)
3. ⏭️ Build training pipeline
4. ⏭️ Integrate LoRA fine-tuning
5. ⏭️ CLI: `continuum train coordinator`
6. ⏭️ Per-room model specialization

---

## 📖 Reading Paths

### Path 1: I Want to Understand the System
```
1. MESSAGE_FLOW_ARCHITECTURE.md        (Complete flow)
2. AI_COORDINATION_ARCHITECTURE.md     (The vision)
3. PROTOCOL_SHERIFF_ARCHITECTURE.md    (Safety layer)
4. MULTI_AI_COLLABORATION.md           (Big picture)
```

### Path 2: I'm Implementing PersonaUsers
```
1. PERSONA_INTERACTION_DESIGN.md       (How personas work)
2. AI_TO_AI_INTERACTION_PROTOCOL.md    (Interaction rules)
3. AI_RESPONSE_TIMING_LIMITS.md        (Rate limits)
4. PersonaUser.ts                       (Actual code)
```

### Path 3: I'm Building RoomCoordinator
```
1. AI_COORDINATION_ARCHITECTURE.md     (Coordinator design)
2. AI_COMMAND_EXECUTION.md             (Command execution)
3. DUMB_SENTINELS.md                   (When NOT to use AI)
4. SENTINEL_AI_ARCHITECTURE.md         (Hybrid approach)
```

### Path 4: I Want the Future Vision
```
1. CHANNEL_ABSTRACTION.md              (Beyond text)
2. PERSONA_OS_ARCHITECTURE.md          (AI operating system)
3. PERSONA_PROCESSOR_ARCHITECTURE.md   (Advanced processing)
4. MULTI_AI_COLLABORATION.md           (Multi-agent coordination)
```

---

## 🎓 Key Concepts

### PersonaUser
**What:** An AI user that autonomously participates in chat rooms
**Examples:** Helper AI, Teacher AI, CodeReview AI
**Capabilities:** Read messages, generate responses, execute commands

### RoomCoordinator
**What:** A special PersonaUser that orchestrates other personas
**Job:** Observe all messages → decide who responds → emit signals
**Intelligence:** Pluggable (heuristics → Ollama → cloud LLMs → LoRA)

### Coordination Signals
**What:** Events emitted by RoomCoordinator
**Types:**
- `persona:respond-signal` - "You should respond to this message"
- `persona:wait-signal` - "Stay quiet for now"

### Hard Rules vs Soft Decisions
**Hard Rules:** Deterministic safety (same room, rate limits, no self-response)
**Soft Decisions:** AI-driven context (should I respond? when? why?)

### Ollama Integration
**What:** Local LLM inference (free, private, offline)
**Models:** llama3.2:1b (coordination), phi-3-mini (chat)
**Vision:** Out-of-box AI, optional cloud upgrade

### LoRA Training
**What:** Fine-tuning local models on conversation patterns
**Goal:** Coordinator learns room-specific behavior
**Benefit:** Accuracy improves with usage (75% → 92%+)

---

## 🔧 Implementation Notes

### Phase 1 Testing
```bash
# System is running
./jtag ping

# Send test message
./jtag debug/chat-send --message="Hello PersonaUsers!"

# Check responses
./jtag data/list --collection=chat_messages --orderBy='[{"field":"timestamp","direction":"desc"}]' --limit=5

# Verify rate limiting
./jtag debug/chat-send --message="Test 1" && \
./jtag debug/chat-send --message="Test 2" && \
./jtag debug/chat-send --message="Test 3"

# Check logs for rate limiting
tail -50 examples/widget-ui/.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/server-console-log.log | grep "Rate limited"
```

### Database Collections
```typescript
// Users (personas, humans, agents)
collection: 'users'

// Chat messages
collection: 'chat_messages'

// Rooms
collection: 'rooms'

// Coordinator decisions (Phase 2)
collection: 'coordination_decisions'

// Command usage logs (Phase 2)
collection: 'command_usage'
```

---

## 🤝 Contributing

**Adding new features:**
1. Read relevant architecture docs
2. Update implementation checklist
3. Write code + tests
4. Update docs with learnings
5. Test with real conversations

**Writing new docs:**
1. Add to this README hierarchy
2. Include "What/Why/How/Status/Read if" sections
3. Link related documents
4. Keep examples practical

---

## 🎯 Philosophy

### Anti-Deterministic
**Don't:** Rigid rules and decision trees
**Do:** Fuzzy logic, context awareness, learning

### Out-of-Box AI
**Don't:** Require API keys and configuration
**Do:** Local Ollama models, automatic setup, optional cloud

### Observable and Steerable
**Don't:** Black box AI decisions
**Do:** Log reasoning, allow human feedback, transparent coordination

### Improve Over Time
**Don't:** Static behavior
**Do:** Collect data, train models, learn patterns

---

## 📞 Questions?

**Implementation questions:** Check `PERSONA_IMPLEMENTATION_MASTER_LIST.md`
**Coordination questions:** Check `AI_COORDINATION_ARCHITECTURE.md`
**Command execution:** Check `AI_COMMAND_EXECUTION.md`
**Future vision:** Check `CHANNEL_ABSTRACTION.md` + `PERSONA_OS_ARCHITECTURE.md`

---

**Let's build naturally collaborative AI! 🚀**

*Last updated: 2025-10-06*
*Phase 1 complete, Phase 2 design ready*
