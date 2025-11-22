# PersonaUser Refactoring Plan
**Breaking Down the 2964-Line Beast**

**Date**: 2025-11-17
**Status**: Analysis Phase
**Goal**: Extract PersonaUser cognition logic into clean, testable adapters and modules

---

## Current State Analysis

**File**: `system/user/server/PersonaUser.ts`
**Lines**: 2964
**Methods**: 31
**Dependencies**: 94 imports (!)

### The Problem

PersonaUser has become an unmaintainable monolith containing:
- Chat message handling
- Decision-making logic (fast-path, LLM, thermal)
- Response generation
- RAG context building
- Coordination and redundancy checks
- Task execution
- Self-task generation
- Autonomous servicing loop
- Training data accumulation
- Cognition state logging
- Tool execution

**Result**: Nearly impossible to read, test, or extend. Cognition logic alone spans hundreds of lines across multiple methods.

---

## Current Module Landscape

### ✅ Already Extracted (Good Progress!)

These modules exist and are working:

**Memory & State**:
- `PersonaMemory.ts` - RAG context, genome, knowledge (175 lines)
- `WorkingMemoryManager.ts` - Working memory management
- `PersonaSelfState.ts` - Self-awareness state tracking
- `InMemoryCognitionStorage.ts` - Storage backend

**Decision Making**:
- `DecisionAdapterChain.ts` - Chain of decision adapters
- `FastPathAdapter.ts` - Fast decisions without LLM
- `LLMAdapter.ts` - LLM-powered decisions
- `ThermalAdapter.ts` - Load-based throttling
- `ProposalRatingAdapter.ts` - Rating proposals

**Reasoning**:
- `SimplePlanFormulator.ts` - Basic plan formulation
- `reasoning/types.ts` - Task, Plan, Evaluation types

**Coordination**:
- `PersonaCentralNervousSystem.ts` - Orchestration
- `CNSFactory.ts` - CNS factory
- `DeterministicCognitiveScheduler.ts` - Tier 1 scheduler
- `HeuristicCognitiveScheduler.ts` - Tier 2 scheduler

**Other**:
- `PersonaInbox.ts` - Priority queue for tasks/messages
- `PersonaState.ts` - Energy/mood/cadence tracking
- `PersonaGenome.ts` - LoRA adapter paging
- `PersonaToolExecutor.ts` - Tool execution
- `SelfTaskGenerator.ts` - Autonomous task creation
- `TrainingDataAccumulator.ts` - Training data collection
- `RateLimiter.ts` - Rate limiting
- `CognitionLogger.ts` - Observability logging

---

## PersonaUser Method Categorization

### 1. **Lifecycle Management** (Keep in PersonaUser)
- Constructor, initialize(), shutdown()
- autoJoinGeneralRoom()
- startAutonomousServicing()
- runServiceLoop()
- checkTrainingReadiness()

**Why**: Core orchestration, not domain-specific logic

---

### 2. **Event Handling** (Keep in PersonaUser, but delegate)
- handleChatMessage(messageEntity)
- handleRoomUpdate(roomEntity)
- handleChatMessageFromCNS(item)

**Why**: Entry points that route to adapters. PersonaUser = event dispatcher.

**Pattern**:
```typescript
private async handleChatMessage(msg: ChatMessageEntity): Promise<void> {
  // PersonaUser receives event
  // Delegates to ChatResponseAdapter
  await this.chatResponseAdapter.handleMessage(msg);
}
```

---

### 3. **Chat Response Logic** (Extract to `ChatResponseAdapter`)

**Current Methods** (lines 437-1651):
- `evaluateAndPossiblyRespondWithCognition()`
- `evaluateAndPossiblyRespond()`
- `respondToMessage()`
- `shouldRespondToMessage()`
- `evaluateShouldRespond()`
- `isPersonaMentioned()`
- `calculateResponseHeuristics()`
- `isSenderHuman()`

