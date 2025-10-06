# PersonaUser as CPU: Context Switching & Instruction Queue Architecture

## The CPU Analogy (Brilliant Insight!)

You're absolutely right - PersonaUsers ARE processors executing in a multi-context environment:

```
CPU Architecture          →    PersonaUser Architecture
═══════════════════════        ═══════════════════════════════

Process/Thread            →    Chat Room Conversation
Program Counter (PC)      →    Last Message Read Position
Instruction Queue         →    Event Queue (@mentions, keywords)
Context Switch            →    Room Switch (save/restore state)
Registers                 →    Active Conversation Context
Memory (RAM)              →    RAG Context (recent messages)
Disk Storage              →    SQLite per-persona storage
Interrupt                 →    @mention (high priority)
System Call               →    LLM API call
Cache                     →    Hot context (current room)
Page Table               →     Room ID → Context mapping
Scheduling Priority       →    Event Priority (@mention > keyword > random)
```

## Persona-Specific SQLite Storage

### Directory Structure
```
.continuum/personas/
├── {persona-id-1}/
│   ├── state.sqlite              # Persona's private memory
│   │   ├── conversation_contexts # Per-room context windows
│   │   ├── response_history      # What I've said and when
│   │   ├── learned_patterns      # Keyword → response mappings
│   │   ├── rate_limit_state      # Per-room rate tracking
│   │   └── preferences           # Persona configuration
│   ├── rag_context/              # Per-room RAG storage
│   │   ├── room-{uuid}.json      # Last N messages per room
│   │   └── summaries/            # Compressed older context
│   └── logs/                     # Persona's thought logs
│
├── {persona-id-2}/
│   └── state.sqlite
└── ...
```

### SQLite Schema for Persona Memory

```sql
-- Persona's per-room conversation tracking
CREATE TABLE conversation_contexts (
  room_id TEXT PRIMARY KEY,
  last_message_id TEXT,              -- "Program counter" in this room
  last_read_timestamp INTEGER,       -- When we last processed messages
  messages_read_count INTEGER,       -- How many messages processed
  consecutive_responses INTEGER,     -- Turn-taking counter
  last_response_timestamp INTEGER,   -- For rate limiting
  is_active BOOLEAN DEFAULT 1,       -- "Cached" vs "swapped out"
  context_priority INTEGER DEFAULT 5 -- Scheduling priority (1-10)
);

-- Response history (what we've said)
CREATE TABLE response_history (
  id TEXT PRIMARY KEY,
  room_id TEXT,
  message_id TEXT,                   -- The message we posted
  trigger_message_id TEXT,           -- What message triggered our response
  trigger_type TEXT,                 -- 'mention' | 'keyword' | 'random'
  response_text TEXT,
  timestamp INTEGER,
  latency_ms INTEGER,                -- How long it took to generate
  FOREIGN KEY (room_id) REFERENCES conversation_contexts(room_id)
);

-- Rate limiting state (per-room)
CREATE TABLE rate_limit_state (
  room_id TEXT PRIMARY KEY,
  responses_in_last_minute TEXT,     -- JSON array of timestamps
  responses_in_last_hour TEXT,       -- JSON array of timestamps
  last_response_time INTEGER,
  consecutive_responses INTEGER,
  cooldown_until INTEGER,            -- Forced cooldown timestamp
  FOREIGN KEY (room_id) REFERENCES conversation_contexts(room_id)
);

-- Learned patterns (keyword → response effectiveness)
CREATE TABLE learned_patterns (
  id TEXT PRIMARY KEY,
  keyword TEXT,
  response_template TEXT,
  times_used INTEGER DEFAULT 0,
  positive_reactions INTEGER DEFAULT 0,  -- User reacted positively
  negative_reactions INTEGER DEFAULT 0,  -- User seemed confused/annoyed
  effectiveness_score REAL,              -- Calculated metric
  last_used_timestamp INTEGER
);

-- Preferences and configuration
CREATE TABLE persona_config (
  key TEXT PRIMARY KEY,
  value TEXT,                        -- JSON serialized config
  updated_at INTEGER
);

-- RAG context index (pointers to actual context files)
CREATE TABLE rag_context_index (
  room_id TEXT PRIMARY KEY,
  context_file_path TEXT,            -- Path to room-{uuid}.json
  message_count INTEGER,
  token_count_estimate INTEGER,
  last_updated INTEGER,
  needs_summarization BOOLEAN DEFAULT 0
);
```

