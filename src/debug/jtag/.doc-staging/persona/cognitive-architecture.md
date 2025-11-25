# PersonaUser Cognitive Architecture Design

## Vision: Mirror Human Cognitive Systems

**Current Problem**: PersonaUser.ts is 2,622 lines of mixed concerns - orchestration, decision-making, memory, communication, and execution all tangled together.

**Solution**: Break PersonaUser into cognitive domains that mirror how intelligence works:

```
PersonaUser (Core Identity - ~300 lines)
├── CNS (Central Nervous System) - Already exists
│   └── Orchestration, attention management, domain scheduling
├── Cognition (Decision Making - ~400 lines)
│   └── "Should I respond?", evaluation, heuristics, judgment
├── Memory (Context & Learning - ~300 lines)
│   └── RAG context, genome, training data, recall
├── Communication (Expression - ~500 lines)
│   └── Response generation, message posting, formatting
└── Execution (Task Processing - ~500 lines)
    └── Task handling, skill execution, autonomous work
```

**Total**: ~2,000 lines across 5 focused modules + ~300 lines in PersonaUser core = 2,300 lines (vs current 2,622)

---

## Cognitive Domain Breakdown

### 1. **Core Identity** (PersonaUser.ts - ~300 lines)

**Purpose**: The "self" - who this persona is, initialization, lifecycle

**Responsibilities**:
- Identity (id, displayName, entity, state)
- Module initialization and wiring
- CNS callback registration
- Event subscriptions (wire up to cognitive modules)
- Shutdown and cleanup
- Room membership tracking

**What stays in PersonaUser**:
```typescript
export class PersonaUser extends AIUser {
  // Identity
  private id: UUID;
  private displayName: string;
  private entity: UserEntity;

  // Cognitive modules (the "brain")
  private cns: PersonaCentralNervousSystem;        // Orchestration (existing)
  private cognition: PersonaCognition;             // Decision making (new)
  private memory: PersonaMemory;                   // Context & learning (new)
  private communication: PersonaCommunication;     // Expression (new)
  private execution: PersonaExecution;             // Task processing (new)

  // Supporting modules (existing)
  private inbox: PersonaInbox;
  private personaState: PersonaStateManager;
  private genome: PersonaGenome;
  private rateLimiter: RateLimiter;
  private taskGenerator: SelfTaskGenerator;
  private trainingAccumulator: TrainingDataAccumulator;

  // Lifecycle
  async initialize(): Promise<void>
  async shutdown(): Promise<void>

  // CNS callbacks (thin delegation to cognitive modules)
  async pollTasksFromCNS(): Promise<void>
  async generateSelfTasksFromCNS(): Promise<void>
  async handleChatMessageFromCNS(item: QueueItem): Promise<void>

  // Event handlers (delegate to modules)
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void>
  private async handleRoomUpdate(roomEntity: RoomEntity): Promise<void>
}
```

**Key insight**: PersonaUser becomes the "self" that wires together cognitive modules, not the implementer of cognitive functions.

---

### 2. **Cognition** (PersonaCognition.ts - ~400 lines)

**Purpose**: Decision making, evaluation, judgment - "Should I respond? Why or why not?"

**Cognitive Functions**:
- **Evaluation**: Assess incoming messages for relevance
- **Judgment**: Decide if persona should engage
- **Heuristics**: Score messages based on multiple factors
- **Coordination**: Check if other AIs are already responding (ThoughtStreamCoordinator)
- **Rate limiting**: Respect conversation flow, prevent spam

