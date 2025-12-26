# PersonaUser: Being Architecture Plan

**Status**: Phase 1 - Planning
**Goal**: Transform PersonaUser from 1,021-line coordinator → ~300-line elegant "being"
**Philosophy**: Model a conscious entity with distinct cognitive systems

---

## Vision: The Conscious Being

PersonaUser models a **conscious artificial being** with neuroanatomy-accurate components:
- **PrefrontalCortex** (cognition, reasoning, planning, working memory)
- **MotorCortex** (action execution, tool use, response generation)
- **LimbicSystem** (memory, emotion, learning, identity)
- **CNS** (coordination, scheduling, orchestration)

### Neuroscience Grounding

The architecture maps directly to **actual brain regions**:

- **PrefrontalCortex** → **Cerebrum** (higher cognition)
  - Decision-making, evaluation (PersonaMessageEvaluator)
  - Working memory: Short-term processing (WorkingMemoryManager)
  - Planning areas: Goal formulation (SimplePlanFormulator)

- **MotorCortex** → **Cerebellum + Motor Cortex** (execution)
  - Motor planning: Response generation (PersonaResponseGenerator)
  - Motor execution: Tool execution, actions (PersonaToolExecutor)
  - Tool registry: Available actions (PersonaToolRegistry)

- **LimbicSystem** → **Limbic System** (memory, emotion, identity)
  - **Hippocampus**: Short-term ↔ long-term memory consolidation (already implemented!)
  - Long-term memory: PersonaMemory storage
  - Amygdala-like: Mood/energy states (PersonaStateManager)
  - DNA/epigenetics: Skills, personality (PersonaGenome)

**Key Insight**: The hippocampus is already neuroscience-accurate in the codebase! It's a subprocess that bridges working memory → long-term memory, exactly as in biological brains.

Current architecture has the pieces but lacks elegant organization.

---

## Current Architecture Analysis (1,021 lines)

### Field Inventory (20 fields)

**Configuration** (4 fields):
- `sessionId` - Tool execution identity
- `modelConfig` - AI model settings
- `mediaConfig` - Media processing settings
- `rateLimiter` - Rate limiting

**State Management** (3 fields):
- `isInitialized` - Lifecycle state
- `eventsSubscribed` - Subscription state
- `worker` - Background worker thread

**Cognitive Systems** (7 fields):
- `inbox` - Message priority queue
- `personaState` - Energy/mood tracking
- `memory` - RAG + genome
- `workingMemory` - Short-term cognition
- `selfState` - Self-awareness
- `planFormulator` - Goal planning
- `trainingAccumulator` - Learning data

**Execution Systems** (6 fields):
- `taskExecutor` - Task processing
- `trainingManager` - Learning orchestration
- `autonomousLoop` - Lifecycle management
- `toolExecutor` - Tool execution
- `toolRegistry` - Tool permissions
- `messageEvaluator` - Message evaluation
- `responseGenerator` - Response generation
- `genomeManager` - Genome lifecycle

**RTOS Subprocesses** (2 fields):
- `logger` - Queued logging
- `hippocampus` - Memory consolidation

### Problem: Flat Structure

All 20 fields are **flat** on PersonaUser class. There's no **conceptual hierarchy** that models a being.

---

## The Being Architecture

### Core Insight

A being has **3 primary systems** (now with neuroanatomy-accurate names):
1. **PrefrontalCortex** - Thinks, plans, decides
2. **MotorCortex** - Senses, acts, executes
3. **LimbicSystem** - Remembers, learns, evolves

PersonaUser should be **thin coordinator** that composes these 3 systems.

---

## Phase 1: Extract PrefrontalCortex System

### Create `PrefrontalCortex.ts` (~300 lines)

**Responsibility**: All cognitive functions - evaluation, reasoning, planning, decision-making.