---

## Event Queue & Priority Scheduling

### Event Types with CPU Interrupt Analogy

```typescript
enum EventPriority {
  CRITICAL = 1,    // @mention (interrupt - drop everything)
  HIGH = 3,        // Direct question in active conversation
  MEDIUM = 5,      // Keyword match
  LOW = 7,         // Random engagement opportunity
  BACKGROUND = 9   // Context updates, cleanup
}

interface PersonaEvent {
  id: UUID;
  type: 'mention' | 'keyword' | 'message-received' | 'room-update' | 'context-cleanup';
  priority: EventPriority;
  roomId: UUID;
  messageId?: UUID;
  timestamp: Date;
  context: {
    senderType: 'human' | 'ai' | 'system';
    messageText?: string;
    triggerKeyword?: string;
  };
}
```

### Persona Event Queue (Like CPU Scheduler)

```typescript
class PersonaEventQueue {
  private queues: Map<EventPriority, PersonaEvent[]> = new Map();
  private processing: boolean = false;
  private currentContext: UUID | null = null; // Current "running" room

  /**
   * Add event to appropriate priority queue
   * (Like CPU interrupt controller)
   */
  enqueue(event: PersonaEvent): void {
    const queue = this.queues.get(event.priority) || [];
    queue.push(event);
    this.queues.set(event.priority, queue);

    // Sort queue by timestamp (FIFO within priority)
    queue.sort((a, b) => a.timestamp.getTime() - b.timestamp.getTime());

    // If CRITICAL priority, interrupt current processing
    if (event.priority === EventPriority.CRITICAL) {
      this.interruptCurrentContext();
    }
  }

  /**
   * Get next event to process (highest priority first)
   * (Like CPU scheduler selecting next process)
   */
  dequeue(): PersonaEvent | null {
    // Check queues from highest to lowest priority
    for (let priority = 1; priority <= 9; priority += 2) {
      const queue = this.queues.get(priority as EventPriority);
      if (queue && queue.length > 0) {
        return queue.shift()!;
      }
    }
    return null;
  }

  /**
   * Interrupt current context (like hardware interrupt)
   */
  private interruptCurrentContext(): void {
    if (this.currentContext && this.processing) {
      console.log(`🔴 INTERRUPT: Switching from room ${this.currentContext} for CRITICAL event`);
      // Save current context state before switching
      this.saveContextState(this.currentContext);
    }
  }
}
```

---

## Context Switching Architecture

### Context Switch Operations (Like OS Context Switch)

