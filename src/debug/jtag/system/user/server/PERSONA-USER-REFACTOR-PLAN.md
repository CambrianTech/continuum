# PersonaUser Refactoring Plan (REVISED)

## Problem Statement

**PersonaUser.ts** is **2,622 lines** - the largest file in the codebase, exceeding the 500-line architectural limit by 5x.

## CRITICAL CONTEXT: Existing CNS Architecture

**IMPORTANT**: PersonaUser already has a Central Nervous System (CNS) orchestration layer!

```
CNS (PersonaCentralNervousSystem.ts - 134 lines)
  ├── serviceCycle()                    # Main orchestration loop
  │   ├── pollTasks()                   → calls PersonaUser.pollTasksFromCNS()
  │   ├── generateSelfTasks()           → calls PersonaUser.generateSelfTasksFromCNS()
  │   ├── waitForWork()                 # Signal-based waiting
  │   ├── buildCognitiveContext()       # Energy, mood, queue state
  │   └── serviceChatDomain()           → calls PersonaUser.handleChatMessageFromCNS()
  │
PersonaUser.ts (2,622 lines - TOO LARGE)
  ├── pollTasksFromCNS()                # CNS callback (delegates to pollTasks)
  ├── generateSelfTasksFromCNS()        # CNS callback (delegates to task generator)
  └── handleChatMessageFromCNS()        # CNS callback (delegates to evaluateAndPossiblyRespond)
      └── evaluateAndPossiblyRespond()  # ~900 lines of chat logic
          └── respondToMessage()        # ~400 lines of AI response generation
```

**The Pattern**: CNS orchestrates, PersonaUser provides domain logic via callbacks.

**What "thoughts" and "data needs" mean**:
- **"thoughts"**: ChatCoordinationStream / ThoughtStreamCoordinator (line 32) - coordinates which AI responds
- **"data needs"**: CNS callbacks need database access (DataDaemon.store, DataDaemon.update) for tasks, genome activation, message evaluation

## Current Structure Analysis

**PersonaUser.ts** contains three distinct functional areas that need extraction:

### 1. Chat Response Logic (~900 lines, 34% of file)
Lines 416-1324 contain all chat message handling:
- **Entry**: `handleChatMessage()` (line 416) - enqueues message to inbox
- **CNS Callback**: `handleChatMessageFromCNS()` (line 2317) - called by CNS orchestrator
- **Evaluation**: `evaluateAndPossiblyRespond()` (line 477) - decides if should respond
- **Generation**: `respondToMessage()` (line 875) - generates AI responses
- **Decision**: `shouldRespondToMessage()` (line 1374) - heuristic evaluation
- **Scoring**: `calculateResponseHeuristics()` (line 1469) - scoring system
- **Helpers**: `isPersonaMentioned()`, `cleanAIResponse()`, `isResponseRedundant()`

### 2. RAG Context Management (~200 lines, 8% of file)
Lines 1581-1798 contain genome and RAG operations:
- `getGenome()` - retrieve current genome
- `setGenome()` - switch active genome
- `storeRAGContext()` - persist context per room
- `loadRAGContext()` - retrieve context for room
- `updateRAGContext()` - update with new message

### 3. Task Execution (~500 lines, 19% of file)
Lines 2403-2596 contain task execution handlers:
- `executeTask()` (line 2403) - task dispatcher
- `executeMemoryConsolidation()` - task type handler
- `executeSkillAudit()` - task type handler
- `executeResumeWork()` - task type handler
- `executeFineTuneLora()` - task type handler

### 4. Core Orchestration (~1,000 lines, 39% of file)
Remaining logic:
- CNS callbacks (lines 2278-2385)
- Initialization and module setup
- Event subscriptions
- Room updates
- Training readiness
- Shutdown

## Refactoring Strategy: Work WITH CNS, Not Around It

### ❌ WRONG APPROACH (original plan):
Create standalone coordinators that bypass CNS:
```typescript
// DON'T DO THIS:
class ChatResponseCoordinator {
  // Independent coordinator that duplicates CNS orchestration
}
```

### ✅ RIGHT APPROACH (revised):
Extract modules that PersonaUser delegates to in CNS callbacks:
```typescript
// PersonaUser remains the CNS callback provider:
public async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
  // Thin delegation to focused module
  await this.chatHandler.handle(item);
}
```