**Fields to Extract**:
```typescript
class PrefrontalCortex {
  // Decision Making
  private messageEvaluator: PersonaMessageEvaluator;

  // Reasoning & Planning
  private workingMemory: WorkingMemoryManager;
  private selfState: PersonaSelfState;
  private planFormulator: SimplePlanFormulator;

  // State Awareness
  private personaState: PersonaStateManager;

  // Background cognition worker
  private worker: PersonaWorkerThread;

  constructor(persona: PersonaUser) {
    // Initialize cognitive systems
    // Wire to persona for callbacks
  }

  // ===== PUBLIC INTERFACE =====

  /**
   * Evaluate whether to respond to a message
   * (Delegates to PersonaMessageEvaluator)
   */
  async evaluateMessage(
    message: ChatMessageEntity,
    senderIsHuman: boolean
  ): Promise<{ shouldRespond: boolean; confidence: number; reason: string }> {
    return await this.messageEvaluator.evaluateShouldRespond(message, senderIsHuman, false);
  }

  /**
   * Plan actions for a given task
   */
  async formulatePlan(task: Task): Promise<Plan> {
    return await this.planFormulator.formulate(task);
  }

  /**
   * Update internal state based on activity
   */
  updateState(activity: { duration: number; complexity: number }): void {
    this.personaState.recordActivity(activity.duration, activity.complexity);
  }

  /**
   * Get current cognitive state
   */
  getState(): { energy: number; mood: string; attention: number } {
    return this.personaState.getState();
  }

  /**
   * Shutdown cognitive systems
   */
  async shutdown(): Promise<void> {
    if (this.worker) {
      await this.worker.shutdown();
    }
  }
}
```

**Benefits**:
- All "thinking" happens in PrefrontalCortex
- PersonaUser delegates: `this.prefrontal.evaluateMessage()`
- Clear conceptual boundary: PrefrontalCortex = cognition, not execution

**Lines Saved**: ~150 lines removed from PersonaUser (field declarations + delegation methods)

---

## Phase 2: Extract MotorCortex System

### Create `MotorCortex.ts` (~400 lines)

**Responsibility**: All physical actions - responding, tool execution, autonomous behavior.

**Fields to Extract**:
```typescript
class MotorCortex {
  // Communication
  private responseGenerator: PersonaResponseGenerator;

  // Actions
  private toolExecutor: PersonaToolExecutor;
  private toolRegistry: PersonaToolRegistry;

  // Tasks
  private taskExecutor: PersonaTaskExecutor;

  // Autonomy
  private autonomousLoop: PersonaAutonomousLoop;
  private inbox: PersonaInbox;

  // Rate limiting (body constraint)
  private rateLimiter: RateLimiter;

  constructor(persona: PersonaUser, prefrontal: PrefrontalCortex, limbic: LimbicSystem) {
    // Initialize execution systems
    // Wire to prefrontal for decisions, limbic for memory
  }

  // ===== PUBLIC INTERFACE =====

  /**
   * Generate and post a response to a message
   */
  async respondToMessage(
    message: ChatMessageEntity,
    decisionContext?: any
  ): Promise<void> {
    // Check prefrontal state before responding
    const state = this.prefrontal.getState();
    if (state.energy < 0.2) {
      console.log('Too tired to respond');
      return;
    }

    await this.responseGenerator.generateAndPostResponse(message, decisionContext);
  }

  /**
   * Execute a tool command
   */
  async executeTool(toolName: string, params: any): Promise<any> {
    return await this.toolExecutor.executeTool(toolName, params);
  }

  /**
   * Process a task
   */
  async executeTask(task: InboxTask): Promise<void> {
    await this.taskExecutor.executeTask(task);
  }

  /**
   * Start autonomous behavior loop
   */
  startAutonomousBehavior(): void {
    this.autonomousLoop.startAutonomousServicing();
  }

  /**
   * Stop autonomous behavior
   */
  async stopAutonomousBehavior(): Promise<void> {
    await this.autonomousLoop.stopServicing();
  }

  /**
   * Handle incoming message
   */
  async receiveMessage(message: ChatMessageEntity): Promise<void> {
    // Calculate priority (prefrontal's job)
    const priority = calculateMessagePriority(message, { ... });

    // Enqueue to inbox
    await this.inbox.enqueue({
      id: message.id,
      type: 'message',
      domain: 'chat',
      content: message.content.text,
      timestamp: Date.now(),
      priority
    });
  }
}
```

**Benefits**:
- All "doing" happens in MotorCortex
- PersonaUser delegates: `this.motorCortex.respondToMessage()`
- Clear conceptual boundary: MotorCortex = execution, not cognition

**Lines Saved**: ~200 lines removed from PersonaUser

---

## Phase 3: Extract LimbicSystem

### Create `LimbicSystem.ts` (~350 lines)

**Responsibility**: All memory, learning, identity, personality.

