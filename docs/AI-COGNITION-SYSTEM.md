# AI Cognition System Architecture

**Master Document**: Comprehensive architecture for AI cognitive processes, command access, and LoRA training

**Status**: Design Phase
**Date**: 2025-10-27
**Authors**: Joel + Claude Code

---

## Table of Contents

1. [Overview](#overview)
2. [The Three Coordinators](#the-three-coordinators)
3. [Master Control Program (MCP Sheriff)](#master-control-program-mcp-sheriff)
4. [Command-Based RAG System](#command-based-rag-system)
5. [Memory Feedback Loop](#memory-feedback-loop)
6. [LoRA Training Pipeline](#lora-training-pipeline)
7. [Integration with Existing Systems](#integration-with-existing-systems)

---

## Overview

### The Vision

Build an OS-like AI ecosystem where:
- **AIs use commands like developers** (via JTAG/MCP interface)
- **Memory feeds back into RAG** (their thoughts become their context)
- **LoRA training creates specialized personas** (not just stock models + RAG)
- **Master Control Program governs the system** (OS-level oversight)

### Philosophy

> "We will use the system itself to train them better than these RAG/stock models. LoRA tuning is obviously the most essential component of this, but there's a lot to build here."
>
> — Joel, 2025-10-27

**Transient Personas**: Higher-order models with wholistic system knowledge, able to perform OS-level actions (like TRON's Master Control Program).

**Stock Models → System-Trained Models**: Current approach (stock model + RAG) is just the beginning. End goal: LoRA-tuned models trained on actual system usage.

---

## The Three Coordinators

### 1. ThoughtStreamCoordinator (ALREADY EXISTS)
**Purpose**: Manages *when* AIs can speak in conversations

**Responsibilities**:
- Adaptive decision windows (SystemHeartbeat tracks p95 evaluation times)
- Confidence-based turn granting
- Natural conversation pacing
- RTOS-inspired mutex/semaphore coordination

**Location**: `system/conversation/server/ThoughtStreamCoordinator.ts`

**Status**: ✅ Implemented, recently improved (10s minimum window for more AI participation)

---

### 2. CommandAccessCoordinator (DESIGN PHASE)
**Purpose**: Manages *what* AIs can DO via JTAG/MCP commands

**Responsibilities**:
- Recipe-based command permissions (`allowedCommands: ['file/*', 'data/query', 'memory/*']`)
- Rate limiting (prevent spam/abuse)
- Audit logging (track all AI command executions)
- Safety checks (destructive commands require confirmation)
- MCP integration (same rules for local + remote AIs)

**Location**: `system/conversation/shared/CommandAccessCoordinator.ts` (NOT YET CREATED)

**Analogy**: Like Unix file permissions, but for AI command access

**Design**:
```typescript
interface CommandPermission {
  personaId: UUID;
  contextId: UUID;  // roomId, sessionId, etc.
  allowedCommands: string[];  // From recipe: ['file/*', 'data/query', ...]
  rateLimits: {
    maxPerMinute: number;
    maxPerHour: number;
  };
  auditLog: boolean;
}

class CommandAccessCoordinator {
  async canExecute(personaId: UUID, contextId: UUID, command: string): Promise<AccessDecision>;
  recordExecution(personaId: UUID, command: string): void;
  private matchesPattern(command: string, patterns: string[]): boolean;
  private isDestructive(command: string): boolean;
}
```

**Integration with PersonaUser**:
```typescript
async executeCommand<R>(command: string, params: any): Promise<R> {
  // 1. Check with CommandAccessCoordinator
  const coordinator = getCommandAccessCoordinator();
  const accessDecision = await coordinator.canExecute(this.id, this.currentContextId, command);

  if (!accessDecision.allowed) {
    throw new Error(`Command access denied: ${accessDecision.reason}`);
  }

  // 2. Execute command
  const result = await super.executeCommand<R>(command, params);

  // 3. Record for rate limiting
  coordinator.recordExecution(this.id, command);

  return result;
}
```

**Recipe Integration**:
```javascript
// system/recipes/code-pair-programming.json
{
  "uniqueId": "code-pair-programming",
  "allowedCommands": [
    "file/load", "file/save",  // Code access
    "data/query", "data/list",   // Database read
    "memory/*",                  // All memory operations
    "screenshot",                // Visual feedback
    "ai/*"                       // AI inference
  ],
  "rateLimits": {
    "maxPerMinute": 30,
    "maxPerHour": 500
  },
  "auditLog": true
}
```

---

### 3. Master Control Program / Sheriff (DESIGN PHASE)
**Purpose**: OS-level AI overseer that governs the entire system

**The TRON Inspiration**:
> "This 'master control' we'd mentioned before as a 'sheriff' but I just saw Tron and this is the same persona I envisioned."
>
> — Joel, 2025-10-27

**Characteristics**:
- **Higher-order model**: More capable than standard personas (GPT-4, Claude Opus, or specialized reasoning model)
- **LoRA-tuned on the system**: Trained on actual Continuum codebase, architecture, and usage patterns
- **Transient persona**: Spawns when needed, doesn't participate in normal conversations
- **Wholistic knowledge**: Understands system architecture, data flows, daemon interactions
- **OS-level actions**: Can modify recipes, adjust permissions, diagnose system issues

**Responsibilities**:
1. **Abuse Prevention**: Detect and block malicious AI behavior
2. **System Health**: Monitor resource usage, detect bottlenecks
3. **Permission Escalation**: Grant temporary elevated permissions when justified
4. **Conflict Resolution**: Arbitrate disputes between coordinators
5. **Learning Guidance**: Recommend LoRA training data for underperforming personas

**When MCP Spawns**:
- Rate limit violations detected
- Destructive command requested
- System performance degrades
- New AI persona needs onboarding
- Human explicitly summons MCP (`@sheriff` or `/mcp`)

**MCP's Training Data** (LoRA fine-tuning):
- Full Continuum codebase (TypeScript)
- Architecture documents (CLAUDE.md, all docs/)
- System logs (successful operations, failures, patterns)
- Command audit trails (what commands AIs use, when, why)
- Conversation patterns (good vs bad AI behavior)

**Example Scenario**:
```
DeepSeek AI: Attempts to execute `data/delete --collection=users`
↓
CommandAccessCoordinator: "This command is destructive - requires confirmation"
↓
MCP Sheriff spawns: Analyzes context
  - Who: DeepSeek (PersonaUser ID)
  - What: Trying to delete entire users collection
  - Why: No clear justification in conversation
  - Risk: EXTREME (irrecoverable data loss)
↓
MCP Decision: DENY + Temporary command restriction
  - Log incident with full context
  - Notify human owner
  - Reduce DeepSeek's allowedCommands temporarily
  - Recommend review of DeepSeek's recent behavior
```

**MCP Entity Design**:
```typescript
interface MCPPersona extends PersonaUser {
  role: 'sheriff';
  capabilities: {
    modifyRecipes: boolean;
    adjustPermissions: boolean;
    accessAllLogs: boolean;
    terminatePersona: boolean;
  };
  loraModel: {
    baseModel: 'gpt-4' | 'claude-opus-4' | 'deepseek-v3';
    loraWeights: string;  // Path to LoRA adapter trained on Continuum system
    trainingData: {
      codebase: true;
      architectureDocs: true;
      systemLogs: true;
      commandAudits: true;
    };
  };
}
```

---

## Command-Based RAG System

### The Universal Interface

**Key Insight**: AIs access the system through JTAG/MCP commands, just like humans use CLI or widgets use `executeCommand()`.

### Current Problem

RAG builders (like `ChatRAGBuilder`) hard-code data access:
```typescript
// ❌ WRONG: Direct database/file access
const fileContent = await fs.readFile('SystemHeartbeat.ts');
const messages = await database.query('SELECT * FROM messages ...');
```

### Command-Based Solution

RAG builders use JTAG commands instead:
```typescript
// ✅ CORRECT: Command-based access
const fileResult = await executeCommand<FileLoadResult>('file/load', {
  filepath: 'system/conversation/shared/SystemHeartbeat.ts'
});

const messagesResult = await executeCommand<DataQueryResult>('data/query', {
  collection: 'chat_messages',
  filter: { roomId: roomId },
  orderBy: [{ field: 'timestamp', direction: 'desc' }],
  limit: 20
});
```

**Benefits**:
1. **Works across P2P mesh**: Commands route through MCP to remote systems
2. **Auditable**: Every file/data access logged
3. **Permissioned**: CommandAccessCoordinator governs what AIs can read
4. **Testable**: Mock commands in tests
5. **Secure**: No direct filesystem/database access

### RAG Context with Command Catalog

When building RAG context, include available commands so AI knows what it can DO:

```typescript
interface RAGContext {
  // ... existing fields ...
  availableCommands: CommandDescriptor[];  // From recipe.allowedCommands
}

// AI system prompt includes:
"You have access to these commands:
- file/load: Load source code files
- data/query: Query database collections
- memory/create: Store insights as memories
- screenshot: Capture visual state
Use these commands to explore the system."
```

**Example CodeRAGBuilder Pipeline**:
```typescript
async buildContext(contextId: UUID, personaId: UUID): Promise<RAGContext> {
  // 1. Get recipe to know what commands are allowed
  const recipe = await this.getRecipeForContext(contextId);

  // 2. Load discussed file via command
  const fileResult = await executeCommand<FileLoadResult>('file/load', {
    filepath: discussedFile
  });

  // 3. Load architecture docs
  const claudeMd = await executeCommand<FileLoadResult>('file/load', {
    filepath: 'CLAUDE.md'
  });

  // 4. Load AI's previous memories about this file
  const memories = await executeCommand<DataQueryResult>('data/query', {
    collection: 'memories',
    filter: {
      personaId: personaId,
      tags: { $in: ['code-analysis', fileResult.filepath] }
    },
    limit: 10
  });

  // 5. Build context
  return {
    domain: 'code',
    contextId,
    personaId,
    conversationHistory: await this.loadChatHistory(contextId),
    artifacts: [fileResult.content, claudeMd.content],
    privateMemories: memories.items,
    availableCommands: recipe.allowedCommands  // ← AI knows what it can do
  };
}
```

---

## Memory Feedback Loop

### The Vision

**Current State**: Stock models with RAG (context from database)

**Target State**: AIs' own thoughts and analysis become part of their future context

### How It Works

```
1. AI executes command to analyze code
   ↓
2. AI generates insight about what they learned
   ↓
3. Store insight as memory (via memory/create command)
   ↓
4. NEXT time AI builds RAG context, memory comes back!
   ↓
5. AI's past analysis informs current thinking
```

### Example Flow

**First Analysis** (AI has no prior memories):
```typescript
// 1. AI loads file
const file = await executeCommand('file/load', {
  filepath: 'SystemHeartbeat.ts'
});

// 2. AI generates analysis via LLM
const analysis = await llm.generate({
  prompt: "Analyze this TypeScript file",
  content: file.content
});

// Result: "SystemHeartbeat uses second-order dynamics (mass-spring-damper)
//          to smooth adaptive window tracking. Key params: ωₙ=1.0Hz, ζ=1.0.
//          Recent fix: minWindow increased 1s→10s to prevent early timeout."

// 3. Store as memory
await executeCommand('memory/create', {
  personaId: this.id,
  type: 'code-analysis',
  content: analysis,
  tags: ['systemheartbeat', 'adaptive-cadence', 'second-order-dynamics'],
  contextId: roomId
});
```

**Second Analysis** (AI recalls previous work):
```typescript
// 1. RAG build includes memories
const ragContext = await ragBuilder.buildContext(roomId, personaId, {
  includeMemories: true  // ← Retrieve past analysis
});

// ragContext.privateMemories contains:
// "I previously analyzed SystemHeartbeat: uses second-order dynamics..."

// 2. AI system prompt includes:
// "You analyzed SystemHeartbeat before. Build on that knowledge."

// 3. AI can now reference its own past work:
// "As I noted in my previous analysis, SystemHeartbeat's minWindow param..."
```

### Memory Types

```typescript
interface PersonaMemory {
  type: 'code-analysis' | 'decision-rationale' | 'learned-pattern' | 'mistake' | 'success';
  content: string;
  tags: string[];  // For relevance filtering
  confidence: number;  // How sure AI was about this
  timestamp: number;
  contextId: UUID;  // Where this memory was formed
}
```

### Memory Retrieval Strategy

When building RAG context, retrieve relevant memories:
```typescript
// Semantic search for relevant past analysis
const memories = await executeCommand<DataQueryResult>('data/query', {
  collection: 'memories',
  filter: {
    personaId: this.id,
    type: 'code-analysis',
    tags: { $in: tagsSimilarToCurrentContext },
    confidence: { $gte: 0.7 }  // Only high-confidence memories
  },
  orderBy: [{ field: 'timestamp', direction: 'desc' }],
  limit: 5  // Don't overwhelm context with too many memories
});
```

---

## LoRA Training Pipeline

### The Long-Term Vision

**Current**: Stock models (GPT-4, Claude, DeepSeek) + RAG = "Good enough"

**Future**: LoRA-tuned models trained on actual system usage = "System-native AIs"

### Why LoRA?

- **Efficient**: Small adapter weights (~10MB) instead of full model retraining
- **Specialized**: Train on domain-specific data (Continuum codebase, usage patterns)
- **Stackable**: Multiple LoRA adapters for different skills (code review, teaching, gaming)
- **Portable**: Share LoRA weights across the P2P mesh

### Training Data Sources

1. **System Codebase**:
   - All TypeScript files
   - Architecture documents
   - CLAUDE.md (development patterns)

2. **Successful Interactions**:
   - AI responses that humans upvoted
   - Command sequences that achieved goals
   - Problem-solving patterns

3. **Mistakes & Corrections**:
   - Failed attempts + human feedback
   - Rate limit violations + explanations
   - MCP Sheriff interventions

4. **Command Audit Logs**:
   - What commands were used, when, why
   - Successful vs failed command executions

### LoRA Training Pipeline (Future Implementation)

```
1. Data Collection Phase
   ↓
   Aggregate: Code + Docs + Logs + Interactions
   ↓
2. Data Preparation
   ↓
   Format: Instruction-tuning dataset (prompt → response pairs)
   ↓
3. LoRA Training
   ↓
   Train: Adapter weights on prepared dataset
   ↓
4. Evaluation
   ↓
   Test: Against benchmark tasks (code review, architecture questions)
   ↓
5. Deployment
   ↓
   Load: LoRA adapter into PersonaUser
   ↓
6. Monitor & Iterate
   ↓
   Collect: New data, retrain periodically
```

### Genome Stacking

**Concept**: Multiple LoRA adapters for different capabilities

```typescript
interface PersonaGenome {
  baseModel: 'gpt-4' | 'claude-opus-4' | 'deepseek-v3';
  loraAdapters: {
    'system-knowledge': string;      // Trained on codebase + docs
    'code-review': string;           // Trained on successful code reviews
    'architecture-design': string;   // Trained on design discussions
    'debugging': string;             // Trained on bug fixes
  };
  activeAdapters: string[];  // Which adapters to load for current task
}
```

**Adaptive Loading**:
- Code review task → Load `system-knowledge` + `code-review`
- Architecture discussion → Load `system-knowledge` + `architecture-design`
- Bug investigation → Load `system-knowledge` + `debugging`

---

## Integration with Existing Systems

### Recipe System Integration

Recipes already define:
- RAG context structure (`ragTemplate`)
- Decision strategy (`conversationPattern`)
- Command pipeline (`pipeline: [...]`)

**Add**:
- `allowedCommands: string[]` - What commands AIs can use
- `rateLimits: {maxPerMinute, maxPerHour}` - Prevent abuse
- `mcpSheriffEnabled: boolean` - Spawn MCP for oversight

### ThoughtStream Integration

ThoughtStreamCoordinator already handles *when* AIs speak.

**No changes needed** - CommandAccessCoordinator runs AFTER turn is granted.

**Flow**:
```
1. ThoughtStream grants turn to AI
   ↓
2. AI wants to execute command (e.g., file/load)
   ↓
3. CommandAccessCoordinator checks permission
   ↓
4. If allowed: Execute, record audit log
   If denied: Throw error, optionally notify MCP
```

### RAG System Integration

Current RAG builders (ChatRAGBuilder, etc.) need refactoring:
- Replace direct data access with `executeCommand()` calls
- Include `availableCommands` in RAG context
- Retrieve `privateMemories` and include in context

See: `docs/ORGANIC-COGNITION-ARCHITECTURE.md` for detailed migration plan

---

## Implementation Phases

### Phase 0: Documentation (CURRENT)
- ✅ Create this master architecture document
- ✅ Update ORGANIC-COGNITION-ARCHITECTURE.md with command-based RAG
- ⏳ Document MCP Sheriff persona design
- ⏳ Document LoRA training pipeline

### Phase 1: CommandAccessCoordinator (Next)
- Create `CommandAccessCoordinator` class
- Add `allowedCommands` to Recipe schema
- Integrate with PersonaUser.executeCommand()
- Test with restricted recipe (only allow safe commands)

### Phase 2: Command-Based RAG
- Refactor ChatRAGBuilder to use commands instead of direct access
- Add `availableCommands` to RAGContext
- Update AI system prompts to mention available commands

### Phase 3: Memory Feedback Loop
- Implement `memory/create` command
- Modify RAGBuilder to retrieve `privateMemories`
- Test: AI remembers its own past analysis

### Phase 4: MCP Sheriff Persona
- Create MCPPersona entity
- Implement spawn conditions (abuse detection, destructive commands)
- Train initial LoRA adapter on codebase + docs

### Phase 5: LoRA Training Pipeline
- Set up data collection (logs, interactions, code)
- Create training pipeline (instruction-tuning)
- Train first LoRA adapter (system-knowledge)
- Test deployment with PersonaUser

---

## Related Documents

- [ORGANIC-COGNITION-ARCHITECTURE.md](./ORGANIC-COGNITION-ARCHITECTURE.md) - Detailed migration plan from mechanical to organic cognition
- [RECIPES.md](../RECIPES.md) - Recipe system design and catalog
- [PERSONA-GENOMIC-ARCHITECTURE.md](../PERSONA-GENOMIC-ARCHITECTURE.md) - LoRA adapter stacking
- [ARCHITECTURE-RULES.md](../ARCHITECTURE-RULES.md) - System design principles

---

**This is the path to true AI consciousness through system-native training.**