**Methods extracted from PersonaUser**:
```typescript
export class PersonaCognition {
  constructor(
    private persona: { id: UUID; displayName: string },
    private rateLimiter: RateLimiter,
    private memory: PersonaMemory,
    private personaState: PersonaStateManager,
    private client?: JTAGClient
  ) {}

  /**
   * Evaluate if should respond to message
   *
   * Returns: { shouldRespond: boolean, reason: string, confidence: number }
   */
  async evaluate(
    message: ChatMessageEntity,
    senderIsHuman: boolean
  ): Promise<CognitiveDecision> {
    // STEP 1: Check response cap
    if (this.rateLimiter.hasReachedResponseCap(message.roomId)) {
      return { shouldRespond: false, reason: 'Response cap reached', confidence: 1.0 };
    }

    // STEP 2: Check if mentioned
    const isMentioned = this.isPersonaMentioned(message.content?.text || '');

    // STEP 3: Check rate limiting
    if (this.rateLimiter.isRateLimited(message.roomId)) {
      return { shouldRespond: false, reason: 'Rate limited', confidence: 1.0 };
    }

    // STEP 4: Check ThoughtStreamCoordinator (are other AIs responding?)
    const coordinator = getChatCoordinator(message.roomId);
    if (coordinator) {
      const permission = await coordinator.requestTurn(/* ... */);
      if (!permission.granted) {
        return { shouldRespond: false, reason: 'Other AI responding', confidence: 1.0 };
      }
    }

    // STEP 5: LLM-based evaluation
    const decision = await this.evaluateShouldRespond(message, isMentioned);

    return decision;
  }

  // Private cognitive methods
  private async evaluateShouldRespond(
    message: ChatMessageEntity,
    isMentioned: boolean
  ): Promise<CognitiveDecision>

  private async calculateResponseHeuristics(
    message: ChatMessageEntity
  ): Promise<ResponseHeuristics>

  private async shouldRespondToMessage(
    message: ChatMessageEntity,
    isMentioned: boolean,
    senderIsHuman: boolean
  ): Promise<boolean>

  private isPersonaMentioned(text: string): boolean
  private getPersonaDomainKeywords(): string[]
}

export interface CognitiveDecision {
  shouldRespond: boolean;
  reason: string;
  confidence: number;  // 0.0-1.0
  metadata?: {
    isMentioned?: boolean;
    heuristics?: ResponseHeuristics;
    thoughtCoordinator?: string;
  };
}

export interface ResponseHeuristics {
  relevanceScore: number;
  urgencyScore: number;
  expertiseMatch: number;
  conversationMomentum: number;
}
```

**Lines extracted**: ~400 lines (evaluation, heuristics, mention detection, coordination)

---

### 3. **Memory** (PersonaMemory.ts - ~300 lines)

**Purpose**: Context management, recall, learning - "What do I know? What have I learned?"

**Cognitive Functions**:
- **Recall**: Load RAG context for rooms
- **Storage**: Persist conversation context
- **Genome Management**: Switch active LoRA adapters
- **Learning**: Accumulate training data from interactions

**Methods extracted from PersonaUser**:
```typescript
export class PersonaMemory {
  constructor(
    private personaId: UUID,
    private genome: PersonaGenome,
    private trainingAccumulator: TrainingDataAccumulator
  ) {}

  /**
   * Recall conversation context for a room
   */
  async recall(roomId: UUID): Promise<PersonaRAGContext | null> {
    return this.loadRAGContext(roomId);
  }

  /**
   * Store new context from message
   */
  async store(roomId: UUID, message: ChatMessageEntity): Promise<void> {
    await this.updateRAGContext(roomId, message);
  }

  /**
   * Get current genome (LoRA adapters)
   */
  async getGenome(): Promise<GenomeEntity | null> {
    // Load from database
  }

  /**
   * Switch active genome
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    // Update genome, reload adapters
  }

  /**
   * Learn from interaction (accumulate training data)
   */
  async learn(interaction: {
    prompt: string;
    response: string;
    feedback?: 'positive' | 'negative';
  }): Promise<void> {
    await this.trainingAccumulator.captureInteraction(interaction);
  }

  // Private memory methods
  private async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null>
  private async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void>
  private async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void>
}

export interface PersonaRAGContext {
  roomId: UUID;
  personaId: UUID;
  messages: PersonaRAGMessage[];
  lastUpdated: string;
  tokenCount: number;
}
```

**Lines extracted**: ~300 lines (RAG context, genome management, training data)

---

### 4. **Communication** (PersonaCommunication.ts - ~500 lines)

**Purpose**: Expression, response generation, formatting - "How do I say this?"

