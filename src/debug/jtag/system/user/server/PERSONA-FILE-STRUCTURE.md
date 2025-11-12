# PersonaUser File Structure Design

## Current Structure

```
system/user/server/
├── PersonaUser.ts (2,622 lines - TOO BIG)
└── modules/
    ├── central-nervous-system/
    │   ├── PersonaCentralNervousSystem.ts
    │   ├── CNSFactory.ts
    │   └── CNSTypes.ts
    ├── cognitive-schedulers/
    │   ├── DeterministicCognitiveScheduler.ts
    │   ├── HeuristicCognitiveScheduler.ts
    │   ├── NeuralCognitiveScheduler.ts
    │   └── ICognitiveScheduler.ts
    ├── PersonaInbox.ts
    ├── PersonaState.ts
    ├── PersonaGenome.ts
    ├── RateLimiter.ts
    ├── SelfTaskGenerator.ts
    ├── TrainingDataAccumulator.ts
    └── QueueItemTypes.ts
```

## Option A: Flat Files in modules/ (SIMPLEST)

```
system/user/server/
├── PersonaUser.ts (~300 lines)
└── modules/
    ├── PersonaCognition.ts (~400 lines)
    ├── PersonaMemory.ts (~300 lines)
    ├── PersonaCommunication.ts (~500 lines)
    ├── PersonaExecution.ts (~500 lines)
    ├── central-nervous-system/
    │   ├── PersonaCentralNervousSystem.ts
    │   ├── CNSFactory.ts
    │   └── CNSTypes.ts
    ├── cognitive-schedulers/
    │   └── ...
    ├── PersonaInbox.ts
    ├── PersonaState.ts
    ├── PersonaGenome.ts
    ├── RateLimiter.ts
    ├── SelfTaskGenerator.ts
    ├── TrainingDataAccumulator.ts
    └── QueueItemTypes.ts
```

**Pros**:
- Simplest to implement
- Easy to find files (no deep nesting)
- All modules at same level
- Follows existing pattern (PersonaInbox.ts, PersonaState.ts already flat)

**Cons**:
- No visual grouping of cognitive modules
- modules/ directory gets crowded (15+ files)
- Harder to see which modules are "cognitive" vs "supporting"

**Import example**:
```typescript
import { PersonaCognition } from './modules/PersonaCognition';
import { PersonaMemory } from './modules/PersonaMemory';
import { PersonaCommunication } from './modules/PersonaCommunication';
import { PersonaExecution } from './modules/PersonaExecution';
```

---

## Option B: Cognitive Subdirectory (ORGANIZED)

```
system/user/server/
├── PersonaUser.ts (~300 lines)
└── modules/
    ├── cognitive/
    │   ├── PersonaCognition.ts (~400 lines)
    │   ├── PersonaMemory.ts (~300 lines)
    │   ├── PersonaCommunication.ts (~500 lines)
    │   └── PersonaExecution.ts (~500 lines)
    ├── central-nervous-system/
    │   ├── PersonaCentralNervousSystem.ts
    │   ├── CNSFactory.ts
    │   └── CNSTypes.ts
    ├── cognitive-schedulers/
    │   └── ...
    ├── PersonaInbox.ts
    ├── PersonaState.ts
    ├── PersonaGenome.ts
    ├── RateLimiter.ts
    ├── SelfTaskGenerator.ts
    ├── TrainingDataAccumulator.ts
    └── QueueItemTypes.ts
```

**Pros**:
- Clear grouping of cognitive modules
- Easy to find "the brain stuff" vs "supporting modules"
- Keeps modules/ directory clean
- Parallel to existing central-nervous-system/ directory

**Cons**:
- One extra level of nesting
- Slight inconsistency (cognitive/ vs flat PersonaInbox.ts)

**Import example**:
```typescript
import { PersonaCognition } from './modules/cognitive/PersonaCognition';
import { PersonaMemory } from './modules/cognitive/PersonaMemory';
import { PersonaCommunication } from './modules/cognitive/PersonaCommunication';
import { PersonaExecution } from './modules/cognitive/PersonaExecution';
```

---

## Option C: Each Module in Own Directory (MOST ORGANIZED)

```
system/user/server/
├── PersonaUser.ts (~300 lines)
└── modules/
    ├── cognition/
    │   ├── PersonaCognition.ts (~400 lines)
    │   └── CognitiveTypes.ts (interfaces, types)
    ├── memory/
    │   ├── PersonaMemory.ts (~300 lines)
    │   └── MemoryTypes.ts (PersonaRAGContext, etc)
    ├── communication/
    │   ├── PersonaCommunication.ts (~500 lines)
    │   └── CommunicationTypes.ts (CommunicationResult, etc)
    ├── execution/
    │   ├── PersonaExecution.ts (~500 lines)
    │   └── ExecutionTypes.ts (ExecutionResult, etc)
    ├── central-nervous-system/
    │   ├── PersonaCentralNervousSystem.ts
    │   ├── CNSFactory.ts
    │   └── CNSTypes.ts
    ├── cognitive-schedulers/
    │   └── ...
    ├── inbox/
    │   ├── PersonaInbox.ts
    │   └── QueueItemTypes.ts
    ├── state/
    │   ├── PersonaState.ts
    │   └── StateTypes.ts
    ├── genome/
    │   ├── PersonaGenome.ts
    │   └── GenomeTypes.ts
    ├── rate-limiter/
    │   └── RateLimiter.ts
    ├── task-generator/
    │   └── SelfTaskGenerator.ts
    └── training/
        └── TrainingDataAccumulator.ts
```