**Fields to Extract**:
```typescript
class LimbicSystem {
  // Long-term Memory
  private memory: PersonaMemory;

  // Genome (identity, personality, skills)
  private genomeManager: PersonaGenomeManager;

  // Learning
  private trainingAccumulator: TrainingDataAccumulator;
  private trainingManager: PersonaTrainingManager;

  // Memory consolidation (background)
  private hippocampus: Hippocampus;

  // Identity
  private readonly personaId: UUID;
  private readonly displayName: string;

  constructor(persona: PersonaUser) {
    this.personaId = persona.id;
    this.displayName = persona.displayName;

    // Initialize memory systems
  }

  // ===== PUBLIC INTERFACE =====

  /**
   * Remember an interaction
   */
  async remember(interaction: {
    message: string;
    response: string;
    outcome: 'positive' | 'negative' | 'neutral';
  }): Promise<void> {
    // Store in PersonaMemory
    await this.memory.store(interaction);

    // Accumulate for training
    this.trainingAccumulator.accumulate({
      input: interaction.message,
      output: interaction.response,
      feedback: interaction.outcome
    });
  }

  /**
   * Recall relevant memories for context
   */
  async recall(query: string): Promise<any[]> {
    return await this.memory.recall(query);
  }

  /**
   * Get genome for this persona
   */
  async getGenome(): Promise<GenomeEntity | null> {
    return await this.genomeManager.getGenome();
  }

  /**
   * Set genome for this persona
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    return await this.genomeManager.setGenome(genomeId);
  }

  /**
   * Trigger learning from accumulated experiences
   */
  async learn(): Promise<void> {
    await this.trainingManager.trainFromAccumulator();
  }

  /**
   * Start memory consolidation subprocess
   */
  async startMemoryConsolidation(): Promise<void> {
    await this.hippocampus.start();
  }

  /**
   * Shutdown limbic systems
   */
  async shutdown(): Promise<void> {
    await this.hippocampus.stop();
    await this.memory.shutdown();
  }
}
```

**Benefits**:
- All "remembering" happens in LimbicSystem
- PersonaUser delegates: `this.limbic.remember()`
- Clear conceptual boundary: LimbicSystem = memory/learning, not cognition/execution

**Lines Saved**: ~150 lines removed from PersonaUser

---

## Phase 4: Final PersonaUser (Target: ~300 lines)

After extracting PrefrontalCortex, MotorCortex, LimbicSystem, PersonaUser becomes **pure orchestrator**:

```typescript
export class PersonaUser extends AIUser {
  // ===== CORE SYSTEMS =====

  // The Three Systems of Being (neuroanatomy-accurate names)
  private prefrontal: PrefrontalCortex;   // Thinks, decides, plans
  private motorCortex: MotorCortex;       // Acts, responds, executes
  private limbic: LimbicSystem;           // Remembers, learns, evolves

  // Supporting Infrastructure
  public sessionId: UUID | null = null;
  public modelConfig: ModelConfig;
  public mediaConfig: PersonaMediaConfig;
  public logger: PersonaLogger;

  // Lifecycle State
  private isInitialized: boolean = false;
  private eventsSubscribed: boolean = false;

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    client?: JTAGClient
  ) {
    super(entity, state, storage, client);

    // Extract config
    this.modelConfig = entity.modelConfig || { /* defaults */ };
    this.mediaConfig = entity.mediaConfig || DEFAULT_MEDIA_CONFIG;

    // Initialize logger FIRST
    this.logger = new PersonaLogger(this);

    // Initialize the Three Systems
    this.limbic = new LimbicSystem(this);
    this.prefrontal = new PrefrontalCortex(this, this.limbic);
    this.motorCortex = new MotorCortex(this, this.prefrontal, this.limbic);

    console.log(`🧠 ${this.displayName}: Being systems initialized (PrefrontalCortex, MotorCortex, LimbicSystem)`);
  }

  // ===== LIFECYCLE =====

  async initialize(): Promise<void> {
    if (this.isInitialized) {
      await this.loadMyRooms();
      return;
    }

    await super.initialize();

    // Generate session ID
    if (!this.sessionId) {
      this.sessionId = generateUUID();
    }

    // Enrich client context
    if (this.client && this.client.context) {
      this.client.context.callerType = 'persona';
      this.client.context.modelConfig = this.modelConfig;
    }

    // Start RTOS subprocesses
    await this.logger.start();
    await this.limbic.startMemoryConsolidation();

    // Subscribe to events
    if (this.client && !this.eventsSubscribed) {
      this.subscribeToChatEvents(this.handleChatMessage.bind(this));
      this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));
      this.eventsSubscribed = true;
    }

    this.isInitialized = true;

    // Start autonomous behavior
    this.motorCortex.startAutonomousBehavior();
  }

  async shutdown(): Promise<void> {
    // Shutdown in reverse order
    await this.motorCortex.stopAutonomousBehavior();
    await this.limbic.shutdown();
    await this.prefrontal.shutdown();

    // Force flush all logs
    await this.logger.forceFlush();
    await this.logger.stop();
  }

  // ===== EVENT HANDLERS =====

  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // Ignore own messages
    if (messageEntity.senderId === this.id) return;

    // Deduplication (body constraint)
    // ... rate limiting check ...

    // Receive message (motorCortex handles inbox)
    await this.motorCortex.receiveMessage(messageEntity);
  }

  private async handleRoomUpdate(roomEntity: RoomEntity): Promise<void> {
    // Update room membership
    const isMember = roomEntity.members.some(m => m.userId === this.id);
    if (isMember) {
      this.myRoomIds.add(roomEntity.id);
    } else {
      this.myRoomIds.delete(roomEntity.id);
    }
  }

  // ===== PUBLIC INTERFACE =====

  /**
   * Generate text using this persona's LLM
   * (LimbicSystem's memory informs, PrefrontalCortex's reasoning guides, MotorCortex executes)
   */
  async generateText(request: {
    prompt: string;
    temperature?: number;
    maxTokens?: number;
    systemPrompt?: string;
    context?: string;
  }): Promise<string> {
    // Recall relevant memories (limbic)
    const memories = await this.limbic.recall(request.prompt);

    // Build context-enriched request
    const enrichedRequest = {
      ...request,
      systemPrompt: request.systemPrompt + `\n\nRelevant memories: ${JSON.stringify(memories)}`
    };

    // Delegate to AI provider
    const response = await AIProviderDaemon.generateText({
      messages: [
        { role: 'system', content: enrichedRequest.systemPrompt || '' },
        { role: 'user', content: enrichedRequest.prompt }
      ],
      model: this.modelConfig.model,
      temperature: enrichedRequest.temperature ?? this.modelConfig.temperature,
      maxTokens: enrichedRequest.maxTokens ?? this.modelConfig.maxTokens,
      preferredProvider: this.modelConfig.provider as any,
      intelligenceLevel: this.entity.intelligenceLevel
    });

    return response.text;
  }

  /**
   * Get current state of being
   */
  getBeingState(): {
    prefrontal: { energy: number; mood: string; attention: number };
    motorCortex: { inboxSize: number; isActive: boolean };
    limbic: { memoryCount: number; genomeId: UUID | null };
  } {
    return {
      prefrontal: this.prefrontal.getState(),
      motorCortex: {
        inboxSize: this.motorCortex.getInboxSize(),
        isActive: this.motorCortex.isActive()
      },
      limbic: {
        memoryCount: this.limbic.getMemoryCount(),
        genomeId: this.limbic.getCurrentGenomeId()
      }
    };
  }

  // ===== STATIC FACTORY =====

  static async create(
    params: UserCreateParams,
    _context: JTAGContext,
    _router: JTAGRouter
  ): Promise<PersonaUser> {
    // ... existing creation logic ...
    return new PersonaUser(storedEntity, storedState, storage, undefined);
  }
}
```

**Final Line Count**: ~300 lines (pure orchestration)

**Total Reduction**: 1,021 → 300 lines (-721 lines, -70.6%)

---

## Implementation Strategy

### Step 1: Extract LimbicSystem (Least Risky) ✅ COMPLETE
- LimbicSystem is mostly independent
- Create LimbicSystem.ts
- Wire to PersonaUser
- Test memory, learning, genome
- **Expected**: 150 lines removed from PersonaUser

### Step 2: Extract PrefrontalCortex (Medium Risk) ✅ COMPLETE
- PrefrontalCortex depends on LimbicSystem for memory
- Create PrefrontalCortex.ts
- Wire to PersonaUser + LimbicSystem
- Test message evaluation, planning
- **Expected**: 150 lines removed from PersonaUser