**Proposed Module**: `adapters/chat/ChatResponseAdapter.ts`

**Responsibilities**:
- Receive chat messages
- Evaluate if should respond (using DecisionAdapterChain)
- Build RAG context (using PersonaMemory)
- Generate response (using AIProviderDaemon)
- Post response (using Commands)
- Log decisions (using CognitionLogger)

**Target**: ~400-500 lines

**Interface**:
```typescript
export class ChatResponseAdapter {
  constructor(
    private persona: PersonaUser,
    private memory: PersonaMemory,
    private decisionChain: DecisionAdapterChain,
    private workingMemory: WorkingMemoryManager,
    private selfState: PersonaSelfState,
    private planFormulator: SimplePlanFormulator
  ) {}

  async handleMessage(msg: ChatMessageEntity): Promise<void> {
    // All chat response logic here
  }
}
```

---

### 4. **RAG Context Building** (Extract to `ChatRAGContextBuilder`)

**Current Methods**:
- `buildCoordinationRAGContext()`
- Various RAG queries scattered across response methods

**Proposed Module**: `adapters/chat/ChatRAGContextBuilder.ts`

**Responsibilities**:
- Load conversation history
- Query RAG for relevant context
- Build coordination context for decision-making
- Format context for LLM prompts

**Target**: ~200 lines

---

### 5. **Response Processing** (Extract to `ResponseProcessor`)

**Current Methods**:
- `cleanAIResponse()`
- `isResponseRedundant()`
- `timestampToNumber()`

**Proposed Module**: `adapters/chat/ResponseProcessor.ts`

**Responsibilities**:
- Clean LLM responses (remove artifacts, formatting)
- Check redundancy with other personas
- Validate responses before posting

**Target**: ~150 lines

---

### 6. **Task Execution** (Extract to `TaskExecutionAdapter`)

**Current Methods** (lines 2735-2920):
- `serviceInbox()`
- `executeTask()`
- `executeMemoryConsolidation()`
- `executeSkillAudit()`
- `executeResumeWork()`
- `executeFineTuneLora()`
- `pollTasks()`
- `pollTasksFromCNS()`
- `generateSelfTasksFromCNS()`

**Proposed Module**: `adapters/task/TaskExecutionAdapter.ts`

**Responsibilities**:
- Poll tasks from inbox
- Route tasks to appropriate handlers
- Execute domain-specific tasks (memory, skill, training)
- Report task outcomes
- Update persona state after task completion

**Target**: ~300 lines

**Interface**:
```typescript
export class TaskExecutionAdapter {
  constructor(
    private persona: PersonaUser,
    private inbox: PersonaInbox,
    private taskHandlers: Map<TaskType, TaskHandler>
  ) {}

  async pollAndExecute(): Promise<void> {
    const task = await this.inbox.peek(1);
    if (!task) return;

    const handler = this.taskHandlers.get(task.type);
    await handler.execute(task);
  }
}
```

---

### 7. **Decision Logging** (Already Extracted!)

**Current Methods**:
- `logAIDecision()` - Thin wrapper around AIDecisionLogger

**Status**: ✅ Already using `AIDecisionLogger` and `CognitionLogger`

**Action**: Keep as-is, just ensure all adapters use these loggers

---

### 8. **Utility Methods** (Extract to `PersonaUtils`)

**Current Methods**:
- `getPersonaDomainKeywords()`
- `adjustCadence()`

**Proposed Module**: `utils/PersonaUtils.ts`

**Target**: ~100 lines

---

## Proposed Architecture