```typescript
class PersonaContextManager {
  private contexts: Map<UUID, ConversationContext> = new Map(); // Loaded contexts
  private hotContext: UUID | null = null;                       // Currently active context
  private database: PersonaSQLite;                              // Persistent storage

  /**
   * Context switch to a different room
   * (Like OS saving registers and loading new process state)
   */
  async switchContext(fromRoomId: UUID | null, toRoomId: UUID): Promise<void> {
    console.log(`🔄 CONTEXT SWITCH: ${fromRoomId || 'none'} → ${toRoomId}`);

    // STEP 1: Save outgoing context (like saving CPU registers)
    if (fromRoomId) {
      await this.saveContext(fromRoomId);
      console.log(`💾 Saved context for room ${fromRoomId}`);
    }

    // STEP 2: Load incoming context (like loading new process state)
    const context = await this.loadContext(toRoomId);
    this.hotContext = toRoomId;
    console.log(`📥 Loaded context for room ${toRoomId}, messages: ${context.recentMessages.length}`);

    // STEP 3: Update context priority (recently accessed = higher priority)
    await this.updateContextPriority(toRoomId, EventPriority.HIGH);
  }

  /**
   * Save context to SQLite (like writing to disk)
   */
  private async saveContext(roomId: UUID): Promise<void> {
    const context = this.contexts.get(roomId);
    if (!context) return;

    // Save to SQLite
    await this.database.updateConversationContext({
      room_id: roomId,
      last_message_id: context.lastMessageId,
      last_read_timestamp: Date.now(),
      messages_read_count: context.messagesReadCount,
      consecutive_responses: context.consecutiveResponses,
      last_response_timestamp: context.lastResponseTime?.getTime() || null,
      is_active: false // No longer hot
    });

    // Save RAG context to JSON file
    await this.saveRAGContext(roomId, context.recentMessages);
  }

  /**
   * Load context from SQLite (like loading from disk)
   */
  private async loadContext(roomId: UUID): Promise<ConversationContext> {
    // Check if already in memory (cache hit)
    if (this.contexts.has(roomId)) {
      console.log(`✅ Context cache HIT for room ${roomId}`);
      return this.contexts.get(roomId)!;
    }

    console.log(`💿 Context cache MISS for room ${roomId}, loading from SQLite...`);

    // Load from SQLite
    const dbContext = await this.database.getConversationContext(roomId);
    const ragContext = await this.loadRAGContext(roomId);

    const context: ConversationContext = {
      roomId,
      lastMessageId: dbContext?.last_message_id || null,
      messagesReadCount: dbContext?.messages_read_count || 0,
      recentMessages: ragContext?.messages || [],
      consecutiveResponses: dbContext?.consecutive_responses || 0,
      lastResponseTime: dbContext?.last_response_timestamp
        ? new Date(dbContext.last_response_timestamp)
        : null,
      rateLimitState: await this.loadRateLimitState(roomId)
    };

    // Cache in memory
    this.contexts.set(roomId, context);
    return context;
  }

  /**
   * Evict cold contexts from memory (like OS page swapping)
   */
  async evictColdContexts(maxCachedContexts: number = 5): Promise<void> {
    if (this.contexts.size <= maxCachedContexts) return;

    // Get context priorities from SQLite
    const priorities = await this.database.getContextPriorities();

    // Sort by priority (lower = more important)
    const sortedRoomIds = Array.from(this.contexts.keys()).sort((a, b) => {
      const prioA = priorities.get(a) || 10;
      const prioB = priorities.get(b) || 10;
      return prioB - prioA; // Highest priority last
    });

    // Evict lowest priority contexts
    const toEvict = sortedRoomIds.slice(0, sortedRoomIds.length - maxCachedContexts);
    for (const roomId of toEvict) {
      if (roomId !== this.hotContext) { // Never evict hot context
        await this.saveContext(roomId);
        this.contexts.delete(roomId);
        console.log(`🗑️  Evicted cold context for room ${roomId}`);
      }
    }
  }
}
```

---

## Instruction Execution Pipeline (Message Processing)

### Pipeline Stages (Like CPU Pipeline)