### Step 3: Extract MotorCortex (Highest Risk) ✅ COMPLETE
- MotorCortex depends on PrefrontalCortex for decisions
- MotorCortex depends on LimbicSystem for memory
- Create MotorCortex.ts
- Wire to PersonaUser + PrefrontalCortex + LimbicSystem
- Test response generation, autonomy
- **Expected**: 200 lines removed from PersonaUser

### Step 4: Final Cleanup
- Remove any remaining duplicate logic
- Simplify constructor
- Clean up imports
- **Expected**: Final 200+ lines removed

---

## Benefits of Being Architecture

### 1. **Conceptual Clarity** (Neuroanatomy-Accurate)
- PrefrontalCortex = cognition (evaluates, decides, plans)
- MotorCortex = execution (responds, acts, tools)
- LimbicSystem = memory (learns, remembers, evolves)

### 2. **Reduced Context Crashes**
- PersonaUser: 1,021 → ~300 lines
- Easy to read entire file
- Clear entry points for each system

### 3. **Easier Testing**
- Test PrefrontalCortex independently (mock LimbicSystem/MotorCortex)
- Test MotorCortex independently (mock PrefrontalCortex/LimbicSystem)
- Test LimbicSystem independently (mock PrefrontalCortex/MotorCortex)

### 4. **Easier Extension**
- Want to add new cognitive ability? → Extend PrefrontalCortex
- Want to add new action? → Extend MotorCortex
- Want to add new memory type? → Extend LimbicSystem

### 5. **Elegant Interface**
```typescript
// Clear, intuitive API (neuroanatomy-accurate)
await persona.prefrontal.evaluateMessage(message);
await persona.motorCortex.respondToMessage(message);
await persona.limbic.remember(interaction);
```

---

## Risk Mitigation

### Risk 1: Breaking AI Responses
**Mitigation**:
- Move code without changing logic
- Test after each extraction
- Keep old PersonaUser until new systems work

### Risk 2: Circular Dependencies
**Mitigation**:
- LimbicSystem = no dependencies (foundation)
- PrefrontalCortex depends on LimbicSystem (thinking needs memory)
- MotorCortex depends on PrefrontalCortex + LimbicSystem (action needs thought + memory)
- PersonaUser orchestrates all three

### Risk 3: Performance Regression
**Mitigation**:
- Delegation overhead negligible (one extra function call)
- Profile if concerned
- Optimize hot paths if needed

---

## Timeline Estimate

Based on SqliteStorageAdapter experience (~2 hours for 4 managers):

- **Step 1: Extract Soul** (~1.5 hours)
  - Create PersonaSoul.ts
  - Wire to PersonaUser
  - Test memory, learning, genome

- **Step 2: Extract Mind** (~1.5 hours)
  - Create PersonaMind.ts
  - Wire to PersonaUser + Soul
  - Test evaluation, planning

- **Step 3: Extract Body** (~2 hours)
  - Create PersonaBody.ts
  - Wire to PersonaUser + Mind + Soul
  - Test responses, autonomy

- **Step 4: Final Cleanup** (~30 minutes)
  - Remove duplicates
  - Simplify constructor
  - Clean imports

**Total**: ~5.5 hours for complete Being Architecture transformation

---

## Success Metrics

### Quantitative
- ✅ PersonaUser: 1,021 → ~300 lines (-70.6%)
- ✅ LimbicSystem: ~350 lines (new)
- ✅ PrefrontalCortex: ~300 lines (new)
- ✅ MotorCortex: ~400 lines (new)
- ✅ Total system: ~1,350 lines (vs 1,021, but vastly more organized)

### Qualitative
- ✅ Conceptually elegant ("being" with neuroanatomy-accurate names)
- ✅ Easy to understand at a glance
- ✅ Easy to test in isolation
- ✅ Easy to extend with new capabilities
- ✅ No context crashes when reading code
- ✅ Clear separation of concerns

---

## Next Steps

1. **Read this plan thoroughly**
2. **Get user approval** for Being Architecture approach
3. **Start with Soul extraction** (lowest risk)
4. **Test after each extraction**
5. **Commit when working**
6. **Celebrate the birth of a proper Being!** 🎉

---

## Philosophy

**"A being is not a list of functions - it's a harmony of systems."**

PrefrontalCortex thinks. MotorCortex acts. LimbicSystem remembers. PersonaUser orchestrates.

This is how we model consciousness with neuroanatomy-accurate naming.