```
PersonaUser (coordinator - 400 lines max)
├── Lifecycle management (init, shutdown, loops)
├── Event subscription/routing
├── Module composition and dependency injection
└── Delegates to adapters:
    │
    ├── ChatResponseAdapter (chat logic - 400 lines)
    │   ├── Uses: DecisionAdapterChain
    │   ├── Uses: PersonaMemory
    │   ├── Uses: ChatRAGContextBuilder
    │   ├── Uses: ResponseProcessor
    │   └── Uses: CognitionLogger
    │
    ├── TaskExecutionAdapter (task logic - 300 lines)
    │   ├── Uses: PersonaInbox
    │   ├── Uses: SelfTaskGenerator
    │   ├── Uses: TaskHandlers
    │   └── Uses: PersonaState
    │
    ├── ChatRAGContextBuilder (RAG building - 200 lines)
    │   └── Uses: PersonaMemory
    │
    └── ResponseProcessor (response processing - 150 lines)
        └── Uses: CoordinationDecisionLogger
```

---

## Migration Strategy

### Phase 1: Extract Chat Response Logic ⚠️ HIGHEST IMPACT

**Why first**: This is the biggest chunk (1000+ lines) and the most complex

**Steps**:
1. Create `adapters/chat/ChatResponseAdapter.ts`
2. Move methods:
   - `evaluateAndPossiblyRespondWithCognition()`
   - `evaluateAndPossiblyRespond()`
   - `respondToMessage()`
   - `shouldRespondToMessage()`
   - `evaluateShouldRespond()`
   - Helper methods (isPersonaMentioned, isSenderHuman, etc.)
3. Create constructor that receives PersonaUser dependencies
4. Update PersonaUser.handleChatMessage() to delegate:
   ```typescript
   private async handleChatMessage(msg: ChatMessageEntity): Promise<void> {
     await this.chatResponseAdapter.handleMessage(msg);
   }
   ```
5. Run tests to ensure no regressions

**Expected Result**: PersonaUser drops from 2964 → ~1900 lines

---

### Phase 2: Extract RAG Context Building

**Steps**:
1. Create `adapters/chat/ChatRAGContextBuilder.ts`
2. Move RAG-related logic from chat response methods
3. ChatResponseAdapter uses this builder
4. Test RAG queries still work

**Expected Result**: ChatResponseAdapter drops from 500 → ~400 lines

---

### Phase 3: Extract Task Execution

**Steps**:
1. Create `adapters/task/TaskExecutionAdapter.ts`
2. Move task-related methods
3. Create TaskHandler interface and implementations
4. Update PersonaUser to delegate task execution

**Expected Result**: PersonaUser drops from ~1900 → ~1400 lines

---

### Phase 4: Extract Response Processing

**Steps**:
1. Create `adapters/chat/ResponseProcessor.ts`
2. Move response cleaning and redundancy checking
3. ChatResponseAdapter uses this processor

**Expected Result**: ChatResponseAdapter drops from 400 → ~300 lines

---

### Phase 5: Cleanup and Optimization

**Steps**:
1. Extract remaining utilities
2. Review adapter boundaries
3. Add comprehensive tests for each adapter
4. Update documentation

**Expected Result**: PersonaUser becomes a clean ~400-line coordinator

---

## Final Target Structure

```
system/user/server/
├── PersonaUser.ts (400 lines - coordinator only)
│
├── adapters/
│   ├── chat/
│   │   ├── ChatResponseAdapter.ts (300 lines)
│   │   ├── ChatRAGContextBuilder.ts (200 lines)
│   │   └── ResponseProcessor.ts (150 lines)
│   │
│   └── task/
│       ├── TaskExecutionAdapter.ts (250 lines)
│       └── handlers/
│           ├── MemoryConsolidationHandler.ts (100 lines)
│           ├── SkillAuditHandler.ts (100 lines)
│           └── FineTuneHandler.ts (150 lines)
│
└── modules/ (existing)
    ├── cognition/
    │   ├── DecisionAdapterChain.ts ✅
    │   ├── adapters/ ✅
    │   ├── memory/ ✅
    │   └── reasoning/ ✅
    ├── central-nervous-system/ ✅
    ├── PersonaInbox.ts ✅
    ├── PersonaState.ts ✅
    ├── PersonaMemory.ts ✅
    └── ... (other modules) ✅
```

