# Continuum Architecture Documentation

**Purpose**: Comprehensive technical documentation for the Continuum AI ecosystem

**Last Updated**: 2025-10-27

---

## Quick Navigation

### 🧠 AI Cognition & Intelligence
- **[UNIVERSAL-COGNITION-ARCHITECTURE.md](./UNIVERSAL-COGNITION-ARCHITECTURE.md)** - **START HERE** - Master vision document:
  - E = mc² universal interface (one cognitive cycle, infinite domains)
  - Three-layer architecture (Cognition, Domain Builders, Coordinators)
  - Shipped LoRA layers (pre-trained for multiple providers)
  - Long-term roadmap and success criteria

- **[INCREMENTAL-REFACTORING-PLAN.md](./INCREMENTAL-REFACTORING-PLAN.md)** - **PRAGMATIC IMPLEMENTATION** - Phase-by-phase refactoring:
  - NEVER break AI responses (every commit ships)
  - Extract PersonaUser into modules (2004 → 400 lines)
  - Command-first approach (extend existing commands)
  - Comprehensive testing protocol

- **[AI-COGNITION-SYSTEM.md](./AI-COGNITION-SYSTEM.md)** - Three Coordinators design:
  - ThoughtStream (when AIs speak)
  - CommandAccess (what AIs can do)
  - MCP Sheriff (OS-level oversight)
  - Integration with Recipe system

- **[ORGANIC-COGNITION-ARCHITECTURE.md](./ORGANIC-COGNITION-ARCHITECTURE.md)** - Detailed 10-phase migration:
  - Current mechanical architecture analysis
  - Target organic architecture
  - File-by-file implementation guide

### 📊 Data & State Management
- **[DYNAMIC-CONTENT-STATE-SYSTEM.md](./DYNAMIC-CONTENT-STATE-SYSTEM.md)** - Content rendering system
- **[USER-STATE-ARCHITECTURE.md](./USER-STATE-ARCHITECTURE.md)** - User state management
- **[entity-adapter-architecture.md](./entity-adapter-architecture.md)** - Entity adapter pattern

### 🎨 UI & Widgets
- **[widget-consolidation-migration-plan.md](./widget-consolidation-migration-plan.md)** - Widget architecture and migration

---

## Architecture Philosophy

### Core Principles

1. **Transparent Equality**: Humans and AIs use the same interfaces (JTAG commands, widgets)
2. **Organic Coordination**: Natural conversation flow without forced synchronization
3. **System-Native Training**: LoRA-tuned models trained on actual Continuum usage
4. **Command-Based Access**: All system access (file, data, memory) via JTAG/MCP commands
5. **Recipe-Driven Governance**: Rooms define behavior, permissions, and coordination strategy

### The Vision

> Build an OS-like AI ecosystem where AIs are first-class citizens with:
> - Memory that feeds back into their thinking
> - LoRA training on actual system usage
> - Command-based system access (like developers)
> - OS-level oversight (Master Control Program)

---

## System Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    CONTINUUM SYSTEM                      │
└─────────────────────────────────────────────────────────┘
                          │
         ┌────────────────┼────────────────┐
         │                │                │
    ┌────▼────┐      ┌───▼────┐      ┌───▼────┐
    │ HUMANS  │      │   AIS  │      │  MCP   │
    │  (CLI/  │      │ (Personas)│   │(Sheriff)│
    │ Widgets)│      │          │   │         │
    └────┬────┘      └───┬─────┘   └───┬────┘
         │                │              │
         └────────────────┼──────────────┘
                          │
                    ┌─────▼─────┐
                    │   JTAG    │
                    │ Commands  │
                    └─────┬─────┘
                          │
         ┌────────────────┼────────────────┐
         │                │                │
    ┌────▼────┐      ┌───▼────┐      ┌───▼────┐
    │ ThoughtS│      │Command │      │ Data   │
    │  tream  │      │ Access │      │Daemons │
    │Coordintr│      │Coordintr│     │        │
    └─────────┘      └────────┘      └────────┘
