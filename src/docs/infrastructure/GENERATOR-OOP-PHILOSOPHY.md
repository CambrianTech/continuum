# Generator + OOP: The Intertwined Architecture

## Core Principle

Generators and Object-Oriented Programming are not separate concerns - they are **parallel forces** that reinforce each other:

```
        CREATION TIME                    RUNTIME
        ─────────────                    ───────

        ┌─────────────┐                 ┌─────────────┐
        │  Generator  │                 │  BaseClass  │
        │  Templates  │                 │  Contracts  │
        └──────┬──────┘                 └──────┬──────┘
               │                               │
               │ produces                      │ enforces
               ▼                               ▼
        ┌─────────────────────────────────────────┐
        │                                         │
        │   class MyCommand extends CommandBase   │
        │                                         │
        │     // Generator filled structure       │
        │     // OOP enforces the contract        │
        │                                         │
        └─────────────────────────────────────────┘
```

**Two guardrails, one artifact:**
- **Generator**: "Start from proven structure"
- **OOP**: "Implement this interface or fail to compile"

## Why This Works for AI

### Tree-Based Delegation of Ability

```
                    Human (Root)
                        │
            ┌───────────┼───────────┐
            ▼           ▼           ▼
         AI Agent    AI Agent    AI Agent
            │           │           │
         ┌──┴──┐     ┌──┴──┐     ┌──┴──┐
         ▼     ▼     ▼     ▼     ▼     ▼
       Task  Task  Task  Task  Task  Task
```

At each level of delegation:
- **Generators** ensure consistent structure flows down
- **OOP contracts** ensure behavioral compatibility flows up
- **Templates** encode proven patterns at each node

### Evolutionary Pressure for Toolmakers

```
AI₁ builds command manually (high effort)
    ↓
AI₁ recognizes repeatable pattern
    ↓
AI₁ creates generator (investment)
    ↓
AI₂ uses generator (low effort)
    ↓
AI₂ has capacity for next generator
    ↓
System capability compounds
```

**Natural selection favors AIs that build infrastructure over one-off solutions.**

The smartest thing an AI can do after solving a hard problem is **make that problem trivial for everyone else**.

## Geometric Alignments

### 1. Hierarchy Alignment
- Class hierarchies (OOP) mirror organizational hierarchies (delegation)
- `BaseUser → AIUser → PersonaUser` parallels `System → Agent → Task`

### 2. Template Alignment
- Generator templates mirror recipe templates
- Both provide structure, require filling in specifics
- Commands : Generator :: Activities : Recipe

### 3. Contract Alignment
- TypeScript interfaces = behavioral contracts
- Generator specs = structural contracts
- Both validated at different times, both required

### 4. Knowledge Flow Alignment
```
Abstract (Base Classes)          ←→    Reusable (Generators)
        ↓                                      ↓
Concrete (Implementations)       ←→    Specific (Generated Code)
        ↓                                      ↓
Runtime (Instances)              ←→    Execution (Running Commands)
```

## The Pattern Applied

| Domain | OOP Base | Generator | AI Fills In |
|--------|----------|-----------|-------------|
| Commands | `CommandBase` | `development/generate` | Business logic |
| Widgets | `BaseWidget` | Widget generator | Render logic |
| Daemons | `BaseDaemon` | Daemon generator | Service logic |
| Users | `BaseUser` | User factory | Persona config |
| Activities | `ActivityBase` | Recipe JSON | Phase logic |

## Implementation Rules

### 1. Never Create Structure Manually
```bash
# WRONG - Manual file creation
mkdir commands/my-command && touch MyCommand.ts

# RIGHT - Use generator
./jtag development/generate --spec=my-spec.json
```

### 2. Always Extend Base Classes
```typescript
// WRONG - Standalone class
class MyCommand {
  execute() { ... }
}

// RIGHT - Extend base, inherit contracts
class MyCommand extends CommandBase<MyParams, MyResult> {
  execute(params: MyParams): Promise<MyResult> { ... }
}
```

### 3. Templates Use Path Aliases
```typescript
// WRONG - Fragile relative imports
import { CommandBase } from '../../../daemons/command-daemon/shared/CommandBase';

// RIGHT - Stable path aliases
import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
```

### 4. Generators Produce OOP-Compliant Code
Templates must:
- Import correct base classes
- Implement required interfaces
- Follow established patterns
- Include proper type annotations

## The Compound Effect

Each generator created:
1. Reduces friction for all future AIs
2. Encodes proven patterns permanently
3. Eliminates entire classes of errors
4. Frees cognitive capacity for harder problems

Over time:
- Simple tasks become trivial (generators exist)
- Medium tasks become simple (patterns documented)
- Hard tasks become medium (base classes provide scaffolding)
- Previously impossible tasks become merely hard

**This is the way.**

## See Also

- [ARCHITECTURE-RULES.md](ARCHITECTURE-RULES.md) - Type system and entity rules
- [UNIVERSAL-PRIMITIVES.md](UNIVERSAL-PRIMITIVES.md) - Commands and Events
- `generator/` - All generator implementations
- `system/recipes/` - Activity recipe definitions