## Proposed Extraction Plan

### Extract 1: ChatMessageHandler (~900 lines)

**New file**: `system/user/server/modules/ChatMessageHandler.ts`

```typescript
/**
 * ChatMessageHandler - Handles chat message evaluation and response generation
 *
 * Called by PersonaUser.handleChatMessageFromCNS() via delegation pattern.
 * Does NOT orchestrate - that's CNS's job. Just provides domain logic.
 */
export class ChatMessageHandler {
  constructor(
    private personaId: UUID,
    private displayName: string,
    private rateLimiter: RateLimiter,
    private modelConfig: ModelConfig,
    private genome: PersonaGenome,
    private ragHandler: RAGContextHandler,
    private client?: JTAGClient
  ) {}

  /**
   * Handle a chat message (called from PersonaUser.handleChatMessageFromCNS)
   */
  async handle(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    messageText: string
  ): Promise<void> {
    await this.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, messageText);
  }

  // Private methods extracted from PersonaUser:
  private async evaluateAndPossiblyRespond(...)
  private async respondToMessage(...)
  private async shouldRespondToMessage(...)
  private async calculateResponseHeuristics(...)
  private async evaluateShouldRespond(...)
  private async isResponseRedundant(...)
  private cleanAIResponse(...)
  private isPersonaMentioned(...)
  private getPersonaDomainKeywords(...)
}
```

**PersonaUser change**:
```typescript
// Before (2,622 lines total, ~900 lines of chat logic inline):
public async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
  // ... task status update ...
  // ... genome activation ...

  if (item.type === 'message') {
    const reconstructedEntity: any = { /* ... */ };
    const senderIsHuman = !item.senderId.startsWith('persona-');
    const messageText = item.content;

    // 900 lines of inline chat logic:
    await this.evaluateAndPossiblyRespond(reconstructedEntity, senderIsHuman, messageText);
  } else if (item.type === 'task') {
    await this.executeTask(item);
  }

  // ... state updates ...
}

// After (PersonaUser reduced to ~1,700 lines):
private chatHandler: ChatMessageHandler;

public async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
  // Task status update
  if (item.type === 'task') {
    await DataDaemon.update<TaskEntity>(
      COLLECTIONS.TASKS,
      item.taskId,
      { status: 'in_progress', startedAt: new Date() }
    );
  }

  // Genome activation
  if (item.domain) {
    const adapterName = this.domainToAdapter[item.domain] || 'conversational';
    await this.genome.activateSkill(adapterName);
  }

  // Delegate to domain handlers
  if (item.type === 'message') {
    const reconstructedEntity = this.reconstructMessageEntity(item);
    const senderIsHuman = !item.senderId.startsWith('persona-');
    await this.chatHandler.handle(reconstructedEntity, senderIsHuman, item.content);
  } else if (item.type === 'task') {
    await this.taskHandler.execute(item);
  }

  // State updates
  this.personaState.updateInboxLoad(this.inbox.getSize());
  this.adjustCadence();
}
```

### Extract 2: RAGContextHandler (~200 lines)

**New file**: `system/user/server/modules/RAGContextHandler.ts`

```typescript
/**
 * RAGContextHandler - Manages RAG context storage and retrieval
 *
 * Used by ChatMessageHandler and other components that need conversation context.
 */
export class RAGContextHandler {
  constructor(
    private personaId: UUID,
    private genome: PersonaGenome
  ) {}

  async getGenome(): Promise<GenomeEntity | null>
  async setGenome(genomeId: UUID): Promise<boolean>
  async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void>
  async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null>
  async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void>
}
```

**PersonaUser change**:
```typescript
// Before:
async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
  // 200 lines of RAG logic...
}

// After:
private ragHandler: RAGContextHandler;

async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
  return this.ragHandler.loadRAGContext(roomId);
}
```

### Extract 3: TaskExecutionHandler (~500 lines)

**New file**: `system/user/server/modules/TaskExecutionHandler.ts`