---

## Testing Strategy

### Baseline Tests (Before Refactoring)

```bash
# Run existing tests to establish baseline
npm test -- PersonaUser
npx vitest tests/integration/persona-lifecycle.test.ts
npx vitest tests/integration/chat-response.test.ts
```

**Critical**: All tests must pass BEFORE and AFTER each phase.

### Adapter-Level Tests (After Each Phase)

```typescript
// Example: ChatResponseAdapter tests
describe('ChatResponseAdapter', () => {
  it('should evaluate and respond to chat messages', async () => {
    const adapter = new ChatResponseAdapter(/* deps */);
    await adapter.handleMessage(mockMessage);
    expect(mockAIProvider.generate).toHaveBeenCalled();
  });

  it('should not respond when decision chain says SILENT', async () => {
    // ... test decision logic
  });

  it('should build RAG context correctly', async () => {
    // ... test RAG integration
  });
});
```

### Integration Tests

```bash
# After all phases, test full system
npm start
./jtag debug/chat-send --room="general" --message="Test refactored persona"
# Should see responses from PersonaUsers
```

---

## Key Principles

### 1. **Single Responsibility**
Each adapter owns ONE domain:
- ChatResponseAdapter = chat responses
- TaskExecutionAdapter = task execution
- RAGContextBuilder = RAG queries

### 2. **Dependency Injection**
Adapters receive dependencies via constructor:
```typescript
new ChatResponseAdapter(memory, decisionChain, selfState, planFormulator)
```

Not:
```typescript
// BAD - adapter creates its own dependencies
class ChatResponseAdapter {
  private memory = new PersonaMemory(...);  // ❌
}
```

### 3. **Preserve Existing Modules**
Don't rewrite `DecisionAdapterChain`, `PersonaMemory`, etc. They work.
Just extract PersonaUser's inline logic into adapters that USE these modules.

### 4. **Test at Every Step**
After each extraction phase:
- Run all tests
- Manually test with `./jtag chat/send`
- Check AI decisions log
- Verify no regressions

### 5. **Keep PersonaUser as Coordinator**
PersonaUser's final role:
- Initialize all adapters
- Subscribe to events
- Dispatch events to appropriate adapters
- Manage lifecycle (startup/shutdown)
- Maintain autonomous servicing loop

It should NOT contain business logic.

---

## Benefits of This Refactor

### Readability
- PersonaUser: 2964 lines → 400 lines (7x smaller!)
- Each adapter: 150-300 lines (readable in one screen)
- Clear separation of concerns

### Testability
- Test chat logic without task logic
- Mock adapters independently
- Faster test execution

### Extensibility
- Add new adapters without touching PersonaUser core
- Example: `CodeReviewAdapter`, `GameplayAdapter`
- Swap implementations (e.g., different RAG strategies)

### Maintainability
- Bug in chat logic? Look in ChatResponseAdapter only
- Want to change decision-making? Look in DecisionAdapterChain
- No more hunting through 3000 lines

### Performance
- Adapters can be optimized independently
- Easier to parallelize (run adapters concurrently)
- Better for future multi-threading

---

## Risk Mitigation

### Risk 1: Breaking Existing Functionality

**Mitigation**:
- Comprehensive baseline tests BEFORE starting
- Test after each phase
- Keep git commits small and atomic
- Easy rollback if issues arise

### Risk 2: Performance Regression

**Mitigation**:
- Profile before/after each phase
- Measure inference latency
- Check memory usage
- Monitor AI decisions log

### Risk 3: Integration Issues

**Mitigation**:
- Keep PersonaUser public API unchanged
- Adapters are internal implementation details
- External callers (CNS, etc.) don't need to change

### Risk 4: Incomplete Understanding

**Mitigation**:
- Read each method carefully before moving
- Preserve comments and context
- Ask clarifying questions
- Document assumptions