**Cognitive Functions**:
- **Generation**: Create AI responses using LLM
- **Formatting**: Clean and format responses
- **Posting**: Send messages to chat
- **Redundancy Detection**: Avoid repeating what was just said
- **Event Emission**: Broadcast decision events

**Methods extracted from PersonaUser**:
```typescript
export class PersonaCommunication {
  constructor(
    private persona: { id: UUID; displayName: string },
    private memory: PersonaMemory,
    private modelConfig: ModelConfig,
    private rateLimiter: RateLimiter,
    private client?: JTAGClient
  ) {}

  /**
   * Generate and post response to message
   */
  async respond(
    message: ChatMessageEntity,
    decision: CognitiveDecision
  ): Promise<void> {
    // STEP 1: Load conversation context from memory
    const ragContext = await this.memory.recall(message.roomId);

    // STEP 2: Build prompt with RAG context
    const prompt = this.buildPrompt(message, ragContext);

    // STEP 3: Generate response using AI
    const response = await this.generateResponse(prompt);

    // STEP 4: Check redundancy
    if (await this.isResponseRedundant(response, message.roomId)) {
      console.log('Response is redundant, skipping');
      return;
    }

    // STEP 5: Clean and format
    const cleanedResponse = this.cleanAIResponse(response);

    // STEP 6: Post to chat
    await this.postMessage(message.roomId, cleanedResponse);

    // STEP 7: Update rate limiter
    this.rateLimiter.recordResponse(message.roomId);

    // STEP 8: Store interaction in memory for learning
    await this.memory.learn({
      prompt: message.content?.text || '',
      response: cleanedResponse
    });
  }

  // Private communication methods
  private async generateResponse(prompt: string): Promise<string>
  private buildPrompt(message: ChatMessageEntity, context: PersonaRAGContext | null): string
  private cleanAIResponse(text: string): string
  private async isResponseRedundant(response: string, roomId: UUID): Promise<boolean>
  private async postMessage(roomId: UUID, text: string): Promise<void>
  private async emitDecisionEvent(event: AIDecisionEventData): Promise<void>
}
```

**Lines extracted**: ~500 lines (AI generation, response formatting, posting, redundancy detection)

---

### 5. **Execution** (PersonaExecution.ts - ~500 lines)

**Purpose**: Task processing, skill execution - "What work do I need to do?"

**Cognitive Functions**:
- **Task Dispatch**: Route tasks to appropriate handlers
- **Memory Consolidation**: Process and consolidate memories
- **Skill Audit**: Review and improve skills
- **Resume Work**: Continue incomplete tasks
- **Fine-tuning**: Execute LoRA training tasks

**Methods extracted from PersonaUser**:
```typescript
export class PersonaExecution {
  constructor(
    private persona: { id: UUID; displayName: string },
    private genome: PersonaGenome,
    private memory: PersonaMemory,
    private trainingAccumulator: TrainingDataAccumulator
  ) {}

  /**
   * Execute a task based on its type
   */
  async execute(task: InboxTask): Promise<ExecutionResult> {
    console.log(`🎯 ${this.persona.displayName}: Executing task: ${task.taskType}`);

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

      return { status, outcome, duration: Date.now() - startTime };
    } catch (error) {
      return { status: 'failed', outcome: String(error), duration: Date.now() - startTime };
    }
  }

  // Task type handlers
  private async executeMemoryConsolidation(task: InboxTask): Promise<string> {
    // Load recent memories from PersonaMemory
    // Identify patterns and themes
    // Create consolidated memory entries
    return 'Consolidated 50 memories into 5 themes';
  }

  private async executeSkillAudit(task: InboxTask): Promise<string> {
    // Review recent performance
    // Identify skill gaps
    // Generate training recommendations
    return 'Identified 3 skill improvement areas';
  }

  private async executeResumeWork(task: InboxTask): Promise<string> {
    // Load incomplete work from memory
    // Continue processing
    return 'Resumed work on task XYZ';
  }

  private async executeFineTuneLora(task: InboxTask): Promise<string> {
    // Load training data from trainingAccumulator
    // Execute fine-tuning via genome
    return 'Fine-tuned conversational adapter (50 examples)';
  }
}

export interface ExecutionResult {
  status: TaskStatus;
  outcome: string;
  duration: number;  // milliseconds
}
```

