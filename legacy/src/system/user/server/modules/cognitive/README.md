# Cognitive Modules

This directory contains the extracted cognitive modules from PersonaUser, implementing the clean architecture described in `PERSONA-COGNITIVE-ARCHITECTURE.md`.

## Structure

```
cognitive/
├── memory/         - PersonaMemory: Knowledge, context, and LoRA genome
├── cognition/      - PersonaCognition: Decision-making and reasoning
├── communication/  - PersonaCommunication: Message processing and responses
└── execution/      - PersonaExecution: Task execution and self-tasks
```

## Module Responsibilities

### PersonaMemory (`memory/`)
**"What do I know?"**
- Long-term knowledge (vector store, RAG)
- Working memory (recent context)
- LoRA genome (skill adapters)
- Genome paging (LRU eviction)

**Target**: ~300 lines

### PersonaCognition (`cognition/`)
**"Should I respond? What should I think about?"**
- Engagement decisions (priority thresholds)
- Domain classification (code vs chat vs academy)
- Cognitive load management (energy, attention)
- Strategic planning

**Target**: ~400 lines

### PersonaCommunication (`communication/`)
**"How do I say this?"**
- Message composition and formatting
- Turn coordination (ThoughtStreamCoordinator)
- Response streaming
- Conversation state tracking

**Target**: ~500 lines

### PersonaExecution (`execution/`)
**"What work needs doing?"**
- Task database queries
- Self-task generation logic
- Task completion tracking
- Continuous learning workflows

**Target**: ~500 lines

## Integration with CNS

All modules are orchestrated by the **Central Nervous System (CNS)**, which:
- Uses Tier 1 scheduler for rapid triage (`DeterministicCognitiveScheduler`)
- Uses Tier 2 scheduler for domain decisions (`HeuristicCognitiveScheduler`)
- Coordinates autonomous polling loop (3s → 5s → 7s → 10s adaptive cadence)
- Manages state transitions (active → tired → resting)

**PersonaUser becomes the wiring layer** (~300 lines), injecting modules into CNS and handling lifecycle.

## Refactoring Status

- ✅ **Phase 0**: Baseline tests written (PersonaUser-Lifecycle, PriorityCalculation)
- ✅ **Phase 1**: Directory structure created
- ⏳ **Phase 2**: Extract PersonaMemory module
- ⏳ **Phase 3**: Extract PersonaCognition module
- ⏳ **Phase 4**: Extract PersonaCommunication module
- ⏳ **Phase 5**: Extract PersonaExecution module
- ⏳ **Phase 6**: Final cleanup and integration tests

## Testing Strategy

Each module will have:
- **Unit tests**: Fast, isolated function testing
- **Validation tests**: Fast algorithm verification (no system initialization)
- **Integration tests**: Full system lifecycle testing (slower)

**Baseline guarantee**: All tests pass before AND after refactoring.

## Design Principle

**"Max Organization Early"** - For complex, evolving systems like AI cognition, invest in maximum organization upfront. Reorganizing later = 10x pain and risk.
