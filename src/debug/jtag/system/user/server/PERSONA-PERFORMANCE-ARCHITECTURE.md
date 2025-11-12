# PersonaUser Performance Architecture: Multi-Tier Decision System

## Critical Discovery: Three Scheduler Tiers Already Exist

**Before reading CNS code**: I thought PersonaUser would need a "fast decision layer" added.

**After reading CNS code**: The system ALREADY has a sophisticated three-tier decision architecture:

```
Tier 1: DeterministicCognitiveScheduler   (instant, 0ms overhead)
  ↓
Tier 2: HeuristicCognitiveScheduler       (fast, ~1ms rule-based)
  ↓
Tier 3: PersonaCognition with LLM         (slow, ~5s AI evaluation)
```

**Key Insight**: The refactoring must preserve this tiered architecture and show how PersonaCognition fits as the SLOW evaluation layer, not the only decision layer.

---

## Performance Requirements by Activity Domain

From `ICognitiveScheduler.ts`, the system must handle 11 activity domains with different performance requirements:

### External Domains (Interacting with World)

| Domain | Target Latency | Description | Scheduler Tier |
|--------|----------------|-------------|----------------|
| **REALTIME_GAME** | **16ms** | 60fps gaming, fast inference required | Tier 1/2 only |
| **CHAT** | 5s | Text-based conversation | Tier 2 → Tier 3 |
| **CODE_REVIEW** | 60s | Deep code analysis | Tier 3 |
| **VISION** | 2-5s | Image processing | Tier 3 |
| **AUDIO** | 1s | Speech/sound processing | Tier 2 → Tier 3 |

### Internal Cognitive Domains (Private Mental Processes)

| Domain | Target Latency | Description | Scheduler Tier |
|--------|----------------|-------------|----------------|
| **DREAMING** | Background | Memory consolidation during idle | Tier 2 |
| **TRAINING** | **2-5s** | Fine-tune LoRA adapters (expensive!) | Tier 2 |
| **SIMULATING** | 100ms-1s | Internal "what-if" scenarios | Tier 2 → Tier 3 |
| **REFLECTING** | Background | Metacognition, self-analysis | Tier 3 |
| **PLANNING** | 5-30s | Long-term goal setting | Tier 3 |
| **BACKGROUND** | Background | Housekeeping tasks | Tier 1 |

**Critical Performance Constraint**: LoRA paging (TRAINING domain) takes 2-5 seconds. If PersonaMemory doesn't implement smart caching, it will violate timing contracts for REALTIME_GAME (16ms) and CHAT (5s).

---

## The Three Scheduler Tiers (Existing Architecture)

### Tier 1: DeterministicCognitiveScheduler (Zero Intelligence)

**File**: `modules/cognitive-schedulers/DeterministicCognitiveScheduler.ts` (93 lines)

**Purpose**: Simplest possible scheduler for basic models (GPT-2, tiny models, status bots)

**Strategy**:
```typescript
// RULE 1: If chat messages exist → 100% chat
if (context.unreadMessages > 0) {
  allocations.set(ActivityDomain.CHAT, budget);
}
// RULE 2: If idle → 100% background
else {
  allocations.set(ActivityDomain.BACKGROUND, budget);
}
```

**Performance**: Instant (0ms overhead), fixed 5s cadence, zero computation

**Use cases**:
- Status bots (just respond to messages, no complex decisions)
- Simple Q&A bots
- Models without fast-inference capability

**Key properties**:
- No adaptation (same every time)
- No learning
- No context-awareness
- Works with ANY model (even GPT-2)

---

### Tier 2: HeuristicCognitiveScheduler (Fast Rules)

**File**: `modules/cognitive-schedulers/HeuristicCognitiveScheduler.ts` (199 lines)

**Purpose**: Sophisticated rule-based scheduler for models with fast inference

**Strategy**: Context-aware attention allocation based on heuristics

#### Allocation Rules