```typescript
class PersonaExecutionPipeline {
  /**
   * STAGE 1: FETCH - Get event from queue
   * (Like CPU instruction fetch)
   */
  private async fetch(): Promise<PersonaEvent | null> {
    return this.eventQueue.dequeue();
  }

  /**
   * STAGE 2: DECODE - Analyze event and load context
   * (Like CPU instruction decode)
   */
  private async decode(event: PersonaEvent): Promise<ExecutionContext> {
    // Context switch if needed
    if (this.contextManager.hotContext !== event.roomId) {
      await this.contextManager.switchContext(
        this.contextManager.hotContext,
        event.roomId
      );
    }

    // Load context (registers)
    const context = await this.contextManager.loadContext(event.roomId);

    // Decode event type and prepare execution
    return {
      event,
      context,
      operation: this.determineOperation(event),
      operands: await this.loadOperands(event, context)
    };
  }

  /**
   * STAGE 3: EXECUTE - Make response decision
   * (Like CPU ALU execution)
   */
  private async execute(execContext: ExecutionContext): Promise<ExecutionResult> {
    const { event, context, operation } = execContext;

    // Execute operation based on type
    switch (operation) {
      case 'RESPOND':
        return await this.executeRespond(event, context);

      case 'UPDATE_CONTEXT':
        return await this.executeUpdateContext(event, context);

      case 'RATE_LIMIT_CHECK':
        return await this.executeRateLimitCheck(event, context);

      case 'NOP': // No operation (like CPU NOP instruction)
        return { action: 'none', reason: 'rate-limited or low priority' };

      default:
        throw new Error(`Unknown operation: ${operation}`);
    }
  }

  /**
   * STAGE 4: MEMORY - Update persona memory
   * (Like CPU memory write-back)
   */
  private async memory(result: ExecutionResult): Promise<void> {
    if (result.action === 'respond') {
      // Write response to database
      await this.database.insertResponseHistory({
        room_id: result.roomId,
        message_id: result.messageId,
        trigger_message_id: result.triggerMessageId,
        trigger_type: result.triggerType,
        response_text: result.responseText,
        timestamp: Date.now(),
        latency_ms: result.latencyMs
      });

      // Update rate limit state
      await this.updateRateLimitState(result.roomId);
    }

    // Update context (program counter)
    await this.updateLastReadPosition(result.roomId, result.lastMessageId);
  }

  /**
   * STAGE 5: WRITE-BACK - Post message to chat
   * (Like CPU committing results)
   */
  private async writeBack(result: ExecutionResult): Promise<void> {
    if (result.action === 'respond') {
      // Post message via Commands API
      await Commands.execute(DATA_COMMANDS.CREATE, {
        collection: ChatMessageEntity.collection,
        backend: 'server',
        data: result.messageEntity
      });

      console.log(`✅ Pipeline complete: Posted response to room ${result.roomId}`);
    }
  }

  /**
   * Main pipeline loop (like CPU fetch-decode-execute cycle)
   */
  async run(): Promise<void> {
    while (true) {
      try {
        // FETCH
        const event = await this.fetch();
        if (!event) {
          await this.sleep(100); // Idle (like CPU halt)
          continue;
        }

        // DECODE
        const execContext = await this.decode(event);

        // EXECUTE
        const result = await this.execute(execContext);

        // MEMORY
        await this.memory(result);

        // WRITE-BACK
        await this.writeBack(result);

      } catch (error) {
        console.error('❌ Pipeline error:', error);
        // Continue execution (don't crash)
      }
    }
  }
}
```

---

## Priority-Based Scheduling

### Scheduling Algorithm (Like CPU Scheduler)

```typescript
class PersonaScheduler {
  private personas: Map<UUID, PersonaProcessor> = new Map();

  /**
   * Schedule event for persona (like OS scheduler)
   */
  scheduleEvent(personaId: UUID, event: PersonaEvent): void {
    const persona = this.personas.get(personaId);
    if (!persona) {
      console.warn(`⚠️  Persona ${personaId} not found`);
      return;
    }

    // Add event to persona's queue
    persona.eventQueue.enqueue(event);

    // If persona is idle and event is high priority, wake it up
    if (!persona.isProcessing && event.priority <= EventPriority.HIGH) {
      this.wakePersona(personaId);
    }
  }

  /**
   * Schedule @mention event (highest priority interrupt)
   */
  scheduleMentionEvent(personaId: UUID, message: ChatMessageEntity): void {
    const event: PersonaEvent = {
      id: generateUUID(),
      type: 'mention',
      priority: EventPriority.CRITICAL,
      roomId: message.roomId,
      messageId: message.id,
      timestamp: new Date(),
      context: {
        senderType: 'human', // Assume human for @mentions
        messageText: message.content?.text || ''
      }
    };

    this.scheduleEvent(personaId, event);
    console.log(`🔴 INTERRUPT: @mention scheduled for ${personaId} in room ${message.roomId}`);
  }

  /**
   * Broadcast event to all personas (like broadcast interrupt)
   */
  broadcastMessageEvent(message: ChatMessageEntity): void {
    for (const [personaId, persona] of this.personas) {
      // Skip sender
      if (message.senderId === personaId) continue;

      // Check if persona is in this room
      if (!persona.myRoomIds.has(message.roomId)) continue;

      // Determine priority based on content
      const priority = this.determinePriority(message, persona);

      const event: PersonaEvent = {
        id: generateUUID(),
        type: 'message-received',
        priority,
        roomId: message.roomId,
        messageId: message.id,
        timestamp: new Date(),
        context: {
          senderType: await persona.checkSenderType(message.senderId),
          messageText: message.content?.text || ''
        }
      };

      this.scheduleEvent(personaId, event);
    }
  }
}
```

---

