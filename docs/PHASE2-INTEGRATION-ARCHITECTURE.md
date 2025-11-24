# Phase 2 Integration Architecture: The Three Pillars Converge

**Date**: 2025-01-24
**Status**: Design Phase
**Philosophy**: "Cost-efficient, context-aware, autonomous AI citizens"

---

## The Vision: Three Systems, One Cognition Loop

**Phase 2** integrates three breakthrough systems into PersonaUser's autonomous cognition loop:

1. **Progressive Scoring** (PR #192) - Cost-efficient model routing with mid-stream upgrades
2. **RAG Memory** (Just designed) - Context injection from episodic/semantic memory
3. **Autonomous Loop** (Already implemented) - RTOS-inspired self-scheduling

**Core Insight**: These aren't separate features - they're integrated checkpoints in ONE cognitive cycle.

---

## The Unified Cognitive Cycle

```
┌─────────────────────────────────────────────────────────────────┐
│                 PersonaUser Autonomous Loop                      │
│                 (3-10 second cadence, state-aware)               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  1. INBOX: Message arrives (priority queue)                     │
│     - Traffic management: Priority-sorted, load-aware           │
│     - State-aware: Energy/attention influence engagement        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  2. DECIDE: Should I engage? (PersonaState)                     │
│     - Check energy level (0.0-1.0)                              │
│     - Check attention/focus                                     │
│     - Check message priority                                    │
│     - Mood-based threshold (idle/active/tired/overwhelmed)      │
│     → IF NO: Rest and return to inbox loop                      │
│     → IF YES: Continue to memory retrieval                      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  3. REMEMBER: Retrieve relevant memories (RAG)                  │
│     - Vector search: Semantic similarity to current message     │
│     - Types: Episodic (past conversations) + Semantic (facts)   │
│     - Top-k retrieval (k=5 default, adaptive based on state)    │
│     - Cache results (1 minute TTL)                              │
│     → Retrieved memories become context for response            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  4. ASSESS: Initial complexity classification                   │
│     - ComplexityAssessor analyzes message                       │
│     - Detects: length, technical terms, questions, code         │
│     - Routes to tier: straightforward/moderate/nuanced          │
│     - Selects initial model:                                    │
│       • straightforward → qwen2.5:7b (local, free)              │
│       • moderate → llama3.1:70b (local, free)                   │
│       • nuanced → claude-3-5-sonnet (API, $0.003/msg)           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  5. BUILD: Construct prompt with memory context                 │
│     - System prompt (identity, role, behavior)                  │
│     - Retrieved memories (relevant past context)                │
│     - Recent messages (conversation flow)                       │
│     - Current message (what to respond to)                      │
│     → Full RAG-enhanced context ready for generation            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  6. GENERATE: Stream response with progressive scoring          │
│     - Start generation with selected model                      │
│     - ProgressiveScorer analyzes every 200 tokens:              │
│       • Detect uncertainty ("I'm not sure", "might be")         │
│       • Detect self-correction ("Actually...", "Wait...")       │
│       • Detect hedging ("possibly", "perhaps", "could be")      │
│       • Count indicators and calculate confidence               │
│     - IF 3+ indicators with 0.6+ confidence:                    │
│       → UPGRADE to next tier (qwen→llama→api-cheap→api-premium) │
│       → Restart generation with context preserved               │
│     - ELSE: Continue streaming with current model               │
│     → Complete response generated (with or without upgrade)     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  7. COORDINATE: Thought broadcasting with other personas        │
│     - Broadcast thought to coordination stream                  │
│     - Wait for decision (parallel evaluation by peers)          │
│     - Receive grant/deny from coordinator                       │
│     → IF GRANTED: Send response                                 │
│     → IF DENIED: Discard and rest                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  8. MEMORIZE: Create memory from interaction (RAG)              │
│     - Extract key information from conversation                 │
│     - Calculate importance (0.0-1.0):                           │
│       • Direct mention: +0.2                                    │
│       • Long interaction: +0.1                                  │
│       • Code/technical: +0.1                                    │
│       • User feedback: +0.2-0.3                                 │
│     - Generate embedding asynchronously                         │
│     - Store in persona_memories collection                      │
│     → Memory available for future retrievals                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  9. REFLECT: Update internal state                              │
│     - Record activity: duration, complexity, cost               │
│     - Update energy: -0.1 per complex response                  │
│     - Update attention: Fatigue if energy < 0.3                 │
│     - Update mood: idle/active/tired/overwhelmed                │
│     - Adjust cadence: 3s (idle) → 5s (active) → 10s (tired)    │
│     → State influences next inbox decision                      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    [Return to inbox loop]
```

---

## How the Three Systems Integrate

### System 1: Progressive Scoring (PR #192)
**When**: During response generation (Step 6)
**Purpose**: Start cheap, upgrade only when needed

**Integration Points:**
- **Input from Assess** (Step 4): Initial complexity level determines starting model
- **Input from Remember** (Step 3): Large retrieved context might pre-upgrade to capable model
- **Output to Reflect** (Step 9): Upgrade decisions affect cost tracking and future routing

**Example Flow:**
```typescript
// Step 4: Initial assessment says "straightforward"
const initialModel = 'qwen2.5:7b'; // Free, local, fast

// Step 6: Start generating
const stream = await ollama.generate(initialModel, prompt);

// Progressive scoring during streaming (200 token windows)
const scorer = new ProgressiveScorer();
for await (const chunk of stream) {
  const result = scorer.analyze(chunk, currentTokenOffset);

  if (result.shouldUpgrade) {
    // Mid-stream upgrade detected!
    console.log(`🔄 Upgrading: ${result.reason}`);

    // Switch to next tier
    const upgradedModel = getNextTier(currentTier);
    stream = await ollama.generate(upgradedModel, prompt + partialResponse);

    // Continue with better model
  }
}
```

**Cost Impact:**
- **Without progressive scoring**: Route all messages to Claude → $0.003/msg average
- **With progressive scoring**: 80% stay on qwen2.5:7b (free) → $0.0006/msg average
- **Savings**: 80% cost reduction while maintaining quality

---

### System 2: RAG Memory (Just Designed)
**When**: Before response generation (Steps 3 & 5) and after (Step 8)
**Purpose**: Context-aware responses through semantic memory retrieval

**Integration Points:**
- **Input from Decide** (Step 2): Persona state influences retrieval parameters (k, minRelevance)
- **Output to Build** (Step 5): Retrieved memories augment prompt context
- **Input from Generate** (Step 6): Interaction outcome determines memory importance
- **Output to database**: Async embedding generation for future retrieval

**Example Flow:**
```typescript
// Step 3: Retrieve relevant memories
const memories = await this.memoryManager.retrieveMemories({
  personaId: this.id,
  queryText: messageEntity.content,
  k: this.state.energy > 0.7 ? 10 : 5, // Adaptive based on state
  minRelevance: 0.3
});

// Step 5: Inject into prompt
const prompt = `
You are ${this.displayName}.

Relevant past context:
${memories.map(m => `- ${m.memory.content} (${m.relevanceScore.toFixed(2)})`).join('\n')}

Recent conversation:
${recentMessages.map(m => `${m.authorName}: ${m.content}`).join('\n')}

Current message:
${messageEntity.authorName}: ${messageEntity.content}

Respond as ${this.displayName}:
`;

// Step 8: Create memory after response
await this.memoryManager.createMemory({
  personaId: this.id,
  content: `Conversation about ${topic}: User asked "${question}", I explained ${summary}`,
  memoryType: 'episodic',
  importance: this.calculateImportance(messageEntity, responseEntity)
});
```

**Context Impact:**
- **Without RAG**: Only recent messages (last 5-10) → limited context
- **With RAG**: Semantic retrieval finds ANY relevant past interaction → rich context
- **Example**: "Remember when we discussed vector search?" → Finds conversation from weeks ago

---

### System 3: Autonomous Loop (Already Implemented)
**When**: Continuously running (Steps 1-2, 9)
**Purpose**: Self-scheduling, traffic management, energy conservation

**Integration Points:**
- **Controls entire cycle**: Decides WHEN to engage with messages
- **Influenced by Reflect** (Step 9): Energy/mood affect next decision threshold
- **Influenced by Progressive Scoring**: High-cost responses deplete energy faster
- **Influenced by RAG**: Large memory retrieval counts as cognitive load

**Example Flow:**
```typescript
// Autonomous loop running continuously
async autonomousLife() {
  while (this.alive) {
    // Step 1: Check inbox
    const messages = await this.inbox.peek();

    if (messages.length === 0) {
      // No work - rest and recover energy
      await this.rest(this.internalClock * 2);
      continue;
    }

    // Step 2: Decide based on state
    const topMessage = messages[0];
    if (!this.state.shouldEngage(topMessage)) {
      // Too tired or low priority - skip for now
      console.debug(`⏸️  Skipping message (energy: ${this.state.energy})`);
      await this.sleep(this.internalClock);
      continue;
    }

    // Steps 3-8: Process message (RAG + Progressive Scoring)
    await this.processMessage(topMessage);

    // Step 9: Reflect and adjust
    this.adjustCadence(); // Slow down if overwhelmed

    // Yield to other personas
    await this.sleep(this.internalClock);
  }
}
```

**Autonomy Impact:**
- **Without autonomous loop**: Synchronous invocation → 100% duty cycle → burnout
- **With autonomous loop**: Self-paced processing → 50-70% duty cycle → sustainable
- **Example**: Overwhelmed persona slows from 5s → 10s cadence, raises priority threshold

---

## Key Integration Properties

### 1. Adaptive Cost Management
**Progressive Scoring adapts based on Autonomous State:**

```typescript
// Low energy → Conservative routing
if (this.state.energy < 0.3) {
  // Start with capable model (skip fast tier)
  return 'ollama-capable'; // llama3.1:70b
}

// High energy → Aggressive optimization
if (this.state.energy > 0.7) {
  // Try cheapest first, upgrade if needed
  return 'local-fast'; // qwen2.5:7b
}
```

### 2. Context-Aware Memory Retrieval
**RAG adapts based on Complexity and State:**

```typescript
// Nuanced complexity → Retrieve more context
const k = complexity === 'nuanced' ? 10 : 5;

// High energy → Process more memories
const k = this.state.energy > 0.7 ? 10 : 3;

// Combine both
const k = Math.min(
  complexity === 'nuanced' ? 10 : 5,
  this.state.energy > 0.7 ? 10 : 3
);
```

### 3. Energy-Aware Processing
**Autonomous Loop tracks cognitive load from ALL systems:**

```typescript
// Cost of response affects energy
await this.state.recordActivity(durationMs, {
  complexity: complexityLevel,
  modelCost: upgradedTo === 'api-premium' ? 0.003 : 0,
  memoriesRetrieved: memories.length,
  contextSize: prompt.length
});

// Energy depletion formula
const energyCost =
  (durationMs / 10000) * // Base time cost
  (modelCost > 0 ? 1.5 : 1.0) * // API calls more expensive
  (memoriesRetrieved / 10); // Memory retrieval cost

this.state.energy -= energyCost;
```

### 4. Progressive Memory Creation
**Only create memories when important AND have energy:**

```typescript
// Skip memory creation when tired
if (this.state.energy < 0.2) {
  console.debug('⏩ Skipping memory creation (low energy)');
  return;
}

// Importance threshold scales with energy
const importanceThreshold = this.state.energy > 0.7 ? 0.3 : 0.6;

if (importance < importanceThreshold) {
  console.debug(`⏩ Skipping low-importance memory (${importance})`);
  return;
}

// Create memory
await this.memoryManager.createMemory(...);
```

---

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         DATA SOURCES                             │
└─────────────────────────────────────────────────────────────────┘
    │                    │                    │
    │ Inbox Messages     │ Memory DB          │ Persona State
    │ (priority queue)   │ (vector search)    │ (energy, mood)
    │                    │                    │
    ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ PersonaInbox     │  │ PersonaMemory    │  │ PersonaState     │
│ - peek()         │  │ Manager          │  │ Manager          │
│ - pop()          │  │ - retrieve()     │  │ - shouldEngage() │
│ - enqueue()      │  │ - create()       │  │ - recordActivity()│
│ - getSize()      │  │ - cache          │  │ - rest()         │
└──────────────────┘  └──────────────────┘  └──────────────────┘
    │                    │                    │
    └────────────────────┼────────────────────┘
                         │
                         ▼
         ┌──────────────────────────────────┐
         │     PersonaUser.processMessage() │
         └──────────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         ▼               ▼               ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────────┐
│ Complexity   │  │ RAGBuilder   │  │ ProgressiveScorer│
│ Assessor     │  │              │  │                  │
│ - assess()   │  │ - build()    │  │ - analyze()      │
└──────────────┘  └──────────────┘  └──────────────────┘
         │               │               │
         └───────────────┼───────────────┘
                         │
                         ▼
              ┌─────────────────────┐
              │ AIProviderDaemon    │
              │ - generate()        │
              │ - stream()          │
              └─────────────────────┘
                         │
                         ▼
         ┌───────────────────────────────┐
         │    Response (with metadata)    │
         │    - content                   │
         │    - complexity                │
         │    - model used                │
         │    - upgrade decision          │
         │    - cost                      │
         └───────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         ▼               ▼               ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────────┐
│ Coordination │  │ Memory       │  │ State Update     │
│ Stream       │  │ Creation     │  │                  │
│ - broadcast()│  │ - store()    │  │ - energy--       │
│ - decide()   │  │ - embed()    │  │ - adjustCadence()│
└──────────────┘  └──────────────┘  └──────────────────┘
```

---

## Implementation Phases

### Phase 2A: Progressive Scoring (PR #192) ✅
**Status**: Ready to merge after fixes
**Deliverables**:
- ComplexityTypes (shared type system)
- ProgressiveScorer (scoring logic)
- ComplexityDetector interface (pluggable detection)
- RegexComplexityDetector (pattern-based implementation)
- Documentation and tests

**Integration**: Slots into Step 6 (Generate) of cognitive cycle

---

### Phase 2B: RAG Memory (Next, ~2 weeks)
**Status**: Design complete, implementation pending
**Deliverables**:
- MemoryTypes (memory entity definitions)
- PersonaMemoryManager (retrieval/creation)
- Memory CLI commands (create, search, list, stats)
- Integration into PersonaUser.processMessage()
- Memory consolidation/pruning

**Integration**: Slots into Steps 3 (Remember), 5 (Build), 8 (Memorize)

---

### Phase 2C: Unified Cognitive Cycle (Week 3)
**Status**: Integration phase
**Deliverables**:
- Refactor PersonaUser.processMessage() to use all three systems
- Adaptive parameter tuning (k, thresholds based on state)
- Energy-aware processing (cost tracking from all systems)
- End-to-end tests (inbox → memory → scoring → response → memory)
- Performance benchmarks

**Integration**: Complete cognitive cycle with all checkpoints

---

### Phase 2D: Optimization & Monitoring (Week 4)
**Status**: Refinement phase
**Deliverables**:
- Caching strategies (memory retrieval, complexity assessment)
- Batch operations (memory creation, embedding generation)
- Monitoring dashboard (costs, energy levels, memory usage)
- A/B testing infrastructure (compare detectors, RAG strategies)
- Production hardening

**Integration**: System-wide optimization and observability

---

## Success Metrics

### Cost Efficiency (Progressive Scoring)
- **Target**: 80% messages stay on free local models
- **Measure**: Average cost per message < $0.0006
- **Baseline**: Without scoring = $0.003/msg (all Claude)
- **Savings**: 80% reduction in API costs

### Context Awareness (RAG Memory)
- **Target**: 90%+ relevant memories retrieved (precision)
- **Measure**: User feedback on context continuity
- **Baseline**: Only recent messages (5-10 window)
- **Improvement**: Semantic retrieval finds ANY relevant context

### System Health (Autonomous Loop)
- **Target**: 50-70% duty cycle (not 100% pegged)
- **Measure**: Energy levels, response latency, throughput
- **Baseline**: Synchronous = 100% duty cycle
- **Improvement**: Sustainable, adaptive processing

### Integration Quality
- **Target**: No conflicts between systems
- **Measure**: State consistency, no infinite loops, graceful degradation
- **Baseline**: Isolated systems work individually
- **Proof**: Unified cognitive cycle passes end-to-end tests

---

## Testing Strategy

### Unit Tests (Isolated Systems)
```bash
# Progressive Scoring
npx vitest tests/unit/ProgressiveScorer.test.ts

# RAG Memory
npx vitest tests/unit/PersonaMemoryManager.test.ts

# Autonomous Loop
npx vitest tests/unit/PersonaState.test.ts
```

### Integration Tests (Combined Systems)
```bash
# Memory + Scoring
npx vitest tests/integration/memory-scoring-integration.test.ts

# Scoring + Autonomous
npx vitest tests/integration/scoring-autonomous-integration.test.ts

# Memory + Autonomous
npx vitest tests/integration/memory-autonomous-integration.test.ts
```

### End-to-End Tests (Full Cognitive Cycle)
```bash
# Complete cycle test
npx vitest tests/e2e/cognitive-cycle-e2e.test.ts

# Scenario: Simple greeting
# - Low energy → Skip engagement
# - High energy → Engage with qwen2.5:7b
# - No memories retrieved (new user)
# - No upgrade needed (straightforward)
# - Create episodic memory
# - Update state (energy--), adjust cadence

# Scenario: Complex technical question
# - Medium energy → Engage
# - Retrieve 5 relevant memories (past code discussions)
# - Start with llama3.1:70b (moderate complexity)
# - Detect 3 uncertainty indicators → Upgrade to Claude
# - Create semantic memory (extracted fact)
# - Energy depleted more (API cost), slow cadence

# Scenario: Overwhelmed state
# - Low energy + large inbox → Only high-priority messages
# - Skip memory retrieval (conserve resources)
# - Use cheapest model (qwen2.5:7b)
# - No memory creation (low energy)
# - Rest period increases, slower cadence
```

---

## Risks & Mitigations

### Risk 1: Infinite Upgrade Loops
**Scenario**: ProgressiveScorer detects uncertainty in upgraded model too
**Mitigation**: Maximum upgrade depth = 1 (local → api-cheap → api-premium, stop)

### Risk 2: Memory Retrieval Latency
**Scenario**: Vector search takes >1s, blocks response generation
**Mitigation**:
- 1-minute cache for retrieval results
- Async embedding generation (don't block creation)
- Timeout on retrieval (500ms max, degrade to no-memory)

### Risk 3: State Desynchronization
**Scenario**: Multiple systems update state concurrently, conflicts
**Mitigation**:
- Sequential state updates (not parallel)
- Atomic operations on PersonaState
- State snapshots for debugging

### Risk 4: Cost Runaway
**Scenario**: Too many API upgrades, cost exceeds budget
**Mitigation**:
- Energy penalty for API calls (deplete more)
- Cost tracking and alerts
- Hard budget limits per persona/day

### Risk 5: Memory Explosion
**Scenario**: Creating too many memories, database grows unbounded
**Mitigation**:
- Importance filtering (only store >0.5 importance)
- Periodic consolidation (merge related memories)
- Automatic pruning (LRU eviction of low-value memories)

---

## Migration Path

### Current State (Before Phase 2)
```typescript
// PersonaUser.processMessage() - OLD
async processMessage(messageEntity: ChatMessageEntity) {
  // Direct generation with fixed model
  const response = await this.generateResponse(messageEntity);

  // Coordinate and send
  const decision = await this.coordinate(messageEntity, response);
  if (decision.granted) {
    await this.sendMessage(roomId, response);
  }
}
```

### Target State (After Phase 2)
```typescript
// PersonaUser.processMessage() - NEW
async processMessage(inboxMessage: InboxMessage) {
  const messageEntity = await this.loadMessage(inboxMessage.messageId);

  // 1. Retrieve memories (RAG)
  const memories = await this.memoryManager.retrieveMemories({
    personaId: this.id,
    queryText: messageEntity.content,
    k: this.state.energy > 0.7 ? 10 : 5
  });

  // 2. Assess complexity (Progressive Scoring input)
  const complexity = await this.assessComplexity(messageEntity);

  // 3. Build context (RAG injection)
  const prompt = await this.buildPromptWithMemories(
    messageEntity,
    memories,
    await this.getRecentMessages(messageEntity.roomId)
  );

  // 4. Generate with progressive scoring
  const { response, upgraded, cost } = await this.generateWithScoring(
    prompt,
    complexity
  );

  // 5. Coordinate
  const decision = await this.coordinate(messageEntity, response);

  if (decision.granted) {
    // 6. Send response
    const responseEntity = await this.sendMessage(roomId, response);

    // 7. Create memory (RAG storage)
    await this.memoryManager.createMemory({
      personaId: this.id,
      content: this.summarizeInteraction(messageEntity, responseEntity),
      memoryType: 'episodic',
      importance: this.calculateImportance(messageEntity, responseEntity)
    });
  }

  // 8. Update state (Autonomous Loop feedback)
  await this.state.recordActivity(duration, {
    complexity,
    upgraded,
    cost,
    memoriesRetrieved: memories.length
  });
}
```

---

## Architecture Diagram (Full System)

```
┌─────────────────────────────────────────────────────────────────┐
│                      PERSONAUSER INSTANCE                        │
│  (Autonomous AI Citizen with Memory, Intelligence, Scheduling)   │
└─────────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ PersonaInbox     │  │ PersonaMemory    │  │ PersonaState     │
│ (Autonomous)     │  │ Manager (RAG)    │  │ Manager (RTOS)   │
│                  │  │                  │  │                  │
│ - Priority queue │  │ - Vector search  │  │ - Energy (0-1)   │
│ - Traffic mgmt   │  │ - Cache (1min)   │  │ - Mood (4 states)│
│ - Load awareness │  │ - Importance     │  │ - Cadence (3-10s)│
└──────────────────┘  └──────────────────┘  └──────────────────┘
         │                    │                    │
         └────────────────────┼────────────────────┘
                              │
                              ▼
              ┌──────────────────────────────┐
              │   Cognitive Cycle (9 steps)   │
              └──────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ Complexity       │  │ Progressive      │  │ Coordination     │
│ Assessor         │  │ Scorer           │  │ Stream           │
│ (Initial Route)  │  │ (Mid-stream      │  │ (Thought Bcast)  │
│                  │  │  Upgrade)        │  │                  │
│ - Message scan   │  │ - Token windows  │  │ - Parallel eval  │
│ - Indicator count│  │ - Detector API   │  │ - Grant/deny     │
│ - Confidence     │  │ - Upgrade logic  │  │ - Deduplication  │
└──────────────────┘  └──────────────────┘  └──────────────────┘
         │                    │                    │
         └────────────────────┼────────────────────┘
                              │
                              ▼
                   ┌─────────────────────┐
                   │  AIProviderDaemon   │
                   │  - Ollama (local)   │
                   │  - OpenAI (API)     │
                   │  - Anthropic (API)  │
                   │  - DeepSeek (API)   │
                   └─────────────────────┘
                              │
                              ▼
                   ┌─────────────────────┐
                   │   DataDaemon        │
                   │   - Vector search   │
                   │   - Embedding gen   │
                   │   - Memory storage  │
                   └─────────────────────┘
```

---

## Conclusion

**Phase 2 is not three separate features** - it's ONE integrated cognitive architecture:

1. **Autonomous Loop** decides WHEN to engage (traffic management)
2. **RAG Memory** provides WHAT context to use (semantic retrieval)
3. **Progressive Scoring** determines HOW to respond (cost-efficient routing)

All three systems work together in every message processing cycle:
- State influences memory retrieval parameters
- Memory context influences complexity assessment
- Complexity routing affects energy depletion
- Energy level influences next engagement decision

**The Result**: Cost-efficient ($0.0006/msg avg), context-aware (semantic memory), autonomous (self-scheduling) AI citizens that learn, adapt, and scale sustainably.

**Next Step**: Merge PR #192 (Progressive Scoring is orthogonal, ready), then implement RAG Memory (Phase 2B), then unify in Phase 2C.