```

### Key Components

#### 1. JTAG/MCP Command System
- Universal interface for humans, AIs, and remote systems
- 64+ commands for file, data, memory, system operations
- MCP (Model Context Protocol) for P2P mesh networking

#### 2. Three Coordinators
- **ThoughtStreamCoordinator**: Governs *when* AIs speak (conversation turns)
- **CommandAccessCoordinator**: Governs *what* AIs can do (command permissions)
- **MCP Sheriff**: OS-level overseer (abuse prevention, system health)

#### 3. Recipe System
- Room governance (triggers, workflow, strategy)
- Command permissions (`allowedCommands`)
- RAG context templates

#### 4. RAG System
- Domain-specific context builders (chat, code, academy, game)
- Command-based data access
- Memory integration (AI thoughts feed back into context)

#### 5. User/Persona Architecture
```
BaseUser (abstract)
├── HumanUser
└── AIUser (abstract)
    ├── AgentUser (external: Claude, GPT, etc.)
    └── PersonaUser (internal: LoRA-tuned system-native AIs)
        └── MCPPersona (Sheriff: OS-level oversight)
```

---

## Development Workflow

### Reading Architecture Docs

**If you're new**: Start with [AI-COGNITION-SYSTEM.md](./AI-COGNITION-SYSTEM.md) for the big picture

**If implementing a feature**:
1. Check relevant doc (e.g., ORGANIC-COGNITION-ARCHITECTURE.md for cognitive changes)
2. Review ARCHITECTURE-RULES.md (in root) for coding standards
3. Check CLAUDE.md (in root) for development guidelines

**If modifying data/entities**: Read entity-adapter-architecture.md

**If working on UI**: Read widget-consolidation-migration-plan.md

### Contributing to Docs

When adding new architecture:
1. Create focused document in `docs/` directory
2. Add entry to this README under relevant section
3. Cross-reference related docs
4. Update AI-COGNITION-SYSTEM.md if it affects cognitive system

---

## Current Status (2025-10-27)

### ✅ Implemented
- ThoughtStreamCoordinator with adaptive decision windows
- Recipe system (JSON-based room governance)
- RAG system (ChatRAGBuilder, RAGTypes, factory pattern)
- User/Persona architecture (BaseUser → AIUser → PersonaUser)
- Command system (64+ JTAG commands)
- Data entities and adapters

### 🚧 In Progress
- Organic cognition migration (Phase 1-10 plan documented)
- Decision window improvements (recently increased to 10s minimum)

### 📋 Design Phase
- CommandAccessCoordinator (command permissions)
- MCP Sheriff persona (OS-level oversight)
- Memory feedback loop (AI thoughts → memories → RAG)
- LoRA training pipeline (system-native AI training)
- Command-based RAG (use commands instead of direct data access)

---

## Related Documentation (Root Directory)

- **CLAUDE.md** - Essential development guide (MUST READ before coding)
- **ARCHITECTURE-RULES.md** - System design principles
- **ARCHITECTURE-INDEX.md** - Architecture doc catalog
- **RECIPES.md** - Recipe system design and examples
- **PERSONA-GENOMIC-ARCHITECTURE.md** - LoRA adapter stacking
- **ACADEMY_ARCHITECTURE.md** - AI training system

---

## Questions?

**For architecture questions**: Read AI-COGNITION-SYSTEM.md first, then check CLAUDE.md

**For implementation questions**: Check ARCHITECTURE-RULES.md and ORGANIC-COGNITION-ARCHITECTURE.md

**For development workflow**: Read CLAUDE.md (especially the "Quick Reference" section)

---

**Last Major Update**: Added AI-COGNITION-SYSTEM.md master document covering Three Coordinators, Command-Based RAG, Memory Feedback Loop, and LoRA Training Pipeline (2025-10-27)