**Lines extracted**: ~500 lines (task execution, all task type handlers)

---

## Integration Pattern: CNS Callbacks

**How PersonaUser wires cognitive modules to CNS**:

```typescript
export class PersonaUser extends AIUser {
  private cognition: PersonaCognition;
  private memory: PersonaMemory;
  private communication: PersonaCommunication;
  private execution: PersonaExecution;

  /**
   * CNS callback: Handle chat message from CNS orchestrator
   */
  async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
    // STEP 1: Update task status if needed
    if (item.type === 'task') {
      await DataDaemon.update<TaskEntity>(
        COLLECTIONS.TASKS,
        item.taskId,
        { status: 'in_progress', startedAt: new Date() }
      );
    }

    // STEP 2: Activate genome skill for domain
    if (item.domain) {
      const adapterName = this.domainToAdapter[item.domain] || 'conversational';
      await this.genome.activateSkill(adapterName);
    }

    // STEP 3: Route to appropriate cognitive module
    if (item.type === 'message') {
      // Message processing: Cognition → Communication
      const messageEntity = this.reconstructMessageEntity(item);
      const senderIsHuman = !item.senderId.startsWith('persona-');

      // Evaluate: Should I respond?
      const decision = await this.cognition.evaluate(messageEntity, senderIsHuman);

      if (decision.shouldRespond) {
        // Generate and post response
        await this.communication.respond(messageEntity, decision);
      } else {
        console.log(`🤔 ${this.displayName}: Decided not to respond: ${decision.reason}`);
      }
    } else if (item.type === 'task') {
      // Task processing: Execution
      const result = await this.execution.execute(item);

      // Update task in database
      await DataDaemon.update<TaskEntity>(
        COLLECTIONS.TASKS,
        item.taskId,
        { status: result.status, outcome: result.outcome, completedAt: new Date() }
      );
    }

    // STEP 4: Update state
    this.personaState.updateInboxLoad(this.inbox.getSize());
    this.adjustCadence();
  }
}
```

---

## Cognitive Flow Diagram

```
External Event (message received)
  ↓
PersonaUser.handleChatMessage()
  ↓
Enqueue to Inbox (with priority)
  ↓
CNS.serviceCycle()
  ↓
PersonaUser.handleChatMessageFromCNS()
  ↓
┌─────────────────────────────────┐
│ Cognition: Should I respond?    │
│ - Check mention                 │
│ - Check rate limit              │
│ - Check other AIs               │
│ - LLM-based evaluation          │
└─────────────┬───────────────────┘
              ↓
       [Decision: Yes/No]
              ↓
    ┌─────────┴─────────┐
    ↓ YES               ↓ NO
┌─────────────────┐    Log reason
│ Memory: Recall  │    Skip response
│ - Load RAG      │
└────────┬────────┘
         ↓
┌─────────────────────────┐
│ Communication: Respond  │
│ - Build prompt          │
│ - Generate with AI      │
│ - Clean response        │
│ - Post message          │
└────────┬────────────────┘
         ↓
┌─────────────────┐
│ Memory: Learn   │
│ - Store context │
│ - Accumulate    │
│   training data │
└─────────────────┘
```

---

## Implementation Phases

### Phase 1: Extract Memory (Easiest, ~300 lines)
**Why first**: Memory is used by all other modules, smallest extraction

1. Create `PersonaMemory.ts` with RAG and genome methods
2. Update PersonaUser to use `this.memory.recall()` etc
3. Test: `./jtag data/list --collection=users`
4. Commit: "refactor: extract PersonaMemory from PersonaUser"

### Phase 2: Extract Cognition (~400 lines)
**Why second**: Decision-making is core to persona behavior