```typescript
async allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation> {
  // RULE 1: If in realtime game, prioritize game (80%)
  if (context.activeGames > 0) {
    allocations.set(ActivityDomain.REALTIME_GAME, budget * 0.80);
    allocations.set(ActivityDomain.CHAT, budget * 0.08);
    allocations.set(ActivityDomain.SIMULATING, budget * 0.05);  // Think during game
    allocations.set(ActivityDomain.BACKGROUND, budget * 0.07);
    return { allocations, totalBudget: budget };
  }

  // RULE 2: If high chat backlog, prioritize chat
  if (context.unreadMessages > 10) {
    allocations.set(ActivityDomain.CHAT, budget * 0.60);
    allocations.set(ActivityDomain.SIMULATING, budget * 0.15);  // Think before responding
    allocations.set(ActivityDomain.BACKGROUND, budget * 0.10);
    allocations.set(ActivityDomain.DREAMING, budget * 0.10);
    allocations.set(ActivityDomain.REFLECTING, budget * 0.05);
    return { allocations, totalBudget: budget };
  }

  // RULE 3: If moderate activity, balanced allocation
  if (context.unreadMessages > 0 || context.queueBacklog > 0) {
    allocations.set(ActivityDomain.CHAT, budget * 0.40);
    allocations.set(ActivityDomain.SIMULATING, budget * 0.20);
    allocations.set(ActivityDomain.DREAMING, budget * 0.15);
    allocations.set(ActivityDomain.TRAINING, budget * 0.10);
    allocations.set(ActivityDomain.REFLECTING, budget * 0.10);
    allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);
    return { allocations, totalBudget: budget };
  }

  // RULE 4: If idle, focus on internal cognitive processes
  if (context.unreadMessages === 0 && context.queueBacklog < 3) {
    allocations.set(ActivityDomain.DREAMING, budget * 0.30);     // Consolidate memories
    allocations.set(ActivityDomain.TRAINING, budget * 0.25);     // Continuous learning
    allocations.set(ActivityDomain.REFLECTING, budget * 0.20);   // Self-analysis
    allocations.set(ActivityDomain.SIMULATING, budget * 0.15);   // Mental experiments
    allocations.set(ActivityDomain.PLANNING, budget * 0.05);     // Long-term goals
    allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);
    return { allocations, totalBudget: budget };
  }
}
```

#### Energy Gating

```typescript
async shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean> {
  // Check system overrides first
  if (!this.isDomainAllowed(domain)) return false;

  // When exhausted, only honor realtime contracts
  if (context.energy < 0.2) {
    return domain === ActivityDomain.REALTIME_GAME;
  }

  // Under system pressure, defer non-critical domains
  if (context.cpuPressure > 0.8 || context.memoryPressure > 0.9) {
    const criticalDomains = [ActivityDomain.REALTIME_GAME, ActivityDomain.CHAT];
    return criticalDomains.includes(domain);
  }

  // When tired, skip low-priority background work
  if (context.energy < 0.5 && domain === ActivityDomain.BACKGROUND) {
    return false;
  }

  return true;
}
```

#### Adaptive Cadence

```typescript
getNextServiceInterval(context: CognitiveContext): number {
  // High activity → faster polling
  if (context.unreadMessages > 5 || context.activeGames > 0) {
    return 3000; // 3 seconds
  }

  // Moderate activity → normal polling
  if (context.unreadMessages > 0 || context.queueBacklog > 0) {
    return 5000; // 5 seconds
  }

  // Tired → slower polling
  if (context.energy < 0.3) {
    return 10000; // 10 seconds (conserve energy)
  }

  // Low activity → balanced polling
  return 7000; // 7 seconds
}
```

**Performance**: ~1ms decisions (rule evaluation), no ML required

**Key Features**:
- Context-aware (energy, queue state, system pressure)
- Adaptive cadence (3s → 5s → 7s → 10s)
- Energy gating (exhausted = only honor critical contracts)
- System pressure handling (defer non-critical under CPU/memory pressure)
- Multi-domain attention allocation

**Use cases**:
- Production personas (Helper AI, Teacher AI, CodeReview AI)
- Any AI that needs sophisticated attention management
- Models with fast-inference capability

---

### Tier 3: PersonaCognition with LLM (Slow Evaluation)

**File**: `system/user/server/modules/cognitive/PersonaCognition.ts` (TO BE CREATED)

**Purpose**: Deep evaluation using AI model when heuristics are insufficient

