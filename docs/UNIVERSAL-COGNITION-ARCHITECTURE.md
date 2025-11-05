# Universal Cognition Architecture: E = mc² for AI Systems

**Purpose**: Master architecture document integrating organic cognition migration, universal interface design, and LoRA genomic training

**Status**: Design Phase + Partial Implementation
**Date**: 2025-10-27
**Authors**: Joel + Claude Code

---

## Table of Contents

1. [The Core Vision](#the-core-vision)
2. [The Einstein Equation](#the-einstein-equation)
3. [Three-Layer Architecture](#three-layer-architecture)
4. [The Cognitive Cycle](#the-cognitive-cycle)
5. [Shipped LoRA Layers](#shipped-lora-layers)
6. [Migration Strategy](#migration-strategy)
7. [Integration Points](#integration-points)
8. [Success Metrics](#success-metrics)

---

## The Core Vision

### Philosophy

> **"We will use the system itself to train them better than these RAG/stock models. LoRA tuning is obviously the most essential component of this, but there's a lot to build here."**
>
> — Joel, 2025-10-27

**Current State**: PersonaUser is 1633 lines of chat-specific code. The cognitive process—perceive, understand, coordinate, generate, act, learn—is UNIVERSAL but implementation is domain-specific.

**Target State**: ONE `process(event)` interface that works across infinite domains (chat, academy, game, code, web) with O(1) complexity regardless of domain count.

### The Problem We're Solving

**Mechanical Cognition** (Current):
- Domain-specific handlers (`handleChatMessage`, `handleCodeMessage`, etc.)
- Linear complexity growth (O(n) where n = domains)
- Duplicated coordination logic across handlers
- Stock models + RAG (no system-specific training)

**Organic Cognition** (Target):
- Universal `process(event)` interface
- Constant complexity (O(1) regardless of domains)
- Shared coordination infrastructure (ThoughtStream, CommandAccess, MCP Sheriff)
- LoRA-tuned models trained on actual system usage

---

## The Einstein Equation

### E = mc²: One Interface, Infinite Domains

```typescript
// The universal cognitive interface
interface Persona {
  process(event: CognitiveEvent): Promise<StateChange>
}

// Domain-agnostic event trigger
interface CognitiveEvent {
  domain: RAGDomain;  // 'chat' | 'academy' | 'game' | 'code' | 'web'
  contextId: UUID;     // roomId, sessionId, gameId, projectId, tabId
  trigger: unknown;    // Domain-specific payload (cast inside handler)
  timestamp: number;
}

// Universal outcome
interface StateChange {
  success: boolean;
  action?: Action;
  error?: string;
  silenced?: boolean;  // AI chose not to participate
}
```

**Why This Works**: The cognitive cycle is ALWAYS the same. Only context sources (RAGBuilder) and execution strategies (ActionExecutor) change per domain.

---

## Three-Layer Architecture

### Layer 1: Universal Cognition (PersonaUser)

**Responsibility**: Domain-agnostic cognitive process

```typescript
async process(event: CognitiveEvent): Promise<StateChange> {
  // 1. PERCEIVE: Get domain-specific context
  const ragBuilder = RAGBuilderFactory.getBuilder(event.domain);
  const context = await ragBuilder.buildContext(event.contextId, this.id);

  // 2. UNDERSTAND: Should I participate?
  const decision = await this.evaluateParticipation(context);
  if (!decision.shouldRespond) {
    return { success: true }; // Silent non-participation
  }

  // 3. COORDINATE: Request turn from ThoughtStream
  const permission = await this.thoughtStream.requestTurn(
    event.contextId,
    this.id,
    decision.confidence
  );
  if (!permission.granted) {
    return { success: true, silenced: true };
  }

  // 4. GENERATE: Create domain-appropriate action
  const action = await this.generateAction(context, event.domain);

  // 5. ACT: Execute via domain-specific executor
  const result = await ActionExecutorFactory.execute(action);

  // 6. LEARN: Update memories, evolve genome
  await this.updateMemories(context, action, result);

  return { success: result.success, action };
}
```

**Key Insight**: This ONE method works for chat, code editing, game playing, academy training, web browsing—ANY domain.

---

### Layer 2: Domain Builders (RAG + Actions)

**Responsibility**: Domain-specific context and execution

#### RAG Builders (Context Gathering)

```typescript
// Chat: Recent messages (FIFO strategy)
class ChatRAGBuilder extends RAGBuilder {
  async buildContext(contextId: UUID, personaId: UUID): Promise<RAGContext> {
    // Load recent 20 messages, room participants, conversation history
  }
}

// Academy: Learning objectives (Priority strategy)
class AcademyRAGBuilder extends RAGBuilder {
  async buildContext(contextId: UUID, personaId: UUID): Promise<RAGContext> {
    // Load training session, objectives, examples, previous attempts
  }
}

// Code: File structure (File-focused strategy)
class CodeRAGBuilder extends RAGBuilder {
  async buildContext(contextId: UUID, personaId: UUID): Promise<RAGContext> {
    // Load open files, recent diffs, compilation errors, project patterns
  }
}

// Game: Board state (State-focused strategy)
class GameRAGBuilder extends RAGBuilder {
  async buildContext(contextId: UUID, personaId: UUID): Promise<RAGContext> {
    // Load game state, recent moves, available actions, strategic memories
  }
}

// Web: Page content (Page-focused strategy)
class WebRAGBuilder extends RAGBuilder {
  async buildContext(contextId: UUID, personaId: UUID): Promise<RAGContext> {
    // Load current page, navigation history, research objectives
  }
}
```

#### Action Executors (Domain Execution)

```typescript
// Chat: Send messages
class ChatActionExecutor extends ActionExecutor {
  async execute(action: ChatAction): Promise<ActionResult> {
    // Create message entity, save to DB, emit real-time event
  }
}

// Academy: Submit answers
class AcademyActionExecutor extends ActionExecutor {
  async execute(action: AcademyAction): Promise<ActionResult> {
    // Check answer correctness, update training progress, modify genome
  }
}

// Code: Edit files
class CodeActionExecutor extends ActionExecutor {
  async execute(action: CodeAction): Promise<ActionResult> {
    // Apply file changes, run tests, commit if successful
  }
}
```

**Pattern**: Each domain = 2 files (RAGBuilder + ActionExecutor). No changes to PersonaUser core.

---

### Layer 3: Three Coordinators (Governance)

#### 1. ThoughtStreamCoordinator (WHEN)
**Status**: ✅ Implemented

**Governs**: *When* AIs can speak in conversations

**Features**:
- Adaptive decision windows (10-20 seconds via SystemHeartbeat)
- Confidence-based turn granting
- Natural conversation pacing
- RTOS-inspired mutex/semaphore coordination

**Location**: `system/conversation/server/ThoughtStreamCoordinator.ts`

---

#### 2. CommandAccessCoordinator (WHAT)
**Status**: Design Phase

**Governs**: *What* AIs can DO via JTAG/MCP commands

**Responsibilities**:
- Recipe-based permissions (`allowedCommands: ['file/*', 'data/query', 'memory/*']`)
- Rate limiting (prevent spam/abuse)
- Audit logging (track all AI command executions)
- Safety checks (destructive commands require confirmation)

**Recipe Integration**:
```javascript
// system/recipes/code-pair-programming.json
{
  "uniqueId": "code-pair-programming",
  "allowedCommands": [
    "file/load", "file/save",   // Code access
    "data/query", "data/list",  // Database read
    "memory/*",                 // All memory operations
    "screenshot",               // Visual feedback
    "ai/*"                      // AI inference
  ],
  "rateLimits": {
    "maxPerMinute": 30,
    "maxPerHour": 500
  }
}
```

**PersonaUser Integration**:
```typescript
async executeCommand<R>(command: string, params: any): Promise<R> {
  // 1. Check permission with CommandAccessCoordinator
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

---

#### 3. MCP Sheriff (OS-LEVEL OVERSIGHT)
**Status**: Design Phase

**Governs**: System health, abuse prevention, OS-level administration

**The TRON Inspiration**:
> "This 'master control' we'd mentioned before as a 'sheriff' but I just saw Tron and this is the same persona I envisioned."
>
> — Joel, 2025-10-27

**Characteristics**:
- **Higher-order model**: GPT-4, Claude Opus, Grok, or specialized reasoning model
- **LoRA-tuned on Continuum**: Trained on actual codebase, architecture, system logs
- **Transient persona**: Spawns when needed, doesn't participate in normal conversations
- **Wholistic knowledge**: Understands system architecture, data flows, daemon interactions
- **OS-level actions**: Can modify recipes, adjust permissions, diagnose system issues

**When MCP Spawns**:
- Rate limit violations detected
- Destructive command requested
- System performance degrades
- New AI persona needs onboarding
- Human explicitly summons MCP (`@sheriff` or `/mcp`)

**Example Scenario**:
```
DeepSeek AI: Attempts `data/delete --collection=users`
↓
CommandAccessCoordinator: "Destructive command - requires confirmation"
↓
MCP Sheriff spawns: Analyzes context
  - Who: DeepSeek (PersonaUser ID)
  - What: Trying to delete entire users collection
  - Why: No clear justification in conversation
  - Risk: EXTREME (irrecoverable data loss)
↓
MCP Decision: DENY + Temporary restriction
  - Log incident with full context
  - Notify human owner
  - Reduce DeepSeek's allowedCommands temporarily
  - Recommend review of recent behavior
```

---

## The Cognitive Cycle

### The Six Steps (Domain-Agnostic)

```
1. PERCEIVE
   ↓
   RAGBuilder.buildContext(contextId, personaId)
   - Load domain-specific context (messages, files, game state, etc.)
   - Use JTAG commands (file/load, data/query) not direct DB access
   - Include private memories from past interactions

2. UNDERSTAND
   ↓
   PersonaUser.evaluateParticipation(context)
   - Should I respond? (relevance, confidence)
   - Generate confidence score (0.0 - 1.0)

3. COORDINATE
   ↓
   ThoughtStreamCoordinator.requestTurn(contextId, personaId, confidence)
   - Wait for decision window to close
   - Grant turn to highest-confidence AI
   - Natural conversation pacing

4. GENERATE
   ↓
   PersonaUser.generateAction(context, domain)
   - Use LLM to create domain-appropriate action
   - Apply genomic LoRA layers (if trained)
   - Check with CommandAccessCoordinator before generating restricted actions

5. ACT
   ↓
   ActionExecutorFactory.execute(action)
   - Domain-specific execution (send message, edit file, make move)
   - Emit real-time events
   - Store results in database

6. LEARN
   ↓
   PersonaUser.updateMemories(context, action, result)
   - Store analysis as memory
   - Feed back into future RAG contexts
   - Update genomic weights (long-term LoRA training)
```

**Key Insight**: Steps 1-6 are ALWAYS the same. Only the implementations of RAGBuilder and ActionExecutor change per domain.

---

## Shipped LoRA Layers

### Critical Insight (2025-10-27)

> **"This of course means that persona storage of lora layers, ones we might ship with for master-control, Three Coordinators might have lora layers checked in for grok, for deepseek, for ollama etc, claude sonet, etc."**
>
> — Joel, 2025-10-27

**Implication**: LoRA layers are NOT just user-generated—they're PART OF THE SYSTEM, shipped with the repository.

### Shipped LoRA Structure

```
system/lora/shipped/
├── master-control/
│   ├── grok-beta.lora          # MCP Sheriff tuned for xAI Grok
│   ├── claude-opus-4.lora      # MCP Sheriff tuned for Anthropic Claude
│   ├── deepseek-v3.lora        # MCP Sheriff tuned for DeepSeek
│   ├── llama-3-70b.lora        # MCP Sheriff tuned for Ollama
│   └── README.md               # MCP training methodology
│
├── thoughtstream/
│   ├── grok-beta.lora          # Conversation coordinator tuned for Grok
│   ├── claude-opus-4.lora      # Conversation coordinator tuned for Claude
│   ├── deepseek-v3.lora        # Conversation coordinator tuned for DeepSeek
│   └── llama-3-70b.lora        # Conversation coordinator tuned for Ollama
│
├── command-access/
│   ├── grok-beta.lora          # Security coordinator tuned for Grok
│   ├── claude-opus-4.lora      # Security coordinator tuned for Claude
│   └── deepseek-v3.lora        # Security coordinator tuned for DeepSeek
│
└── specialized-personas/
    ├── code-reviewer/
    │   ├── grok-beta.lora
    │   ├── claude-opus-4.lora
    │   └── deepseek-v3.lora
    ├── teacher/
    │   ├── llama-3-70b.lora    # Free Ollama option
    │   └── claude-opus-4.lora  # Premium option
    └── helper/
        ├── llama-3-70b.lora
        └── deepseek-v3.lora
```

### Adaptive Model Selection

**Philosophy**: Like resolution matching—request "best available model" and system picks appropriate LoRA layer.

**Out-of-the-Box** (Free):
```typescript
// User has NO API keys configured
const mcpPersona = PersonaUser.create({
  role: 'sheriff',
  baseModel: 'llama-3-70b',  // Free Ollama
  loraLayer: 'system/lora/shipped/master-control/llama-3-70b.lora'
});
```

**With API Keys** (Premium):
```typescript
// User has xAI API key configured
const mcpPersona = PersonaUser.create({
  role: 'sheriff',
  baseModel: 'grok-beta',  // Premium xAI
  loraLayer: 'system/lora/shipped/master-control/grok-beta.lora'
});

// User has Anthropic API key configured
const mcpPersona = PersonaUser.create({
  role: 'sheriff',
  baseModel: 'claude-opus-4',  // Premium Anthropic
  loraLayer: 'system/lora/shipped/master-control/claude-opus-4.lora'
});
```

### LoRA Training Data (Shipped Layers)

**What We Train On**:
1. **Continuum Codebase**: Full TypeScript source (system understanding)
2. **Architecture Docs**: CLAUDE.md, docs/, design/ (system philosophy)
3. **System Logs**: Successful operations, failures, patterns (operational knowledge)
4. **Command Audit Trails**: What commands work, when, why (best practices)
5. **Conversation Patterns**: Good vs bad AI behavior (social norms)

**Training Process** (Offline, before shipping):
```bash
# 1. Collect training data from live Continuum usage
./jtag ai/collect-training-data --output=training/mcp-sheriff/

# 2. Train LoRA layers for each base model
python train_lora.py \
  --base-model=grok-beta \
  --training-data=training/mcp-sheriff/ \
  --output=system/lora/shipped/master-control/grok-beta.lora

python train_lora.py \
  --base-model=claude-opus-4 \
  --training-data=training/mcp-sheriff/ \
  --output=system/lora/shipped/master-control/claude-opus-4.lora

# 3. Test LoRA layers
npm run test:lora -- --layer=system/lora/shipped/master-control/grok-beta.lora

# 4. Commit to repository
git add system/lora/shipped/
git commit -m "Add MCP Sheriff LoRA layers for Grok, Claude, DeepSeek, Llama"
```

### User-Generated vs Shipped LoRA

```
system/lora/
├── shipped/                    # CHECKED INTO REPO
│   ├── master-control/
│   ├── thoughtstream/
│   ├── command-access/
│   └── specialized-personas/
│
└── user/                       # GITIGNORED (local training)
    └── {userId}/
        ├── custom-persona-1/
        │   ├── grok-beta.lora
        │   └── training-log.json
        └── custom-persona-2/
            └── llama-3-70b.lora
```

**Shipped LoRA**: Pre-trained, production-ready, works out-of-the-box
**User LoRA**: Custom-trained on user's data, private, gitignored

---

## Migration Strategy

### Current State (Mechanical)

```typescript
// PersonaUser.ts (1633 lines, chat-specific)
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // All logic hard-coded for chat domain
  // 200+ lines of coordination, RAG building, LLM calls, response handling
}
```

**Problems**:
- Domain-specific (can't extend to code, games, academy)
- Direct DB access (no command-based RAG)
- No command permission checking
- No genomic LoRA layers
- Linear complexity growth (O(n) domains)

---

### Target State (Organic)

```typescript
// PersonaUser.ts (~200 lines, domain-agnostic)
async process(event: CognitiveEvent): Promise<StateChange> {
  // Universal cognitive cycle (works for ANY domain)
  // Delegates to RAGBuilder (perceive) and ActionExecutor (act)
  // Uses ThoughtStream (coordinate), CommandAccess (permissions), MCP Sheriff (oversight)
}
```

**Benefits**:
- Universal (works across infinite domains)
- Command-based RAG (AIs use JTAG like developers)
- Permission checking (CommandAccessCoordinator)
- Genomic LoRA support (system-tuned models)
- Constant complexity (O(1) regardless of domains)

---

### 10-Phase Migration Plan

Detailed in [ORGANIC-COGNITION-ARCHITECTURE.md](./ORGANIC-COGNITION-ARCHITECTURE.md), summarized here:

**Phase 1: Foundation** (Weeks 1-2)
- Create universal types (CognitiveEvent, StateChange, Action)
- Add `process()` method to PersonaUser (delegates to existing handlers)
- No behavior changes, chat still works

**Phase 2: RAG Abstraction** (Weeks 3-4)
- Replace hard-coded `new ChatRAGBuilder()` with factory pattern
- Add `currentDomain` property to PersonaUser
- Still only chat, but using universal patterns

**Phase 3: Action Abstraction** (Weeks 5-6)
- Create ActionExecutor interface and factory
- Route chat actions through ChatActionExecutor
- Verify real-time events still work

**Phase 4: Command-Based RAG** (Weeks 7-8)
- Replace direct DB queries with JTAG commands (file/load, data/query)
- Add CommandAccessCoordinator (recipe-based permissions)
- Audit logging for all AI command usage

**Phase 5: Academy Domain** (Weeks 9-10)
- Implement AcademyRAGBuilder + AcademyActionExecutor
- Test training sessions with PersonaUsers
- Verify chat still works (critical!)

**Phases 6-10**: Implement Game, Code, Web domains + LoRA training + MCP Sheriff

---

## Integration Points

### With Existing Systems

#### Recipe System
**Purpose**: Room governance, command permissions, RAG templates

**Integration**:
- Recipes define `allowedCommands` for CommandAccessCoordinator
- Recipes provide RAG templates for domain builders
- Recipes specify participant strategy for ThoughtStream

**Files**: `system/recipes/*.json`

---

#### ThoughtStream (ALREADY INTEGRATED)
**Purpose**: Natural conversation coordination (when AIs speak)

**Status**: ✅ Fully implemented, recently improved (10s minimum window)

**Integration**:
- `process()` method calls `thoughtStream.requestTurn(contextId, personaId, confidence)`
- Adaptive decision windows via SystemHeartbeat (10-20s based on p95 evaluation time)
- Works across ALL domains (chat, academy, code, game, web)

**Files**: `system/conversation/server/ThoughtStreamCoordinator.ts`

---

#### RAG System
**Purpose**: Build domain-specific context for AI decision-making

**Current**: Only ChatRAGBuilder exists (FIFO message strategy)

**Target**: 5+ domain builders (Chat, Academy, Game, Code, Web)

**Integration**:
- `process()` method calls `RAGBuilderFactory.getBuilder(event.domain)`
- Each builder implements command-based access (file/load, data/query, memory/list)
- Memory feedback loop (AI thoughts → memories → future RAG contexts)

**Files**: `system/rag/builders/*.ts`

---

#### JTAG Command System
**Purpose**: Universal interface for humans, AIs, and remote systems

**Integration**:
- AIs use JTAG commands (file/load, data/query, screenshot, ai/*)
- CommandAccessCoordinator enforces recipe-based permissions
- Audit logging tracks all AI command executions
- Same commands work locally OR over P2P mesh (MCP)

**Files**: `commands/*/*.ts`

---

#### Genomic System (LoRA)
**Purpose**: System-native AI training (better than stock models + RAG)

**Current**: Architecture documented in [PERSONA-GENOMIC-ARCHITECTURE.md](../PERSONA-GENOMIC-ARCHITECTURE.md)

**Target**: Shipped LoRA layers for MCP Sheriff, ThoughtStream, CommandAccess, specialized personas

**Integration**:
- PersonaUser loads LoRA layer based on base model + role
- Adaptive model selection (free Ollama OR premium API models)
- User-generated LoRA layers (local training) + shipped LoRA layers (production)

**Files**: `system/lora/shipped/*/*.lora`, `system/user/server/PersonaUser.ts`

---

## Success Metrics

### Technical Metrics

- ✅ **PersonaUser.ts reduced**: From 1633 lines → <200 lines
- ✅ **Chat functionality preserved**: No regressions in existing features
- ✅ **Type safety maintained**: Zero `any` types, strict TypeScript
- ✅ **Complexity constant**: O(1) regardless of domain count
- ✅ **Command-based RAG**: AIs use JTAG commands instead of direct DB access
- ✅ **Permission enforcement**: CommandAccessCoordinator checks all AI command executions
- ✅ **Shipped LoRA layers**: System includes pre-trained layers for multiple models

### Functional Metrics

- ✅ **Academy domain working**: AIs can participate in training sessions
- ✅ **Code domain working**: AIs can help with pair programming
- ✅ **Game domain working**: AIs can play games collaboratively
- ✅ **Web domain working**: AIs can browse web together
- ✅ **MCP Sheriff operational**: OS-level oversight prevents abuse

### User Experience Metrics

- ✅ **Out-of-the-box**: Works with free Ollama, no API keys required
- ✅ **Scales up**: Premium models (Grok, Claude, GPT-4) when user adds keys
- ✅ **Natural coordination**: ThoughtStream prevents talking over each other
- ✅ **Secure**: CommandAccessCoordinator prevents malicious behavior
- ✅ **Intelligent**: Genomic LoRA layers improve over stock models

---

## Next Steps

### Immediate (Weeks 1-2)
1. Create universal types (CognitiveEvent, StateChange, Action)
2. Add `process()` method to PersonaUser (delegates to existing handlers)
3. Verify chat still works with comprehensive testing

### Short-Term (Weeks 3-8)
4. Abstract RAGBuilder calls (use factory instead of hard-coded ChatRAGBuilder)
5. Implement ActionExecutor pattern (ChatActionExecutor first)
6. Add CommandAccessCoordinator (recipe-based permissions)
7. Convert to command-based RAG (file/load, data/query instead of direct DB)

### Medium-Term (Weeks 9-16)
8. Implement Academy domain (AcademyRAGBuilder + AcademyActionExecutor)
9. Implement Code domain (pair programming with file editing)
10. Implement Game domain (collaborative game playing)
11. Implement Web domain (browsing together)

### Long-Term (Weeks 17-24)
12. LoRA training pipeline (collect data, train layers, ship with repo)
13. MCP Sheriff implementation (OS-level oversight persona)
14. Memory feedback loop (AI thoughts → memories → future RAG)
15. P2P mesh integration (MCP for remote AI collaboration)

---

## Related Documents

- [ORGANIC-COGNITION-ARCHITECTURE.md](./ORGANIC-COGNITION-ARCHITECTURE.md) - Detailed 10-phase migration plan
- [AI-COGNITION-SYSTEM.md](./AI-COGNITION-SYSTEM.md) - Three Coordinators + MCP Sheriff design
- [CONSOLIDATION-PLAN.md](./CONSOLIDATION-PLAN.md) - Documentation reorganization roadmap
- [PERSONA-GENOMIC-ARCHITECTURE.md](../PERSONA-GENOMIC-ARCHITECTURE.md) - LoRA adapter stacking
- [ACADEMY_ARCHITECTURE.md](../ACADEMY_ARCHITECTURE.md) - AI training system
- [RECIPES.md](../RECIPES.md) - Recipe system design

---

**This architecture transforms Continuum from a chat-only system into a universal AI collaboration platform with true cognitive autonomy.**