```typescript
/**
 * TaskExecutionHandler - Executes different task types
 *
 * Called by PersonaUser.handleChatMessageFromCNS() when item.type === 'task'.
 */
export class TaskExecutionHandler {
  constructor(
    private personaId: UUID,
    private displayName: string,
    private genome: PersonaGenome,
    private ragHandler: RAGContextHandler,
    private trainingAccumulator: TrainingDataAccumulator
  ) {}

  /**
   * Execute a task based on its type
   */
  async execute(task: InboxTask): Promise<void> {
    console.log(`🎯 ${this.displayName}: Executing task: ${task.taskType}`);

    const startTime = Date.now();
    let outcome = '';
    let status: TaskStatus = 'completed';

    try {
      switch (task.taskType) {
        case 'memory-consolidation':
          outcome = await this.executeMemoryConsolidation(task);
          break;
        case 'skill-audit':
          outcome = await this.executeSkillAudit(task);
          break;
        case 'resume-work':
          outcome = await this.executeResumeWork(task);
          break;
        case 'fine-tune-lora':
          outcome = await this.executeFineTuneLora(task);
          break;
        default:
          outcome = `Unknown task type: ${task.taskType}`;
          status = 'failed';
      }

      // Update task in database
      await DataDaemon.update<TaskEntity>(
        COLLECTIONS.TASKS,
        task.taskId,
        { status, outcome, completedAt: new Date() }
      );
    } catch (error) {
      console.error(`❌ ${this.displayName}: Task failed: ${error}`);
      await DataDaemon.update<TaskEntity>(
        COLLECTIONS.TASKS,
        task.taskId,
        { status: 'failed', outcome: String(error) }
      );
    }
  }

  // Task type handlers (extracted from PersonaUser):
  private async executeMemoryConsolidation(task: InboxTask): Promise<string>
  private async executeSkillAudit(task: InboxTask): Promise<string>
  private async executeResumeWork(task: InboxTask): Promise<string>
  private async executeFineTuneLora(task: InboxTask): Promise<string>
}
```

**PersonaUser change**:
```typescript
// Before:
private async executeTask(task: InboxTask): Promise<void> {
  // 500 lines of task execution logic...
}

// After:
private taskHandler: TaskExecutionHandler;

// Method removed - handleChatMessageFromCNS calls taskHandler.execute directly
```

## Expected Results

### Code Reduction

**Before**:
- PersonaUser.ts: 2,622 lines

**After**:
- PersonaUser.ts: ~1,000 lines (core orchestration + CNS callbacks)
- ChatMessageHandler.ts: ~900 lines (new)
- RAGContextHandler.ts: ~200 lines (new)
- TaskExecutionHandler.ts: ~500 lines (new)

**Net change**: +578 lines across 4 files (but PersonaUser reduced by 1,622 lines)

### Architecture Benefits

1. **PersonaUser under 1,500 lines** - Manageable size, focuses on CNS callbacks
2. **CNS untouched** - Existing orchestration layer preserved
3. **Clear delegation pattern** - PersonaUser → Handlers via CNS callbacks
4. **Testable modules** - Each handler testable independently
5. **Reduced cognitive load** - Each file focused on one domain
6. **Data access preserved** - Handlers can use DataDaemon, Events, Commands

### Dependency Graph

```
CNS (orchestration)
  ↓ serviceCycle()
PersonaUser (thin coordinator, ~1000 lines)
  ├── chatHandler: ChatMessageHandler
  │   ├── uses: rateLimiter, modelConfig, genome
  │   └── uses: ragHandler (for context)
  ├── ragHandler: RAGContextHandler
  │   └── uses: genome
  └── taskHandler: TaskExecutionHandler
      ├── uses: genome, trainingAccumulator
      └── uses: ragHandler (for memory consolidation)
```

## Implementation Stages

### Stage 1: Extract RAGContextHandler (1 hour)
**Why first**: Smallest extraction, used by other handlers

1. Create `RAGContextHandler.ts` with all RAG methods
2. Update PersonaUser to use handler
3. Test RAG context operations
4. **Checkpoint**: `./jtag data/list --collection=users` works
5. Commit: "refactor: extract RAGContextHandler from PersonaUser"

### Stage 2: Extract ChatMessageHandler (2-3 hours)
**Why second**: Largest extraction, uses RAGContextHandler

