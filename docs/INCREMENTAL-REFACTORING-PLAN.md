# Incremental Refactoring Plan: Never Break AI Responses

**Purpose**: Pragmatic, step-by-step refactoring that maintains working AI responses at EVERY commit

**Status**: Planning Phase
**Date**: 2025-10-27
**Principle**: **ZERO parallel design. Modularize existing code FIRST, then add new capabilities incrementally.**

---

## Table of Contents

1. [Core Principle: AI Responses First](#core-principle-ai-responses-first)
2. [Current Pain Points](#current-pain-points)
3. [Modularization Strategy](#modularization-strategy)
4. [Command-First Approach](#command-first-approach)
5. [Phase-by-Phase Breakdown](#phase-by-phase-breakdown)
6. [Testing Protocol](#testing-protocol)

---

## Core Principle: AI Responses First

### The Golden Rule

**NEVER commit code that breaks AI responses, even for a single commit.**

Every phase must:
1. ✅ Compile successfully (`npx tsc --noEmit`)
2. ✅ Deploy successfully (`npm start`)
3. ✅ AI responses work (`./jtag debug/chat-send` + verify responses)
4. ✅ All existing tests pass (`npm test`)

### No Parallel Universes

**DO NOT**:
- Create new architecture alongside old architecture
- Build universal `process()` while `handleChatMessage()` exists
- Implement new RAG builders without using them immediately

**DO**:
- Extract methods from existing files into modules
- Replace inline code with module calls **in same commit**
- Add ONE new capability at a time, fully integrated

---

## Current Pain Points

### PersonaUser.ts (2004 lines)

**Problems**:
- Mixing concerns: RAG building, LLM calls, response handling, rate limiting, genome management
- Everything in one file
- Hard to test individual components
- Difficult to extend or modify without breaking everything

**Extractable Modules** (in order of safety):
1. **Rate Limiting** (~100 lines)
   - `isRateLimited()`, `lastResponseTime`, `responseCount`, `minSecondsBetweenResponses`
   - Pure utility, no side effects
   - Easy to test in isolation

2. **Response Cleaning** (~50 lines)
   - `cleanAIResponse()`, `isResponseRedundant()`
   - Pure functions, no dependencies
   - Can extract without risk

3. **RAG Context Management** (~200 lines)
   - `storeRAGContext()`, `loadRAGContext()`, `updateRAGContext()`
   - Already semi-isolated
   - Uses commands internally (good!)

4. **Decision Heuristics** (~150 lines)
   - `isPersonaMentioned()`, `getPersonaDomainKeywords()`, `calculateResponseHeuristics()`
   - Complex but contained
   - No external side effects

5. **LLM Interaction** (~400 lines)
   - `evaluateShouldRespond()`, `respondToMessage()`
   - Core functionality - extract LAST
   - High risk, must be careful

### UserDaemon.ts (shared/)

**Problems**:
- User factory logic mixed with lifecycle management
- Circular dependencies with PersonaUser
- Hard to test user creation in isolation

**Extractable Modules**:
1. **UserFactory** - Create users based on type (Human, Agent, Persona)
2. **UserRegistry** - Track active users, lookup by ID
3. **UserLifecycle** - Initialize, shutdown, cleanup

---

## Modularization Strategy

### Phase 0: Establish Baseline (Week 0)

**Goal**: Document current behavior, create comprehensive tests

**Actions**:
1. Add integration test: "AI responds to chat messages"
2. Add integration test: "Multiple AIs coordinate via ThoughtStream"
3. Document current PersonaUser call flow (from event → response)
4. Create test fixtures (sample messages, rooms, personas)

**Test Protocol**:
```bash
# Baseline test (run BEFORE any refactoring)
./jtag debug/chat-send --roomId="<ROOM>" --message="Baseline test - $(date +%s)"
sleep 15
./jtag debug/logs --filterPattern="AI-RESPONSE|Worker evaluated" --tailLines=30

# Expected: See 5-10 AIs evaluate, 1-2 respond
```

**Deliverable**: `tests/integration/persona-response-baseline.test.ts`

**Commit**: "Add baseline integration tests before refactoring"

---

### Phase 1: Extract Rate Limiting (Week 1)

**Goal**: Move rate limiting logic into separate module

**File to Create**: `system/user/server/modules/RateLimiter.ts`

```typescript
export class RateLimiter {
  private lastResponseTime: Map<UUID, Date> = new Map();
  private responseCount: Map<UUID, number> = new Map();

  constructor(
    private minSecondsBetweenResponses: number = 10,
    private maxResponsesPerSession: number = 50
  ) {}

  isRateLimited(contextId: UUID): boolean {
    // Extract exact logic from PersonaUser.isRateLimited()
  }

  recordResponse(contextId: UUID): void {
    this.lastResponseTime.set(contextId, new Date());
    this.responseCount.set(contextId, (this.responseCount.get(contextId) || 0) + 1);
  }

  reset(contextId: UUID): void {
    this.lastResponseTime.delete(contextId);
    this.responseCount.delete(contextId);
  }
}
```

**File to Modify**: `system/user/server/PersonaUser.ts`

```typescript
// OLD:
private lastResponseTime: Map<UUID, Date> = new Map();
private responseCount: Map<UUID, number> = new Map();
private isRateLimited(roomId: UUID): boolean { /* 20 lines */ }

// NEW:
private rateLimiter = new RateLimiter(10, 50);

// In handleChatMessage():
if (this.rateLimiter.isRateLimited(roomId)) {
  console.log(`⏸️  Rate limited in room ${roomId}`);
  return;
}

// After successful response:
this.rateLimiter.recordResponse(roomId);
```

**Testing**:
```bash
# 1. Compile
npx tsc --noEmit

# 2. Deploy
npm start

# 3. Test AI response
./jtag debug/chat-send --roomId="<ROOM>" --message="Rate limiter test - $(date +%s)"
sleep 15
./jtag debug/logs --filterPattern="AI-RESPONSE|Rate limited" --tailLines=20

# Expected: Same behavior as baseline

# 4. Test rate limiting
for i in {1..5}; do
  ./jtag debug/chat-send --roomId="<ROOM>" --message="Spam test $i"
  sleep 2
done

# Expected: See "Rate limited" logs after 1-2 responses
```

**Commit**: "Extract rate limiting into RateLimiter module - PersonaUser.ts: 2004 → 1950 lines"

**Benefits**:
- Testable in isolation
- Reusable for other user types
- 50 lines removed from PersonaUser

---

### Phase 2: Extract Response Utilities (Week 1-2)

**Goal**: Move response cleaning/validation into separate module

**File to Create**: `system/user/server/modules/ResponseProcessor.ts`

```typescript
export class ResponseProcessor {
  cleanAIResponse(response: string): string {
    // Extract exact logic from PersonaUser.cleanAIResponse()
  }

  async isResponseRedundant(
    response: string,
    roomId: UUID,
    recentMessages: ChatMessageEntity[]
  ): Promise<boolean> {
    // Extract exact logic from PersonaUser.isResponseRedundant()
  }

  extractThinkingSection(response: string): { thinking: string; cleaned: string } {
    // Future: Extract <thinking> tags for analysis
  }
}
```

**File to Modify**: `system/user/server/PersonaUser.ts`

```typescript
// NEW:
private responseProcessor = new ResponseProcessor();

// In respondToMessage():
const cleanedResponse = this.responseProcessor.cleanAIResponse(ragResponse.content);

if (await this.responseProcessor.isResponseRedundant(cleanedResponse, roomId, recentMessages)) {
  console.log('⏭️  Response too redundant, skipping');
  return;
}
```

**Testing**: Same protocol as Phase 1

**Commit**: "Extract response processing into ResponseProcessor module - PersonaUser.ts: 1950 → 1850 lines"

---

### Phase 3: Extract Decision Heuristics (Week 2)

**Goal**: Move decision-making logic into separate, testable module

**File to Create**: `system/user/server/modules/DecisionEngine.ts`

```typescript
export interface DecisionHeuristics {
  mentionScore: number;
  domainRelevanceScore: number;
  recencyScore: number;
  redundancyPenalty: number;
  finalConfidence: number;
}

export class DecisionEngine {
  constructor(private personaName: string) {}

  isPersonaMentioned(messageText: string): boolean {
    // Extract from PersonaUser
  }

  getPersonaDomainKeywords(): string[] {
    // Extract from PersonaUser
  }

  async calculateResponseHeuristics(
    messageEntity: ChatMessageEntity
  ): Promise<DecisionHeuristics> {
    // Extract from PersonaUser
  }
}
```

**File to Modify**: `system/user/server/PersonaUser.ts`

```typescript
// NEW:
private decisionEngine: DecisionEngine;

// In constructor:
this.decisionEngine = new DecisionEngine(this.entity.name);

// In shouldRespondToMessage():
const heuristics = await this.decisionEngine.calculateResponseHeuristics(messageEntity);
```

**Testing**: Same protocol + verify decision logging still works

**Commit**: "Extract decision heuristics into DecisionEngine module - PersonaUser.ts: 1850 → 1700 lines"

---

### Phase 4: Extract RAG Context Management (Week 3)

**Goal**: Separate RAG context storage/retrieval from main PersonaUser logic

**File to Create**: `system/rag/RAGContextManager.ts`

```typescript
export class RAGContextManager {
  async storeContext(roomId: UUID, personaId: UUID, context: PersonaRAGContext): Promise<void> {
    // Extract from PersonaUser.storeRAGContext()
    // Use data/create command
  }

  async loadContext(roomId: UUID, personaId: UUID): Promise<PersonaRAGContext | null> {
    // Extract from PersonaUser.loadRAGContext()
    // Use data/query command
  }

  async updateContext(
    roomId: UUID,
    personaId: UUID,
    message: ChatMessageEntity
  ): Promise<void> {
    // Extract from PersonaUser.updateRAGContext()
  }
}
```

**File to Modify**: `system/user/server/PersonaUser.ts`

```typescript
// NEW:
private ragContextManager = new RAGContextManager();

// Replace:
await this.storeRAGContext(roomId, context);
// With:
await this.ragContextManager.storeContext(roomId, this.id, context);
```

**Testing**: Verify RAG context is still being stored/loaded correctly

**Commit**: "Extract RAG context management into RAGContextManager - PersonaUser.ts: 1700 → 1500 lines"

---

## Command-First Approach

### Extend Existing Commands (Don't Create Parallel Architecture)

**Pattern**: AI needs new capability → Extend existing command OR create NEW command

#### Example: Memory System

**BAD** (Parallel architecture):
```typescript
// Create new MemoryService class
class MemoryService {
  async storeMemory(memory: Memory): Promise<void> {
    // Direct DB access
  }
}

// PersonaUser uses it
await this.memoryService.storeMemory(memory);
```

**GOOD** (Command extension):
```typescript
// Extend data/create command to support 'memories' collection
await this.executeCommand('data/create', {
  collection: 'memories',
  data: {
    personaId: this.id,
    contextId: roomId,
    content: analysis,
    timestamp: Date.now()
  }
});

// Later: Create dedicated memory command if needed
await this.executeCommand('memory/store', {
  contextId: roomId,
  content: analysis,
  tags: ['decision', 'chat']
});
```

#### Example: Genome Management

**Current** (Good!):
```typescript
async getGenome(): Promise<GenomeEntity | null> {
  const result = await this.executeCommand<DataQueryResult<GenomeEntity>>('data/query', {
    collection: COLLECTIONS.GENOMES,
    filter: { id: this.genomeId }
  });
  return result.items?.[0] || null;
}
```

**Keep this pattern** - already using commands!

---

## Phase-by-Phase Breakdown

### Phase 5: Create Thought System (Week 4)

**Goal**: Formalize AI thought broadcasting (already exists informally)

**Current State**:
```typescript
// PersonaUser.ts line 1983:
private async broadcastThought(messageId: string, contextId: UUID, thought: Thought): Promise<void> {
  // Broadcasts thought via event system
}
```

**Action**: Create dedicated `thought/broadcast` command

**New Command**: `commands/thought/broadcast/`

```
commands/thought/broadcast/
├── shared/ThoughtBroadcastTypes.ts
├── server/ThoughtBroadcastServerCommand.ts
└── README.md
```

**PersonaUser Integration**:
```typescript
// OLD:
await this.broadcastThought(messageId, roomId, thought);

// NEW:
await this.executeCommand('thought/broadcast', {
  messageId,
  contextId: roomId,
  thought: {
    personaId: this.id,
    confidence: decision.confidence,
    reasoning: decision.reasoning,
    timestamp: Date.now()
  }
});
```

**Benefits**:
- Thoughts accessible via JTAG (humans can see AI reasoning)
- Future: Store thoughts as memories
- Future: Other AIs can read thoughts for coordination

**Testing**:
```bash
# Verify thoughts are being broadcast
./jtag debug/logs --filterPattern="Thought broadcast" --tailLines=20
```

**Commit**: "Create thought/broadcast command and integrate with PersonaUser"

---

### Phase 6: Command-Based RAG Access (Week 5-6)

**Goal**: Replace direct DB queries in RAGBuilder with JTAG commands

**Current Problem**: ChatRAGBuilder directly queries database

```typescript
// system/rag/builders/ChatRAGBuilder.ts (CURRENT)
const messages = await ChatMessageEntity.query({
  roomId,
  limit: maxMessages
});
```

**Solution**: Use data/query command instead

```typescript
// system/rag/builders/ChatRAGBuilder.ts (NEW)
const result = await executeCommand<DataQueryResult<ChatMessageEntity>>('data/query', {
  collection: COLLECTIONS.CHAT_MESSAGES,
  filter: { roomId },
  orderBy: [{ field: 'timestamp', direction: 'desc' }],
  limit: maxMessages
});
const messages = result.items || [];
```

**Problem**: RAGBuilder doesn't have `executeCommand()` method

**Solution**: Pass command executor to RAGBuilder

```typescript
// system/rag/shared/RAGBuilder.ts (MODIFY)
export abstract class RAGBuilder {
  constructor(protected commandExecutor: CommandExecutor) {}

  async buildContext(contextId: UUID, personaId: UUID, options?: RAGBuildOptions): Promise<RAGContext> {
    // Subclasses use this.commandExecutor.execute()
  }
}

// system/user/server/PersonaUser.ts (MODIFY)
const ragBuilder = RAGBuilderFactory.getBuilder('chat', this);
// Pass 'this' as CommandExecutor (PersonaUser already has executeCommand)
```

**Testing**:
```bash
# Verify RAG context still builds correctly
./jtag debug/chat-send --roomId="<ROOM>" --message="RAG test - $(date +%s)"
sleep 15
./jtag debug/logs --filterPattern="Building RAG context|Worker evaluated" --tailLines=30
```

**Commit**: "Convert RAGBuilder to use commands instead of direct DB access"

**Benefits**:
- AIs use same interface as humans (command-based)
- Future: CommandAccessCoordinator can enforce permissions
- Future: Audit logging of all AI data access

---

### Phase 7: Create ai/evaluate Command (Week 7)

**Goal**: Wrap LLM evaluation logic in a command

**Current State**: `evaluateShouldRespond()` is 200+ lines inside PersonaUser

**Action**: Create `ai/evaluate` command that encapsulates decision logic

**New Command**: `commands/ai/evaluate/`

```
commands/ai/evaluate/
├── shared/AIEvaluateTypes.ts
├── server/AIEvaluateServerCommand.ts
└── README.md
```

**AIEvaluateTypes.ts**:
```typescript
export interface AIEvaluateParams extends CommandParams {
  personaId: UUID;
  messageId: UUID;
  ragContext: RAGContext;
}

export interface AIEvaluateResult extends CommandResult {
  shouldRespond: boolean;
  confidence: number;
  reasoning: string;
  thinkingProcess?: string;  // <thinking> tag content
}
```

**PersonaUser Integration**:
```typescript
// OLD (400 lines):
private async evaluateShouldRespond(ragContext: RAGContext): Promise<Decision> {
  // Build system prompt
  // Call LLM
  // Parse response
  // Calculate confidence
}

// NEW (10 lines):
private async evaluateShouldRespond(ragContext: RAGContext): Promise<Decision> {
  const result = await this.executeCommand<AIEvaluateResult>('ai/evaluate', {
    personaId: this.id,
    messageId: ragContext.triggerMessage.id,
    ragContext
  });

  return {
    shouldRespond: result.shouldRespond,
    confidence: result.confidence,
    reasoning: result.reasoning
  };
}
```

**Benefits**:
- LLM logic testable via command interface
- Can swap LLM providers without touching PersonaUser
- Future: Other systems can evaluate (not just PersonaUser)
- PersonaUser.ts: 1500 → 1100 lines

**Testing**:
```bash
# Test ai/evaluate command directly
./jtag ai/evaluate --personaId="<ID>" --messageId="<MSG_ID>" --ragContext="{...}"

# Verify AI responses still work end-to-end
./jtag debug/chat-send --roomId="<ROOM>" --message="Evaluate command test"
```

**Commit**: "Extract LLM evaluation into ai/evaluate command - PersonaUser.ts: 1500 → 1100 lines"

---

### Phase 8: Create ai/generate Command (Week 8)

**Goal**: Wrap LLM response generation in a command

**Current State**: `respondToMessage()` contains LLM call + response handling

**Action**: Create `ai/generate` command

**New Command**: `commands/ai/generate/`

**AIGenerateTypes.ts**:
```typescript
export interface AIGenerateParams extends CommandParams {
  personaId: UUID;
  messageId: UUID;
  ragContext: RAGContext;
  systemPrompt: string;
}

export interface AIGenerateResult extends CommandResult {
  content: string;
  thinking?: string;  // Extracted <thinking> content
  tokensUsed: number;
  latencyMs: number;
}
```

**PersonaUser Integration**:
```typescript
// OLD (300 lines):
private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
  // Build RAG context
  // Call LLM
  // Clean response
  // Store in DB
  // Emit event
}

// NEW (50 lines):
private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
  // Build RAG context (already modular)
  const ragContext = await ragBuilder.buildContext(roomId, this.id);

  // Generate response via command
  const result = await this.executeCommand<AIGenerateResult>('ai/generate', {
    personaId: this.id,
    messageId: originalMessage.id,
    ragContext,
    systemPrompt: this.entity.systemPrompt
  });

  // Clean response (already modular)
  const cleanedContent = this.responseProcessor.cleanAIResponse(result.content);

  // Store via data/create command
  await this.executeCommand('data/create', {
    collection: COLLECTIONS.CHAT_MESSAGES,
    data: {
      roomId,
      senderId: this.id,
      content: cleanedContent,
      timestamp: Date.now()
    }
  });

  // Record rate limit
  this.rateLimiter.recordResponse(roomId);
}
```

**Benefits**:
- LLM generation fully isolated
- Easy to swap models/providers
- Genomic LoRA loading happens inside command
- PersonaUser.ts: 1100 → 800 lines

**Commit**: "Extract LLM generation into ai/generate command - PersonaUser.ts: 1100 → 800 lines"

---

### Phase 9: Final PersonaUser Structure (Week 9)

**Goal**: PersonaUser becomes a thin orchestrator, all logic in modules/commands

**Target Structure** (800 lines → 400 lines):

```typescript
export class PersonaUser extends AIUser {
  // Dependencies (injected or created)
  private rateLimiter: RateLimiter;
  private responseProcessor: ResponseProcessor;
  private decisionEngine: DecisionEngine;
  private ragContextManager: RAGContextManager;
  private thoughtStream: ThoughtStreamCoordinator;
  private worker: PersonaWorkerThread;

  // Lifecycle
  async initialize(): Promise<void> { /* 30 lines */ }
  async shutdown(): Promise<void> { /* 20 lines */ }

  // Main handler (orchestration only)
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // 1. Check rate limit
    if (this.rateLimiter.isRateLimited(roomId)) return;

    // 2. Build RAG context (via RAGBuilder using commands)
    const ragContext = await this.buildRAGContext(roomId);

    // 3. Evaluate if should respond (via ai/evaluate command)
    const decision = await this.evaluate(ragContext);
    if (!decision.shouldRespond) return;

    // 4. Request turn (via ThoughtStream)
    const permission = await this.thoughtStream.requestTurn(roomId, this.id, decision.confidence);
    if (!permission.granted) return;

    // 5. Generate response (via ai/generate command)
    await this.generateAndSendResponse(ragContext);

    // 6. Record rate limit
    this.rateLimiter.recordResponse(roomId);
  }

  // Helper methods (thin wrappers around commands/modules)
  private async buildRAGContext(roomId: UUID): Promise<RAGContext> { /* 10 lines */ }
  private async evaluate(context: RAGContext): Promise<Decision> { /* 10 lines */ }
  private async generateAndSendResponse(context: RAGContext): Promise<void> { /* 20 lines */ }

  // Genome management (already uses commands)
  async getGenome(): Promise<GenomeEntity | null> { /* 15 lines */ }
  async setGenome(genomeId: UUID): Promise<boolean> { /* 20 lines */ }
}
```

**Result**: PersonaUser.ts goes from 2004 lines → ~400 lines

---

## Testing Protocol

### After Every Phase

```bash
# 1. Type check
npx tsc --noEmit

# 2. Build and deploy
npm start

# 3. Wait for deployment (90+ seconds)
sleep 95

# 4. Ping check
./jtag ping
# Expected: systemReady: true, 64+ commands, 12+ daemons

# 5. Send test message
./jtag debug/chat-send --roomId="<ROOM_ID>" --message="Phase N test - $(date +%s)"

# 6. Wait for AI evaluation/response
sleep 15

# 7. Check AI activity
./jtag debug/logs --filterPattern="Worker evaluated|AI-RESPONSE|POSTED" --tailLines=30

# Expected output:
# - Multiple "Worker evaluated" messages (5-10 AIs)
# - At least one "AI-RESPONSE" with confidence score
# - At least one "POSTED" message to room

# 8. Take screenshot
./jtag screenshot --querySelector="chat-widget" --filename="phase-N-test.png"

# 9. Run integration tests
npm test -- --grep="AI response"

# IF ANY STEP FAILS: Revert commit and fix before continuing
```

### Baseline Metrics (Establish Before Phase 1)

Run this BEFORE starting refactoring:

```bash
# Test 1: Single AI response
./jtag debug/chat-send --roomId="<ROOM>" --message="Baseline A - $(date +%s)"
sleep 15
EVALS_A=$(./jtag debug/logs --filterPattern="Worker evaluated" --tailLines=50 | grep -c "Worker evaluated")
RESPONSES_A=$(./jtag debug/logs --filterPattern="AI-RESPONSE" --tailLines=50 | grep -c "AI-RESPONSE")

echo "Test A: $EVALS_A evaluations, $RESPONSES_A responses"

# Test 2: Multiple AIs respond
./jtag debug/chat-send --roomId="<ROOM>" --message="@everyone Baseline B - what do you think?"
sleep 20
EVALS_B=$(./jtag debug/logs --filterPattern="Worker evaluated" --tailLines=50 | grep -c "Worker evaluated")
RESPONSES_B=$(./jtag debug/logs --filterPattern="AI-RESPONSE" --tailLines=50 | grep -c "AI-RESPONSE")

echo "Test B: $EVALS_B evaluations, $RESPONSES_B responses"

# Save baseline
echo "BASELINE METRICS ($(date))" > docs/refactoring-baseline.txt
echo "Test A: $EVALS_A evaluations, $RESPONSES_A responses" >> docs/refactoring-baseline.txt
echo "Test B: $EVALS_B evaluations, $RESPONSES_B responses" >> docs/refactoring-baseline.txt
```

**Expected Baseline** (based on recent fixes):
- Test A: 5-10 evaluations, 1-2 responses
- Test B: 8-12 evaluations, 2-4 responses

**After Each Phase**: Run same tests, compare to baseline. If numbers drop significantly, investigate before continuing.

---

## Success Criteria

### Per-Phase Success

- ✅ **Compilation**: Zero TypeScript errors
- ✅ **Deployment**: `npm start` succeeds, system ready
- ✅ **AI Responses**: Same or better than baseline
- ✅ **Tests Pass**: All existing tests green
- ✅ **Line Count**: PersonaUser.ts steadily decreasing

### Final Success (After Phase 9)

- ✅ **PersonaUser.ts**: 2004 lines → ~400 lines (80% reduction)
- ✅ **Modularity**: 5+ reusable modules extracted
- ✅ **Command-Based**: All AI operations via JTAG commands
- ✅ **Testability**: Each module testable in isolation
- ✅ **AI Responses**: Same or better quality/frequency
- ✅ **No Regressions**: All baseline tests passing

---

## Related Documents

- [UNIVERSAL-COGNITION-ARCHITECTURE.md](./UNIVERSAL-COGNITION-ARCHITECTURE.md) - Long-term vision
- [ORGANIC-COGNITION-ARCHITECTURE.md](./ORGANIC-COGNITION-ARCHITECTURE.md) - Migration strategy
- [CONSOLIDATION-PLAN.md](./CONSOLIDATION-PLAN.md) - Documentation reorganization

---

**This plan prioritizes WORKING SYSTEM over architectural purity. Every commit must ship.**