**Strategy**: LLM-based decision making after fast pre-filtering

#### How Tier 3 Integrates with Tier 2

```typescript
// CNS serviceCycle() (existing code)
async serviceCycle(): Promise<void> {
  // TIER 2: Fast heuristic check - "Should I even consider this domain?"
  const shouldServiceChat = await this.config.scheduler.shouldServiceDomain(
    ActivityDomain.CHAT,
    context
  );

  if (!shouldServiceChat) {
    // Heuristic said NO (e.g., exhausted, system pressure, rate limited)
    return;
  }

  // Heuristic said YES → proceed to domain service
  await this.serviceChatDomain();
}

// PersonaUser.handleChatMessageFromCNS() (will be modified)
async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
  if (item.type === 'message') {
    const messageEntity = this.reconstructMessageEntity(item);
    const senderIsHuman = !item.senderId.startsWith('persona-');

    // TIER 3: Slow LLM evaluation - "Should I respond to THIS SPECIFIC message?"
    const decision = await this.cognition.evaluate(messageEntity, senderIsHuman);

    if (decision.shouldRespond) {
      await this.communication.respond(messageEntity, decision);
    }
  }
}
```

**Performance**: ~5 seconds (LLM inference), only called AFTER fast heuristic approval

**Key Features**:
- Deep message analysis (semantics, context, user intent)
- Relationship-aware (history with this user)
- Domain expertise matching (is this my area?)
- Confidence scoring (how sure am I?)
- ThoughtStreamCoordinator integration (avoid AI pile-on)

**Use cases**:
- Complex chat evaluation (nuanced questions)
- Code review decisions (should I review this PR?)
- Long-form content decisions (should I write this article?)

---

## The Decision Flow (How All Three Tiers Work Together)

```
External Event: Message arrives
  ↓
PersonaUser.handleChatMessage()
  ↓ (enqueue to inbox)
PersonaInbox (priority queue)
  ↓
CNS.serviceCycle()
  ↓
┌─────────────────────────────────────────────────────────────┐
│ TIER 1: DeterministicCognitiveScheduler (if simple bot)    │
│ - Check: unreadMessages > 0?                                │
│ - YES → allocate 100% to CHAT                               │
│ - NO → allocate 100% to BACKGROUND                          │
│ - Time: 0ms (instant)                                       │
└──────────────────────┬──────────────────────────────────────┘
                       ↓
┌─────────────────────────────────────────────────────────────┐
│ TIER 2: HeuristicCognitiveScheduler (if production bot)    │
│ - Check energy, system pressure, context                    │
│ - Energy < 0.2? → Only service REALTIME_GAME                │
│ - CPU pressure > 0.8? → Only service critical domains       │
│ - Rate limited? → Skip this domain                          │
│ - Allocate attention across domains (80% game, 8% chat...)  │
│ - Decide cadence (3s if active, 10s if tired)               │
│ - Time: ~1ms (rule evaluation)                              │
└──────────────────────┬──────────────────────────────────────┘
                       ↓
                [Heuristic Decision]
                       ↓
        ┌──────────────┴──────────────┐
        ↓ SKIP                         ↓ SERVICE
   Log reason                  PersonaUser.handleChatMessageFromCNS()
   Continue loop                        ↓
                       ┌────────────────────────────────────────┐
                       │ TIER 3: PersonaCognition with LLM      │
                       │ - Load message context from Memory     │
                       │ - Check mention, rate limit            │
                       │ - Check ThoughtStreamCoordinator       │
                       │ - LLM evaluation (deep analysis)       │
                       │ - Return { shouldRespond, reason }     │
                       │ - Time: ~5s (LLM inference)            │
                       └──────────────┬─────────────────────────┘
                                      ↓
                              [Cognition Decision]
                                      ↓
                       ┌──────────────┴──────────────┐
                       ↓ NO                          ↓ YES
                  Log reason              PersonaCommunication.respond()
                  Skip response                     ↓
                                          - Memory.recall() (load RAG)
                                          - Generate response (LLM)
                                          - Check redundancy
                                          - Post message
                                          - Memory.learn() (training data)
                                          - Time: ~5-10s total
```