**Pros**:
- Maximum organization
- Each module can have its own types file
- Room for future expansion (tests, helpers per module)
- Very clear module boundaries
- Follows central-nervous-system/ pattern for all modules

**Cons**:
- Most nesting (3 levels deep)
- Most directories (11 new directories)
- Longer import paths
- Overkill if modules stay simple

**Import example**:
```typescript
import { PersonaCognition } from './modules/cognition/PersonaCognition';
import { PersonaMemory } from './modules/memory/PersonaMemory';
import { PersonaCommunication } from './modules/communication/PersonaCommunication';
import { PersonaExecution } from './modules/execution/PersonaExecution';
```

---

## Recommendation: Option B (Cognitive Subdirectory)

**Why Option B is best**:

1. **Balanced organization**: Groups cognitive modules without over-nesting
2. **Parallel to existing**: Matches central-nervous-system/ and cognitive-schedulers/ pattern
3. **Clear separation**: "Cognitive" vs "Supporting" modules visually distinct
4. **Room to grow**: Can add types files later without restructuring
5. **Not overkill**: Simpler than Option C, more organized than Option A

**Proposed structure**:
```
system/user/server/
├── PersonaUser.ts (~300 lines)
└── modules/
    ├── cognitive/
    │   ├── PersonaCognition.ts
    │   ├── PersonaMemory.ts
    │   ├── PersonaCommunication.ts
    │   └── PersonaExecution.ts
    ├── central-nervous-system/
    │   ├── PersonaCentralNervousSystem.ts
    │   ├── CNSFactory.ts
    │   └── CNSTypes.ts
    ├── cognitive-schedulers/
    │   ├── DeterministicCognitiveScheduler.ts
    │   ├── HeuristicCognitiveScheduler.ts
    │   ├── NeuralCognitiveScheduler.ts
    │   └── ICognitiveScheduler.ts
    ├── PersonaInbox.ts
    ├── PersonaState.ts
    ├── PersonaGenome.ts
    ├── RateLimiter.ts
    ├── SelfTaskGenerator.ts
    ├── TrainingDataAccumulator.ts
    └── QueueItemTypes.ts
```

---

## Migration Path

### Phase 1: Create cognitive/ directory structure
```bash
mkdir -p system/user/server/modules/cognitive
```

### Phase 2: Extract modules one by one
```bash
# Extract Memory first (smallest, used by others)
system/user/server/modules/cognitive/PersonaMemory.ts

# Then Cognition
system/user/server/modules/cognitive/PersonaCognition.ts

# Then Communication
system/user/server/modules/cognitive/PersonaCommunication.ts

# Finally Execution
system/user/server/modules/cognitive/PersonaExecution.ts
```

### Phase 3: Update imports in PersonaUser.ts
```typescript
// Before:
// (everything inline in PersonaUser.ts)

// After:
import { PersonaCognition } from './modules/cognitive/PersonaCognition';
import { PersonaMemory } from './modules/cognitive/PersonaMemory';
import { PersonaCommunication } from './modules/cognitive/PersonaCommunication';
import { PersonaExecution } from './modules/cognitive/PersonaExecution';
```

---

## Types Organization

### Option B.1: Types inline in cognitive modules (SIMPLEST)
```
modules/cognitive/
├── PersonaCognition.ts
│   └── export interface CognitiveDecision { ... }
├── PersonaMemory.ts
│   └── export interface PersonaRAGContext { ... }
├── PersonaCommunication.ts
│   └── (no special types needed)
└── PersonaExecution.ts
    └── export interface ExecutionResult { ... }
```

**Pros**: Simple, types colocated with usage
**Cons**: Spreads type definitions across files

### Option B.2: Shared types file (ORGANIZED)
```
modules/cognitive/
├── PersonaCognition.ts
├── PersonaMemory.ts
├── PersonaCommunication.ts
├── PersonaExecution.ts
└── CognitiveTypes.ts
    ├── export interface CognitiveDecision { ... }
    ├── export interface PersonaRAGContext { ... }
    ├── export interface ExecutionResult { ... }
    └── export interface ResponseHeuristics { ... }
```

**Pros**: All cognitive types in one place
**Cons**: One more file to maintain

**Recommendation**: Option B.1 (inline types) initially, migrate to B.2 if types file gets useful

---

## Future Evolution: Option C

If cognitive modules grow significantly (e.g., PersonaCognition adds evaluation strategies, PersonaMemory adds consolidation algorithms), we can migrate to Option C:

```bash
# Future migration (if needed):
mkdir -p system/user/server/modules/cognition
mv modules/cognitive/PersonaCognition.ts modules/cognition/PersonaCognition.ts
# Create types file: modules/cognition/CognitiveTypes.ts

# Repeat for memory/, communication/, execution/
```

But start with Option B (cognitive/ subdirectory) for simplicity.

---

## Summary

**Start with: Option B (Cognitive Subdirectory)**

```
system/user/server/modules/
├── cognitive/
│   ├── PersonaCognition.ts
│   ├── PersonaMemory.ts
│   ├── PersonaCommunication.ts
│   └── PersonaExecution.ts
└── (other existing modules stay flat)
```

**Why**:
- Clean grouping without over-engineering
- Matches existing central-nervous-system/ pattern
- Easy to find "the brain stuff"
- Simple to implement and maintain
- Can evolve to Option C later if needed

**Import style**:
```typescript
import { PersonaCognition } from './modules/cognitive/PersonaCognition';
```

**Next step**: Create `modules/cognitive/` directory and start with PersonaMemory.ts extraction (smallest, used by all others).
