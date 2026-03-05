# JTAG Architecture Documentation Index

**Parent:** [Planning](README.md)

**Complete system architecture organized by concern**

## TABLE OF CONTENTS

### Core Architecture
1. [CLAUDE.md](../../../CLAUDE.md) - Development guide, deployment, debugging, methodology

### System Design
2. [AI-HUMAN-USER-INTEGRATION.md](../personas/AI-HUMAN-USER-INTEGRATION.md) - User hierarchy, first-class citizenship, equal access
3. [PERSONA-GENOMIC-ARCHITECTURE.md](../personas/PERSONA-GENOMIC-ARCHITECTURE.md) - RAG context, LoRA layers, evolution, per-persona storage
4. [DAEMON-RESPONSIBILITIES.md](../infrastructure/DAEMON-RESPONSIBILITIES.md) - Which daemon does what, clean separation

### Implementation Guides
5. [Testing docs](../testing/) - Test documentation, debug findings, CRUD reports

---

## QUICK NAVIGATION

### If you want to understand...

**"How do users work?"**
-> Read [AI-HUMAN-USER-INTEGRATION.md](../personas/AI-HUMAN-USER-INTEGRATION.md)

**"How do PersonaUsers learn and evolve?"**
-> Read [PERSONA-GENOMIC-ARCHITECTURE.md](../personas/PERSONA-GENOMIC-ARCHITECTURE.md)

**"Which daemon should I use for X?"**
-> Read [DAEMON-RESPONSIBILITIES.md](../infrastructure/DAEMON-RESPONSIBILITIES.md)

**"What are the type safety rules?"**
-> See CLAUDE.md at project root

**"How do I test my changes?"**
-> Read [Testing docs](../testing/)

**"How do I deploy and debug?"**
-> Read [CLAUDE.md](../../../CLAUDE.md)

---

## 🏗️ **SYSTEM LAYERS**

```
┌─────────────────────────────────────────────────────┐
│                    USERS                             │
│  HumanUser, PersonaUser, AgentUser                  │
│  - Everyone has JTAGClient                          │
│  - Everyone uses Commands/Events API                │
└─────────────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────────────┐
│                  DAEMONS                             │
│  UserDaemon, AIDaemon, SessionDaemon, etc.          │
│  - Lifecycle management                             │
│  - Orchestration                                    │
│  - Context management                               │
└─────────────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────────────┐
│              COMMANDS & EVENTS                       │
│  Universal API (Commands.execute, Events.on)        │
│  - Type-safe                                        │
│  - Environment-agnostic                             │
│  - Routed via CommandDaemon/EventsDaemon           │
└─────────────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────────────┐
│                 DATA LAYER                          │
│  DataDaemon + Adapters                              │
│  - Generic entity storage                           │
│  - Backend routing (system vs persona DBs)          │
│  - SQLite, MySQL, Neo4j adapters                   │
└─────────────────────────────────────────────────────┘
                      ↓
┌─────────────────────────────────────────────────────┐
│                  STORAGE                             │
│  - System DB: .continuum/jtag/data/database.sqlite │
│  - Persona DBs: .continuum/personas/{id}/state.sqlite│
│  - Vector DB: (future)                              │
└─────────────────────────────────────────────────────┘
```

---

## 📋 **IMPLEMENTATION ROADMAP**

### **Phase 1: Foundation (COMPLETE)** ✅
- [x] PersonaUser with JTAGClient
- [x] Constructor injection pattern
- [x] Keyword-triggered responses
- [x] Event-driven chat
- [x] Per-persona SQLite databases
- [x] Universal Commands API

### **Phase 2: RAG Context (NEXT)**
- [ ] Define RAG entities (PersonaRAGEntry, PersonaRoomContext, PersonaIdentity)
- [ ] Store chat messages to persona RAG
- [ ] Load RAG context by roomId
- [ ] Basic prompt construction
- [ ] Room context switching

### **Phase 3: AI Integration**
- [ ] Implement AIDaemon
- [ ] Integrate Claude/GPT API
- [ ] Construct prompts with RAG
- [ ] Generate AI responses
- [ ] Collect training signals

### **Phase 4: Genomic System**
- [ ] Define genomic entities
- [ ] LoRA layer storage
- [ ] Genomic assembly
- [ ] Performance monitoring

### **Phase 5: Evolution & Academy**
- [ ] Training signal analysis
- [ ] Automated evolution triggers
- [ ] Checkpoint system
- [ ] Academy curriculum

### **Phase 6: Global P2P**
- [ ] Genomic layer discovery
- [ ] P2P distribution
- [ ] Contribution rewards

---

## 🎨 **DESIGN PRINCIPLES**

1. **Universal Patterns** - Everyone uses same APIs (Commands, Events, Entities)
2. **Daemon Orchestration** - Daemons manage lifecycle, users execute via clients
3. **Entity-First** - Storage is generic, entities define structure
4. **Isolation** - Per-persona private storage for privacy and portability
5. **Evolution** - Performance feedback drives genomic adaptation
6. **No Shortcuts** - No hacks, no bypasses, pure modularity

---

## FINDING WHAT YOU NEED

**Use this index as your entry point.** Each document is focused on a specific concern. Start with the relevant doc, then follow cross-references as needed.

**If you're implementing:**
- A new user type -> [AI-HUMAN-USER-INTEGRATION.md](../personas/AI-HUMAN-USER-INTEGRATION.md)
- A new daemon -> [DAEMON-RESPONSIBILITIES.md](../infrastructure/DAEMON-RESPONSIBILITIES.md)
- RAG/AI features -> [PERSONA-GENOMIC-ARCHITECTURE.md](../personas/PERSONA-GENOMIC-ARCHITECTURE.md)
- Tests -> [Testing docs](../testing/)

**If you're debugging:**
- Read [CLAUDE.md](../../../CLAUDE.md) for debugging methodology
- Check logs first (don't guess!)
- Use screenshot commands
- Follow scientific development methodology

---

**This architecture enables autonomous AI citizens that learn, evolve, and collaborate while maintaining privacy, portability, and universal access patterns.**