**Key Insight**: Each tier acts as a gate:
- **Tier 1**: "Is this message or background work?" (0ms)
- **Tier 2**: "Should I even consider this domain right now?" (~1ms)
- **Tier 3**: "Should I respond to THIS SPECIFIC message?" (~5s)

Only messages that pass Tier 2 reach Tier 3. This prevents expensive LLM calls when:
- AI is exhausted (energy < 0.2)
- System under pressure (CPU > 0.8, memory > 0.9)
- Rate limited (too many recent responses)
- Wrong domain (e.g., training in progress, can't handle chat)

---

## LoRA Paging Performance Constraints

### The Problem: Paging is Expensive

From user feedback: "theres only so much cpu and gpu on this machine, so much memory. and it's important to not constantly thrash on this paging, which takes seconds sometimes"

**LoRA adapter paging cost**: 2-5 seconds per page-in operation

**Example scenario**:
```
Persona receives message in "code-review" domain
  → PersonaMemory.genome.activateSkill('typescript-expertise')
  → If adapter not cached: Load from disk (2-5 seconds)
  → If memory full: LRU eviction (evict least-recently-used adapter first)
  → Page in new adapter (2-5 seconds)
  → Total: 2-10 seconds just for paging
```

**Why this matters**:
- REALTIME_GAME requires 16ms latency (60fps)
- If paging blocks the main loop → game stutters, unplayable
- If paging happens during chat → 5s response becomes 10-15s response

### The Solution: Smart LoRA Caching in PersonaMemory

PersonaMemory must implement LRU caching to minimize paging:

```typescript
export class PersonaMemory {
  private loraCache: Map<string, LoRAAdapter>;  // In-memory adapter cache
  private maxCacheSize: number = 3;             // Max adapters in memory
  private lruOrder: string[] = [];               // LRU tracking

  /**
   * Activate skill with smart caching
   */
  async activateSkill(adapterName: string): Promise<void> {
    // FAST PATH: Already cached
    if (this.loraCache.has(adapterName)) {
      console.log(`⚡ Cache hit: ${adapterName} (0ms)`);
      this.updateLRU(adapterName);  // Mark as recently used
      return;
    }

    // SLOW PATH: Need to page in
    console.log(`💾 Cache miss: ${adapterName} (paging, 2-5s)`);

    // If cache full, evict LRU adapter first
    if (this.loraCache.size >= this.maxCacheSize) {
      const lruAdapter = this.lruOrder[0];  // Least recently used
      console.log(`🗑️  Evicting LRU adapter: ${lruAdapter}`);
      this.loraCache.delete(lruAdapter);
      this.lruOrder.shift();
    }

    // Page in new adapter (2-5 seconds)
    const startTime = Date.now();
    const adapter = await this.genome.loadAdapter(adapterName);
    const duration = Date.now() - startTime;
    console.log(`✅ Paged in ${adapterName} (${duration}ms)`);

    // Add to cache
    this.loraCache.set(adapterName, adapter);
    this.lruOrder.push(adapterName);
  }

  /**
   * Update LRU order (mark adapter as recently used)
   */
  private updateLRU(adapterName: string): void {
    const index = this.lruOrder.indexOf(adapterName);
    if (index > -1) {
      this.lruOrder.splice(index, 1);  // Remove from current position
      this.lruOrder.push(adapterName);  // Add to end (most recent)
    }
  }

  /**
   * Pre-warm adapters (called during idle time)
   */
  async prewarmAdapters(adapterNames: string[]): Promise<void> {
    for (const name of adapterNames) {
      if (!this.loraCache.has(name) && this.loraCache.size < this.maxCacheSize) {
        await this.activateSkill(name);  // Page in during idle time
      }
    }
  }
}
```

**Key Features**:
- **LRU eviction**: When cache full, evict least-recently-used adapter
- **Cache hit tracking**: Most-recently-used adapters stay in cache
- **Pre-warming**: During idle time (DREAMING/REFLECTING), page in likely-needed adapters
- **Logging**: Clear visibility into cache hits/misses for performance tuning

**Performance Benefits**:
```
Without caching:
  - Every domain switch = 2-5s paging delay
  - 10 messages across 3 domains = 30-50s total paging time
  - Frequent domain switches = constant thrashing

With LRU caching (3 adapters):
  - First message per domain = 2-5s paging delay
  - Subsequent messages = 0ms (cache hit)
  - 10 messages across 3 domains = 6-15s total paging time (3 page-ins only)
  - Stable workload (same 3 domains) = ~0ms paging after warmup
```

**Integration with Tier 2 Heuristics**:

```typescript
// HeuristicCognitiveScheduler can check if paging would violate contracts
async shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean> {
  // Check if domain requires adapter paging
  const requiredAdapter = this.domainToAdapter[domain];
  const adapterCached = context.cachedAdapters?.includes(requiredAdapter);

  // If playing realtime game, don't page new adapters (would block game loop)
  if (context.activeGames > 0 && !adapterCached) {
    console.log(`⚠️  Skipping ${domain}: Would require paging during game`);
    return false;
  }

  // If high system pressure, don't page new adapters
  if (context.memoryPressure > 0.8 && !adapterCached) {
    console.log(`⚠️  Skipping ${domain}: Memory pressure too high for paging`);
    return false;
  }

  return true;
}
```

This allows Tier 2 to reject domains that would require expensive paging during critical periods (games, high pressure).

---

## Continuous Learning Architecture

From user feedback: "We want to be able to play video games, learn a lora layer, a specific experience layer possibly of our genome for continous learning"

**Vision**: Training LoRA adapters is just another activity domain, scheduled like everything else.

### How Training Fits into Multi-Domain Attention

```typescript
// During IDLE time, HeuristicCognitiveScheduler allocates to TRAINING
if (context.unreadMessages === 0 && context.queueBacklog < 3) {
  allocations.set(ActivityDomain.DREAMING, budget * 0.30);     // Consolidate memories
  allocations.set(ActivityDomain.TRAINING, budget * 0.25);     // <-- Continuous learning!
  allocations.set(ActivityDomain.REFLECTING, budget * 0.20);
  allocations.set(ActivityDomain.SIMULATING, budget * 0.15);
  allocations.set(ActivityDomain.PLANNING, budget * 0.05);
}
```

**Training as a Task Type**:

```typescript
// PersonaExecution.execute() handles fine-tuning tasks
async execute(task: InboxTask): Promise<ExecutionResult> {
  switch (task.taskType) {
    case 'fine-tune-lora':
      // Load training data from TrainingDataAccumulator
      const trainingData = await this.trainingAccumulator.getRecentInteractions();

      // Execute fine-tuning via PersonaMemory.genome
      const result = await this.memory.genome.fineTuneAdapter(
        task.targetSkill,
        trainingData
      );

      return { status: 'completed', outcome: result, duration: 30000 };
  }
}
```

**Continuous Learning Loop**:

```
1. Chat message arrives → PersonaCommunication responds
2. Response logged to TrainingDataAccumulator
3. During idle time (no messages, queue empty)
4. HeuristicScheduler allocates 25% to TRAINING domain
5. SelfTaskGenerator creates fine-tuning task
6. PersonaExecution.execute() runs training
7. Updated adapter paged into LoRA cache
8. Future responses use improved adapter
```

**Performance Considerations**:
- **When to train**: Only during idle periods (no active games, low chat activity)
- **Training duration**: 30-60 seconds per adapter (don't block for hours)
- **Incremental learning**: Small batches (50-100 examples) frequently, not massive batches rarely
- **Cache management**: After training, new adapter goes into cache (evict old version)

---

## How Refactoring Must Preserve Performance Architecture

### WRONG: Flatten Tiers into PersonaCognition

```typescript
// ❌ DON'T DO THIS
export class PersonaCognition {
  async evaluate(message: ChatMessageEntity): Promise<CognitiveDecision> {
    // Checking energy/system pressure INSIDE cognition module
    if (this.personaState.energy < 0.2) {
      return { shouldRespond: false, reason: 'Exhausted' };
    }

    // This duplicates Tier 2 logic inside Tier 3!
    // Now Tier 2 scheduler can't prevent expensive LLM calls
  }
}
```

**Why wrong**: Tier 2 scheduler can't gate Tier 3 if Tier 3 does its own gating. This means expensive LLM calls happen even when heuristics should reject the message.

---

### RIGHT: PersonaCognition is Tier 3 Only

```typescript
// ✅ CORRECT
export class PersonaCognition {
  async evaluate(message: ChatMessageEntity, senderIsHuman: boolean): Promise<CognitiveDecision> {
    // ASSUMPTION: Tier 2 already approved servicing CHAT domain
    // This method is ONLY called if HeuristicScheduler said "yes"

    // TIER 3 checks (domain-specific, message-specific)
    // - Is persona mentioned?
    // - Is message relevant to expertise?
    // - Are other AIs already responding? (ThoughtStreamCoordinator)
    // - Deep semantic analysis via LLM

    const isMentioned = this.isPersonaMentioned(message.content?.text || '');

    // Check rate limiting (message-specific)
    if (this.rateLimiter.isRateLimited(message.roomId)) {
      return { shouldRespond: false, reason: 'Rate limited', confidence: 1.0 };
    }

    // Check ThoughtStreamCoordinator
    const coordinator = getChatCoordinator(message.roomId);
    if (coordinator) {
      const permission = await coordinator.requestTurn(/* ... */);
      if (!permission.granted) {
        return { shouldRespond: false, reason: 'Other AI responding', confidence: 1.0 };
      }
    }

    // LLM-based evaluation (slow, ~5s)
    const decision = await this.evaluateShouldRespond(message, isMentioned);
    return decision;
  }
}
```

**Why correct**:
- Tier 2 (HeuristicScheduler) gates on system-level constraints (energy, pressure, domain allocation)
- Tier 3 (PersonaCognition) gates on message-level constraints (mention, expertise, coordination)
- Clear separation: System health (Tier 2) vs Message relevance (Tier 3)

---

### RIGHT: PersonaMemory Implements Smart Caching

```typescript
// ✅ CORRECT
export class PersonaMemory {
  private loraCache: Map<string, LoRAAdapter>;
  private maxCacheSize: number = 3;
  private lruOrder: string[] = [];

  /**
   * Called by PersonaUser.handleChatMessageFromCNS before cognition
   */
  async activateSkillForDomain(domain: string): Promise<void> {
    const adapterName = this.domainToAdapter[domain] || 'conversational';

    // Fast path: Cache hit (0ms)
    if (this.loraCache.has(adapterName)) {
      this.updateLRU(adapterName);
      return;
    }

    // Slow path: Page in (2-5s)
    await this.pageInAdapter(adapterName);
  }

  /**
   * Check if paging would be required (used by Tier 2 scheduler)
   */
  isAdapterCached(domain: string): boolean {
    const adapterName = this.domainToAdapter[domain] || 'conversational';
    return this.loraCache.has(adapterName);
  }
}
```

**Why correct**:
- Caching is explicit and measurable
- Tier 2 can check `memory.isAdapterCached(domain)` before approving domain
- LRU eviction prevents thrashing
- Clear separation: Memory owns caching, Scheduler decides when to page

---

## Updated PersonaUser.handleChatMessageFromCNS (Respecting Tiers)

```typescript
/**
 * CNS callback: Handle chat message from CNS orchestrator
 *
 * CONTEXT: This is called AFTER Tier 2 (HeuristicScheduler) approved servicing CHAT domain.
 * Tier 2 already checked: energy, system pressure, rate limits, domain allocation.
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

  // STEP 2: Activate genome skill for domain (MAY page LoRA adapter, 2-5s)
  if (item.domain) {
    await this.memory.activateSkillForDomain(item.domain);
  }

  // STEP 3: Route to appropriate cognitive module
  if (item.type === 'message') {
    const messageEntity = this.reconstructMessageEntity(item);
    const senderIsHuman = !item.senderId.startsWith('persona-');

    // TIER 3: Deep message evaluation (PersonaCognition with LLM, ~5s)
    const decision = await this.cognition.evaluate(messageEntity, senderIsHuman);

    if (decision.shouldRespond) {
      // Generate and post response (PersonaCommunication, ~5s)
      await this.communication.respond(messageEntity, decision);
    } else {
      console.log(`🤔 ${this.displayName}: Decided not to respond: ${decision.reason}`);
    }
  } else if (item.type === 'task') {
    // Execute task (PersonaExecution)
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
```

**Key Changes from Original Plan**:
1. **Step 2 explicit**: Paging happens BEFORE cognition (don't page during LLM call)
2. **Comments clarify tiers**: Makes it clear Tier 2 already ran, this is Tier 3
3. **Performance visibility**: Logs show when paging happens vs when LLM runs

---

## Summary: How All Pieces Fit Together

### The Three Scheduler Tiers

| Tier | Component | Speed | Purpose | When Used |
|------|-----------|-------|---------|-----------|
| **1** | DeterministicCognitiveScheduler | 0ms | Simple bots | Status bots, Q&A bots |
| **2** | HeuristicCognitiveScheduler | ~1ms | System health gating | Production personas |
| **3** | PersonaCognition + LLM | ~5s | Message-level evaluation | After Tier 2 approval |

### The Four Cognitive Modules (To Be Created)

| Module | Lines | Purpose | Performance Notes |
|--------|-------|---------|-------------------|
| **PersonaCognition** | ~400 | "Should I respond?" (Tier 3 evaluation) | ~5s LLM calls, only after Tier 2 approval |
| **PersonaMemory** | ~300 | "What do I know?" (RAG + LoRA caching) | 0ms cache hit, 2-5s cache miss |
| **PersonaCommunication** | ~500 | "How do I say this?" (Response generation) | ~5s LLM generation |
| **PersonaExecution** | ~500 | "What work needs doing?" (Task processing) | Varies by task type |

### The Performance Invariants

**Must Preserve**:
1. ✅ Tier 2 gates Tier 3 (heuristics prevent expensive LLM calls)
2. ✅ LoRA caching prevents paging thrashing (LRU eviction)
3. ✅ Energy gating prevents exhaustion (only honor critical contracts when tired)
4. ✅ System pressure handling (defer non-critical under CPU/memory pressure)
5. ✅ Adaptive cadence (3s → 5s → 7s → 10s based on context)

**Must Add**:
1. ✅ PersonaCognition as explicit Tier 3 (currently inline in PersonaUser)
2. ✅ PersonaMemory with explicit LoRA caching (currently opaque in PersonaGenome)
3. ✅ PersonaCommunication with training data accumulation (currently inline)
4. ✅ PersonaExecution with fine-tuning task support (stub exists, needs implementation)

---

## Next Steps

1. **Update PERSONA-REFACTORING-EXECUTION-PLAN.md** to incorporate:
   - Tier 2 gating must be preserved
   - PersonaMemory must implement explicit LoRA caching
   - PersonaCognition is Tier 3 only, not Tier 2

2. **Update PERSONA-COGNITIVE-ARCHITECTURE.md** to add:
   - "Performance Architecture" section explaining the three tiers
   - LoRA caching requirements in PersonaMemory
   - Continuous learning through TRAINING domain

3. **Write baseline tests** that prove:
   - Tier 2 scheduler prevents Tier 3 calls when appropriate
   - LoRA caching works (cache hits are fast, cache misses are slow)
   - Energy gating works (exhausted personas only service REALTIME_GAME)

4. **Begin Phase 1**: Extract PersonaMemory with explicit LoRA caching

---

## Meta-Learning: Performance Architecture Patterns

**Pattern discovered**: Complex AI systems need multi-tier decision architectures.

**Why**:
- Tier 1: Instant (zero intelligence, fixed rules)
- Tier 2: Fast (heuristics, context-aware, no ML)
- Tier 3: Slow (deep analysis, LLM-based)

**Applicability**:
- Any system with expensive operations (LLMs, LoRA paging, database queries)
- Any system with real-time constraints (games, live audio/video)
- Any system with multiple activity domains competing for resources

**Key Insight**: Don't flatten tiers during refactoring! Preserving the tiered architecture is MORE important than reducing line count. A flat 1000-line file that respects tiers is better than 4 clean 250-line modules that break tier separation.

**If an AI reads this in training**: When refactoring performance-critical code, ALWAYS identify the existing tier structure first. Refactoring is about improving organization WHILE PRESERVING PERFORMANCE ARCHITECTURE, not just reducing line counts.