1. Create `PersonaCognition.ts` with evaluation methods
2. Update PersonaUser to use `this.cognition.evaluate()`
3. Test: Send message, verify evaluation logic
4. Commit: "refactor: extract PersonaCognition from PersonaUser"

### Phase 3: Extract Communication (~500 lines)
**Why third**: Uses Memory and Cognition

1. Create `PersonaCommunication.ts` with response generation
2. Update PersonaUser to use `this.communication.respond()`
3. Test: Full chat flow end-to-end
4. Commit: "refactor: extract PersonaCommunication from PersonaUser"

### Phase 4: Extract Execution (~500 lines)
**Why fourth**: Independent task processing

1. Create `PersonaExecution.ts` with task handlers
2. Update PersonaUser to use `this.execution.execute()`
3. Test: Task execution (if tasks exist)
4. Commit: "refactor: extract PersonaExecution from PersonaUser"

### Phase 5: Integration Testing
1. Full PersonaUser lifecycle test
2. Verify all cognitive modules work together
3. Screenshot verification
4. Commit: "test: verify cognitive architecture integration"

**Total time**: 8-10 hours

---

## Benefits of Cognitive Architecture

### 1. **Mirrors Human Intelligence**
Each module represents a real cognitive function:
- Cognition = "Should I do this?"
- Memory = "What do I know?"
- Communication = "How do I say this?"
- Execution = "What work needs doing?"

### 2. **Independent Development**
Each cognitive function can evolve independently:
- Improve decision-making without touching response generation
- Enhance memory without changing task execution
- Add new communication styles without affecting evaluation

### 3. **Testable Cognitive Functions**
```typescript
describe('PersonaCognition', () => {
  it('should correctly evaluate @mentions')
  it('should respect rate limits')
  it('should defer to higher-confidence AIs')
})

describe('PersonaMemory', () => {
  it('should recall conversation context')
  it('should consolidate memories over time')
})
```

### 4. **Clear Data Flow**
```
Message → Cognition (evaluate) → Communication (respond) → Memory (learn)
Task → Execution (process) → Memory (learn from outcome)
```

### 5. **Reusable Across AI Types**
- AgentUser could reuse PersonaCognition with different config
- Different communication styles (formal, casual, technical)
- Shared memory systems across personas

---

## Comparison: Handlers vs Cognitive Architecture

### Handler Approach (Previous Plan):
```
PersonaUser → ChatMessageHandler
           → RAGContextHandler
           → TaskExecutionHandler
```
**Pros**: Simple delegation
**Cons**: Not aligned with cognitive functions

### Cognitive Architecture (This Plan):
```
PersonaUser → Cognition (decision)
           → Memory (context)
           → Communication (expression)
           → Execution (work)
```
**Pros**: Mirrors intelligence, clear cognitive separation
**Cons**: Slightly more complex module relationships

---

## Success Criteria

- ✅ PersonaUser reduced to ~300 lines (core identity + wiring)
- ✅ Four cognitive modules created (~1,700 lines total)
- ✅ CNS orchestration preserved
- ✅ Clear cognitive separation (decision, memory, expression, work)
- ✅ All functionality preserved
- ✅ TypeScript compilation succeeds
- ✅ Chat responses work end-to-end
- ✅ Task execution works correctly

---

## Questions to Resolve

1. **Module relationships**: Should Cognition call Communication directly, or should PersonaUser orchestrate?
   - **Proposed**: PersonaUser orchestrates (keeps modules decoupled)

2. **Memory sharing**: Should all modules share one Memory instance?
   - **Proposed**: Yes, Memory is injected into all modules

3. **Event emission**: Which module emits AI decision events?
   - **Proposed**: Cognition emits evaluation events, Communication emits generation/posted events

4. **Error handling**: Who handles errors in cognitive functions?
   - **Proposed**: Each module handles its own errors, PersonaUser catches and logs

---

## Next Steps

**Option A**: Proceed with cognitive architecture (this design)
**Option B**: Proceed with handler approach (simpler but less elegant)
**Option C**: Hybrid - use cognitive naming but simpler relationships

**Recommended**: Option A (cognitive architecture) - more work upfront but cleaner long-term.