---

## Implementation Checklist

### Pre-Work
- [ ] Run all existing tests, document baseline
- [ ] Profile current performance metrics
- [ ] Create feature branch: `refactor/persona-user-adapters`
- [ ] Set up automated testing in CI

### Phase 1: Chat Response Adapter
- [ ] Create `adapters/chat/` directory
- [ ] Create `ChatResponseAdapter.ts` stub
- [ ] Move `evaluateAndPossiblyRespondWithCognition()` method
- [ ] Move `evaluateAndPossiblyRespond()` method
- [ ] Move `respondToMessage()` method
- [ ] Move `shouldRespondToMessage()` method
- [ ] Move `evaluateShouldRespond()` method
- [ ] Move helper methods (isPersonaMentioned, isSenderHuman, etc.)
- [ ] Update PersonaUser to use adapter
- [ ] Run tests, fix any issues
- [ ] Commit: `refactor: extract ChatResponseAdapter (1000+ lines)`

### Phase 2: RAG Context Builder
- [ ] Create `ChatRAGContextBuilder.ts`
- [ ] Move RAG building logic
- [ ] Update ChatResponseAdapter to use builder
- [ ] Run tests
- [ ] Commit: `refactor: extract ChatRAGContextBuilder (200 lines)`

### Phase 3: Task Execution Adapter
- [ ] Create `adapters/task/` directory
- [ ] Create `TaskExecutionAdapter.ts`
- [ ] Move task execution methods
- [ ] Create TaskHandler interface
- [ ] Implement task handlers
- [ ] Update PersonaUser to use adapter
- [ ] Run tests
- [ ] Commit: `refactor: extract TaskExecutionAdapter (400+ lines)`

### Phase 4: Response Processor
- [ ] Create `ResponseProcessor.ts`
- [ ] Move response processing methods
- [ ] Update ChatResponseAdapter to use processor
- [ ] Run tests
- [ ] Commit: `refactor: extract ResponseProcessor (150 lines)`

### Phase 5: Cleanup
- [ ] Extract remaining utilities
- [ ] Review all adapters for optimization
- [ ] Add comprehensive documentation
- [ ] Update architecture diagrams
- [ ] Final test pass
- [ ] Commit: `refactor: complete PersonaUser adapter extraction`

### Post-Work
- [ ] Update CLAUDE.md with new architecture
- [ ] Update design docs
- [ ] Profile performance (ensure no regression)
- [ ] Create PR with full summary
- [ ] Celebrate! 🎉

---

## Next Steps

1. **Review this plan** with team/collaborators
2. **Get approval** on approach and phases
3. **Start Phase 1** - Extract ChatResponseAdapter
4. **Iterate quickly** - Keep momentum, small commits
5. **Test constantly** - No broken code at any phase

---

## Questions to Answer Before Starting

1. Should we keep PersonaUser as the event subscriber, or move that to adapters?
   - **Recommendation**: Keep in PersonaUser - it's the coordinator

2. Should adapters have direct access to PersonaUser, or only specific modules?
   - **Recommendation**: Pass specific modules (memory, decisionChain, etc.) not whole PersonaUser

3. How do we handle circular dependencies (PersonaUser → Adapter → PersonaUser)?
   - **Recommendation**: Adapters shouldn't need PersonaUser. If they do, extract that logic to a module.

4. Should adapters be stateful or stateless?
   - **Recommendation**: Stateless. State lives in PersonaMemory, PersonaSelfState, etc.

5. How do we handle CNS integration (PersonaCentralNervousSystem)?
   - **Recommendation**: CNS orchestrates PersonaUser, PersonaUser orchestrates adapters. No change to CNS.

---

**Status**: Ready for Phase 1 implementation
**Expected Timeline**: 1-2 weeks for full extraction (5 phases)
**Expected Impact**: 7x reduction in PersonaUser size, massive maintainability gains