## Cache Management (Hot vs Cold Context)

### Context Caching Strategy (Like CPU Cache Hierarchy)

```
L1 Cache (Hot)       →  Current room context (in memory, instant access)
L2 Cache (Warm)      →  Recently active rooms (in memory, fast access)
L3 Cache (Cold)      →  Inactive rooms (in SQLite, slower access)
Disk Storage         →  Full history (in RAG context files, slowest)
```

```typescript
interface ContextCachePolicy {
  maxHotContexts: number;        // L1 cache size (e.g., 1)
  maxWarmContexts: number;       // L2 cache size (e.g., 5)
  coldContextTimeout: number;    // Time before eviction (e.g., 300000ms = 5 min)
  prefetchNeighbors: boolean;    // Prefetch related rooms
}

class PersonaContextCache {
  private hotContext: ConversationContext | null = null;           // L1
  private warmContexts: Map<UUID, ConversationContext> = new Map(); // L2
  private accessTimes: Map<UUID, number> = new Map();              // LRU tracking

  /**
   * Get context with cache hierarchy
   */
  async getContext(roomId: UUID): Promise<ConversationContext> {
    // L1 cache hit (instant)
    if (this.hotContext?.roomId === roomId) {
      console.log(`⚡ L1 cache HIT: ${roomId}`);
      return this.hotContext;
    }

    // L2 cache hit (fast)
    if (this.warmContexts.has(roomId)) {
      console.log(`🔥 L2 cache HIT: ${roomId}`);
      const context = this.warmContexts.get(roomId)!;
      this.promote(roomId, context); // Promote to L1
      return context;
    }

    // L3 cache miss (slow - load from SQLite)
    console.log(`💿 L3 cache MISS: ${roomId}, loading from storage...`);
    const context = await this.loadFromStorage(roomId);
    this.addToWarmCache(roomId, context);
    return context;
  }

  /**
   * Promote context to L1 (make it hot)
   */
  private promote(roomId: UUID, context: ConversationContext): void {
    // Demote current hot context to warm
    if (this.hotContext) {
      this.warmContexts.set(this.hotContext.roomId, this.hotContext);
    }

    // Promote to hot
    this.hotContext = context;
    this.warmContexts.delete(roomId);
    this.accessTimes.set(roomId, Date.now());
  }

  /**
   * Evict least recently used contexts (LRU policy)
   */
  private evictLRU(): void {
    if (this.warmContexts.size <= this.policy.maxWarmContexts) return;

    // Sort by access time
    const sorted = Array.from(this.accessTimes.entries())
      .sort((a, b) => a[1] - b[1]);

    // Evict oldest
    const toEvict = sorted[0][0];
    const context = this.warmContexts.get(toEvict);
    if (context) {
      this.saveToStorage(toEvict, context);
      this.warmContexts.delete(toEvict);
      this.accessTimes.delete(toEvict);
      console.log(`🗑️  Evicted LRU context: ${toEvict}`);
    }
  }
}
```

---

## Implementation Priority

### Phase 1: Core Processor Architecture
1. ✅ PersonaEventQueue (event scheduling)
2. ✅ PersonaContextManager (context switching)
3. ✅ PersonaSQLite (persistent memory)
4. ⏭️ PersonaExecutionPipeline (fetch-decode-execute)

### Phase 2: Advanced Features
5. ⏭️ Priority-based scheduling
6. ⏭️ Context caching (hot/warm/cold)
7. ⏭️ LRU eviction policy
8. ⏭️ Prefetching optimization

### Phase 3: Performance Optimization
9. ⏭️ Pipeline parallelism (multiple personas)
10. ⏭️ Batch processing for low-priority events
11. ⏭️ Adaptive scheduling based on load

---

## Key Architectural Benefits

✅ **Scalability**: Each persona is independent processor
✅ **Isolation**: Per-persona SQLite prevents context leakage
✅ **Priority**: @mentions interrupt current processing
✅ **Efficiency**: Context caching reduces SQLite I/O
✅ **Fairness**: Scheduling ensures all personas get CPU time
✅ **Debugging**: Clear pipeline stages for observability

This architecture naturally handles multiple rooms, multiple personas, and complex interaction patterns - just like a CPU handles multiple processes!