1. Create `ChatMessageHandler.ts` with all chat methods
2. Pass ragHandler to constructor
3. Update PersonaUser.handleChatMessageFromCNS to delegate
4. Test chat functionality end-to-end
5. **Checkpoint**: Send message via `./jtag debug/chat-send`, verify AI responds
6. Commit: "refactor: extract ChatMessageHandler from PersonaUser"

### Stage 3: Extract TaskExecutionHandler (2-3 hours)
**Why third**: Uses RAGContextHandler, independent of chat

1. Create `TaskExecutionHandler.ts` with all task execution methods
2. Pass ragHandler to constructor
3. Update PersonaUser.handleChatMessageFromCNS to delegate
4. Test task execution (if tasks exist in database)
5. **Checkpoint**: System startup completes, autonomous loop runs
6. Commit: "refactor: extract TaskExecutionHandler from PersonaUser"

### Stage 4: Integration Testing (1 hour)
1. Test full PersonaUser lifecycle (init → chat → tasks → shutdown)
2. Verify CNS orchestration still works correctly
3. Check no regressions in autonomous behavior
4. Take screenshot to verify browser UI
5. Commit: "test: verify PersonaUser refactoring integration"

**Total estimated time**: 6-8 hours

## Testing Strategy

### Unit Tests (Optional - Focus on Integration)
```typescript
describe('ChatMessageHandler', () => {
  it('should handle @mentions correctly')
  it('should evaluate response heuristics')
  it('should detect redundant responses')
})

describe('RAGContextHandler', () => {
  it('should store and retrieve RAG context per room')
  it('should update context with new messages')
})

describe('TaskExecutionHandler', () => {
  it('should execute memory consolidation tasks')
  it('should handle task errors gracefully')
})
```

### Integration Tests (CRITICAL)
```bash
# System-level tests
npm start

# Test chat response (most important!)
./jtag debug/chat-send --message="@test-persona hello"
# Verify response appears in chat widget

# Test data operations
./jtag data/list --collection=users
# Should return users successfully

# Take screenshot
./jtag screenshot --querySelector="chat-widget"
# Verify UI shows messages correctly
```

## Rollback Criteria

**Only revert if**:
1. Chat responses broken (personas don't respond to messages)
2. CNS orchestration broken (serviceInbox doesn't run)
3. RAG context not persisting between rooms
4. TypeScript compilation fails and can't be fixed in 30 minutes
5. System startup fails and logs show architectural issue

**Don't revert for**:
- Minor TypeScript compilation errors (fix them)
- Failing unit tests (fix the tests or code)
- Import path issues (use @system path aliases)

## Success Criteria

- ✅ PersonaUser.ts reduced to <1,500 lines
- ✅ Three new handlers created with focused responsibilities
- ✅ All existing functionality preserved (chat, RAG, tasks)
- ✅ CNS orchestration untouched and working
- ✅ TypeScript compilation succeeds
- ✅ `npm start` completes successfully
- ✅ `./jtag ping` shows systemReady: true
- ✅ `./jtag data/list --collection=users` returns data
- ✅ Chat messages trigger AI responses correctly
- ✅ Screenshot shows working UI

## Key Differences from Original Plan

### What Changed:
1. **Renamed modules** from "Coordinators" to "Handlers" (better reflects delegation pattern)
2. **CNS preserved** - No changes to PersonaCentralNervousSystem.ts
3. **Callback pattern maintained** - PersonaUser still provides CNS callbacks
4. **Focused on delegation** - PersonaUser delegates to handlers, doesn't create parallel coordinators
5. **Data access clarified** - Handlers can use DataDaemon, Events, Commands directly

### Why This Approach is Better:
- Works WITH existing CNS architecture, not against it
- Maintains clear orchestration hierarchy: CNS → PersonaUser → Handlers
- Preserves "thoughts" (ChatCoordinationStream) and "data needs" (DataDaemon access)
- Reduces PersonaUser size without breaking existing patterns
- Easier to test (handlers are independent but called via PersonaUser)

## Related Documentation

- `PersonaCentralNervousSystem.ts` - CNS orchestration layer (DON'T TOUCH)
- `CNSFactory.ts` - Creates CNS based on capabilities
- `SQL-ADAPTER-DEBUGGING-RECOVERY-PLAN.md` - Debugging methodology (same approach applies)
- `CLAUDE.md` - "AGGRESSIVE REFACTORING PRINCIPLE" section
