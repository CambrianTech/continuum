# Hippocampus Memory Design

**Status**: Design Phase
**Author**: Claude Code
**Date**: 2025-11-23
**Version**: 1.0

## Overview

Hippocampus is a PersonaContinuousSubprocess that manages episodic memory for PersonaUsers. It implements a two-tier memory system inspired by biological memory consolidation:

1. **Short-term memory** (STM) - Fast, in-memory ring buffer (last N events)
2. **Long-term memory** (LTM) - Persistent SQLite database (important memories)

**Key Philosophy**: Not every experience needs to be remembered forever. Hippocampus filters, consolidates, and prunes memories based on importance, recency, and relevance.

---

## Architecture

### RTOS Integration

```typescript
export class Hippocampus extends PersonaContinuousSubprocess {
  // Subprocess priority: 'low' (1000ms tick rate)
  // Runs continuously in background, never blocks other processes

  protected async tick(): Promise<void> {
    // 1. Process enqueued experiences (from PersonaUser)
    // 2. Evaluate STM for consolidation candidates
    // 3. Consolidate important memories to LTM
    // 4. Prune old/low-importance STM entries
    // 5. Background maintenance (cleanup, indexing)
  }
}
```

### Database Architecture

Each PersonaUser has a **dedicated SQLite database** for long-term memory:

```
.continuum/personas/
└── helper-ai-154ee833/
    ├── memory/
    │   └── longterm.db          ← Persistent across all sessions
    └── sessions/
        └── 5a93320b/
            └── logs/
                └── hippocampus.log
```

**Benefits**:
- ✅ **Isolated** - Each persona's memories are separate
- ✅ **Fast** - Direct SQLite connection with WAL mode
- ✅ **Persistent** - Survives restarts, continues learning
- ✅ **Queryable** - Full SQL power for complex retrieval
- ✅ **Scalable** - Database can grow to GBs without impacting RAM

---

## Memory Model

### Memory Types

```typescript
enum MemoryType {
  CHAT = 'chat',           // Chat messages (user interactions)
  OBSERVATION = 'observation',  // System events witnessed
  TASK = 'task',           // Tasks completed/attempted
  DECISION = 'decision',   // Decisions made (with reasoning)
  TOOL_USE = 'tool-use',   // Tool invocations and results
  ERROR = 'error',         // Errors encountered (for learning)
  INSIGHT = 'insight'      // Self-generated insights/patterns
}
```

### Memory Entity Schema

```typescript
interface MemoryEntity extends BaseEntity {
  // Core identification
  id: UUID;
  personaId: UUID;          // Owner of this memory
  sessionId: UUID;          // Session where memory originated

  // Memory content
  type: MemoryType;
  content: string;          // Primary memory content (text)
  context: Record<string, any>;  // Structured context (JSON)

  // Temporal information
  timestamp: Date;          // When memory was created
  consolidatedAt?: Date;    // When moved to LTM (null if STM only)
  lastAccessedAt?: Date;    // Last retrieval time (for LRU)

  // Importance/relevance
  importance: number;       // 0.0-1.0 score (how important to remember)
  accessCount: number;      // Times retrieved (frequently accessed = important)

  // Relationships
  relatedTo: UUID[];        // Links to other memories (graph structure)
  tags: string[];           // Searchable tags

  // Metadata
  source: string;           // Where memory came from (e.g., "chat/send", "ai/generate")
  embedding?: number[];     // Vector embedding (future: semantic search)
}
```

### Collections

1. **`memories`** - Main memory storage
2. **`memory_clusters`** - Grouped related memories (future)
3. **`memory_stats`** - Performance tracking (consolidation rates, etc.)

---

## Short-Term Memory (STM)

### Implementation

```typescript
private stmBuffer: MemoryEntity[] = [];
private readonly STM_MAX_SIZE = 100;  // Last 100 experiences
```

**Characteristics**:
- **Ring buffer** - Fixed size, oldest evicted when full
- **Fast access** - In-memory, O(1) append, O(n) search
- **Transient** - Not persisted to disk (lost on restart)
- **Working memory** - Recent context for active tasks

### When to Add to STM

PersonaUser adds experiences to STM via:

```typescript
this.hippocampus.enqueue({
  type: MemoryType.CHAT,
  content: messageEntity.content,
  context: {
    messageId: messageEntity.id,
    roomId: messageEntity.roomId,
    senderId: messageEntity.senderId
  },
  importance: this.calculateImportance(messageEntity)
});
```

---

## Long-Term Memory (LTM)

### Database Handle Management

```typescript
private memoryDbHandle: DbHandle;

async initialize(): Promise<void> {
  // Open dedicated memory database
  const result = await Commands.execute<DataOpenParams, DataOpenResult>('data/open', {
    adapter: 'sqlite',
    config: {
      path: `.continuum/personas/${this.getPersonaDirName()}/memory/longterm.db`,
      mode: 'readwrite',
      wal: true,           // Write-Ahead Logging (fast writes, concurrent reads)
      foreignKeys: true,   // Referential integrity
      poolSize: 5          // Connection pool for parallel queries
    }
  });

  this.memoryDbHandle = result.dbHandle;
  this.log(`LTM database opened: ${this.memoryDbHandle}`);

  // Ensure schema exists
  await this.ensureSchema();
}
```

### Schema Creation

```typescript
async ensureSchema(): Promise<void> {
  // Create memories table
  await this.executeSQL(`
    CREATE TABLE IF NOT EXISTS memories (
      id TEXT PRIMARY KEY,
      personaId TEXT NOT NULL,
      sessionId TEXT NOT NULL,
      type TEXT NOT NULL,
      content TEXT NOT NULL,
      context TEXT,  -- JSON
      timestamp TEXT NOT NULL,
      consolidatedAt TEXT,
      lastAccessedAt TEXT,
      importance REAL NOT NULL DEFAULT 0.5,
      accessCount INTEGER NOT NULL DEFAULT 0,
      relatedTo TEXT,  -- JSON array of UUIDs
      tags TEXT,       -- JSON array of strings
      source TEXT NOT NULL
    );

    -- Indexes for fast queries
    CREATE INDEX IF NOT EXISTS idx_memories_persona_timestamp
      ON memories(personaId, timestamp DESC);
    CREATE INDEX IF NOT EXISTS idx_memories_importance
      ON memories(importance DESC);
    CREATE INDEX IF NOT EXISTS idx_memories_type
      ON memories(type);
    CREATE INDEX IF NOT EXISTS idx_memories_tags
      ON memories(tags);  -- JSON array index (SQLite 3.38+)
  `);
}
```

---

## Consolidation Strategy

### When to Consolidate

Consolidation happens during Hippocampus `tick()` when:

1. **STM is nearly full** (>80% capacity)
2. **High-importance memory detected** (importance >= 0.7)
3. **Periodic cleanup** (every N ticks, e.g., every 60 seconds)

### Consolidation Algorithm

```typescript
async consolidateMemories(): Promise<void> {
  // 1. Select consolidation candidates from STM
  const candidates = this.stmBuffer.filter(mem =>
    mem.importance >= this.CONSOLIDATION_THRESHOLD  // Default: 0.6
  );

  // 2. Sort by importance (most important first)
  candidates.sort((a, b) => b.importance - a.importance);

  // 3. Batch write to LTM (efficient)
  if (candidates.length > 0) {
    await Commands.execute('data/create', {
      dbHandle: this.memoryDbHandle,
      collection: 'memories',
      data: candidates.map(mem => ({
        ...mem,
        consolidatedAt: new Date().toISOString()
      }))
    });

    this.log(`Consolidated ${candidates.length} memories to LTM`);

    // 4. Remove from STM (keep buffer lean)
    this.stmBuffer = this.stmBuffer.filter(
      mem => !candidates.includes(mem)
    );
  }

  // 5. Prune old low-importance STM entries
  const now = Date.now();
  const STM_MAX_AGE_MS = 60 * 60 * 1000;  // 1 hour
  this.stmBuffer = this.stmBuffer.filter(mem =>
    (now - mem.timestamp.getTime()) < STM_MAX_AGE_MS ||
    mem.importance >= 0.5  // Keep moderately important even if old
  );
}
```

### Importance Scoring

**Factors that increase importance**:
- User directly addressed the persona
- Tool use (action taken)
- Error occurred (learning opportunity)
- Decision made (reasoning trace)
- Insight generated (self-reflection)
- Frequently accessed (high `accessCount`)

**Base importance calculation**:

```typescript
calculateImportance(memory: MemoryEntity): number {
  let score = 0.5;  // Baseline

  // Type-based weights
  const typeWeights: Record<MemoryType, number> = {
    [MemoryType.CHAT]: 0.6,
    [MemoryType.OBSERVATION]: 0.4,
    [MemoryType.TASK]: 0.7,
    [MemoryType.DECISION]: 0.8,
    [MemoryType.TOOL_USE]: 0.7,
    [MemoryType.ERROR]: 0.9,      // Learn from mistakes!
    [MemoryType.INSIGHT]: 0.85
  };

  score = typeWeights[memory.type] || 0.5;

  // Context-based adjustments
  if (memory.context.mentionedMe) score += 0.2;
  if (memory.context.userDirectMessage) score += 0.15;
  if (memory.context.actionTaken) score += 0.1;
  if (memory.context.errorRecovered) score += 0.15;

  // Clamp to [0.0, 1.0]
  return Math.max(0.0, Math.min(1.0, score));
}
```

---

## Retrieval API

### Methods Exposed

```typescript
class Hippocampus extends PersonaContinuousSubprocess {
  /**
   * Recall recent memories from STM + LTM
   * Fast query for current context
   */
  async recall(params: {
    types?: MemoryType[];
    tags?: string[];
    minImportance?: number;
    limit?: number;
    since?: Date;
  }): Promise<MemoryEntity[]> {
    // 1. Search STM (in-memory, instant)
    const stmResults = this.searchSTM(params);

    // 2. Search LTM (database, fast with indexes)
    const ltmResults = await this.searchLTM(params);

    // 3. Merge and deduplicate
    const combined = this.mergeResults(stmResults, ltmResults);

    // 4. Update access tracking (LRU)
    await this.updateAccessStats(combined);

    return combined;
  }

  /**
   * Semantic search (future: vector embeddings)
   */
  async recallSimilar(query: string, limit: number = 10): Promise<MemoryEntity[]> {
    // TODO: Implement with vector embeddings
    // 1. Generate embedding for query
    // 2. Cosine similarity search in LTM
    // 3. Return top-k matches
    throw new Error('Vector search not yet implemented');
  }

  /**
   * Get memories related to specific entity (e.g., all memories about a user)
   */
  async recallRelated(params: {
    userId?: UUID;
    roomId?: UUID;
    taskId?: UUID;
  }): Promise<MemoryEntity[]> {
    // Query by context fields
    return await this.searchLTM({
      contextFilter: params,
      limit: 100
    });
  }

  /**
   * Forget memories (explicit deletion)
   * Use sparingly - prefer natural decay via importance scoring
   */
  async forget(memoryIds: UUID[]): Promise<void> {
    await Commands.execute('data/delete', {
      dbHandle: this.memoryDbHandle,
      collection: 'memories',
      ids: memoryIds
    });

    this.log(`Forgot ${memoryIds.length} memories`);
  }
}
```

### Query Examples

```typescript
// Recall recent chat memories
const recentChats = await hippocampus.recall({
  types: [MemoryType.CHAT],
  since: new Date(Date.now() - 24 * 60 * 60 * 1000),  // Last 24 hours
  minImportance: 0.5,
  limit: 50
});

// Recall all decisions made
const decisions = await hippocampus.recall({
  types: [MemoryType.DECISION],
  minImportance: 0.7
});

// Recall errors for learning
const errors = await hippocampus.recall({
  types: [MemoryType.ERROR],
  limit: 20
});

// Recall memories tagged with specific topic
const codingMemories = await hippocampus.recall({
  tags: ['coding', 'typescript'],
  minImportance: 0.6,
  limit: 30
});
```

---

## Performance Optimizations

### Database-Level

1. **WAL Mode** - Concurrent reads during writes
2. **Indexes** - Fast lookups on common query patterns
3. **Connection Pooling** - Parallel queries via DatabaseHandleRegistry
4. **Batch Writes** - Consolidate multiple memories in single transaction

### Application-Level

1. **STM Ring Buffer** - O(1) append, fast recent access
2. **Lazy Consolidation** - Only consolidate when necessary (>80% full)
3. **LRU Eviction** - Track `lastAccessedAt` for smart pruning
4. **Query Caching** - Cache frequent queries (e.g., "last 10 chats")

### Memory Bounds

```typescript
// STM limits
private readonly STM_MAX_SIZE = 100;           // Last 100 experiences
private readonly STM_MAX_AGE_MS = 60 * 60 * 1000;  // 1 hour

// LTM limits (future: implement pruning)
private readonly LTM_MAX_MEMORIES = 100000;    // 100k memories per persona
private readonly LTM_MIN_IMPORTANCE = 0.3;     // Prune below this threshold
```

---

## Integration with PersonaUser

### Feeding Memories to Hippocampus

```typescript
// PersonaUser.ts
async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // 1. Process message (generate response, etc.)
  const response = await this.generateResponse(message);

  // 2. Create memory of interaction
  this.hippocampus.enqueue({
    type: MemoryType.CHAT,
    content: `User: ${message.content}\nMe: ${response.content}`,
    context: {
      messageId: message.id,
      roomId: message.roomId,
      userId: message.senderId,
      mentionedMe: message.mentions?.includes(this.id),
      userDirectMessage: this.isDirectMessage(message)
    },
    importance: this.calculateImportance(message),
    timestamp: new Date(),
    source: 'chat/send',
    tags: this.extractTags(message.content)
  });

  // 3. Hippocampus will consolidate asynchronously
}
```

### Retrieving Context for Responses

```typescript
async generateResponse(message: ChatMessageEntity): Promise<string> {
  // 1. Recall relevant recent memories
  const context = await this.hippocampus.recall({
    types: [MemoryType.CHAT, MemoryType.OBSERVATION],
    since: new Date(Date.now() - 60 * 60 * 1000),  // Last hour
    limit: 20
  });

  // 2. Recall memories related to this user
  const userHistory = await this.hippocampus.recallRelated({
    userId: message.senderId
  });

  // 3. Build prompt with memory context
  const prompt = this.buildPrompt(message, context, userHistory);

  // 4. Generate response with AI
  return await this.ai.generate(prompt);
}
```

---

## Future Extensions

### Phase 2: Semantic Search

- Generate embeddings for memories using `ai/embedding/generate`
- Store embeddings in `embedding` field (BLOB)
- Implement cosine similarity search
- Consider vector database adapter (Qdrant, Pinecone)

### Phase 3: Memory Clustering

- Group related memories into themes/episodes
- Use temporal proximity + semantic similarity
- Implement `memory_clusters` collection
- Hierarchical memory structure (episode → memories)

### Phase 4: Forgetting Curve

- Implement Ebbinghaus forgetting curve
- Decay importance over time (unless accessed)
- Periodic background pruning of old low-importance memories
- Configurable retention policies per memory type

### Phase 5: Cross-Persona Memory (Cautious)

- Shared memory pool for team knowledge
- Privacy controls (which memories are shareable)
- Attribution (who contributed each memory)
- Careful about persona identity bleed

---

## Testing Strategy

### Unit Tests

```typescript
// tests/unit/Hippocampus.test.ts
describe('Hippocampus', () => {
  test('STM ring buffer maintains size limit', () => {
    // Add 150 memories, verify only last 100 remain
  });

  test('Consolidation moves high-importance memories to LTM', async () => {
    // Add memories with varying importance
    // Trigger consolidation
    // Verify only important ones in LTM
  });

  test('Recall merges STM + LTM results', async () => {
    // Add memories to both STM and LTM
    // Query with filters
    // Verify combined results
  });
});
```

### Integration Tests

```typescript
// tests/integration/hippocampus-memory.test.ts
describe('Hippocampus Memory System', () => {
  test('Memories persist across restarts', async () => {
    // Create persona, add memories
    // Shutdown persona
    // Restart persona
    // Verify memories still retrievable
  });

  test('Database handle is isolated per persona', async () => {
    // Create 2 personas
    // Add memories to each
    // Verify no cross-contamination
  });
});
```

---

## Implementation Phases

### ✅ Phase 0: Foundation (Complete)
- PersonaLogger (non-blocking logging)
- Hippocampus subprocess scaffold (logs ticks)

### 📋 Phase 1: Basic Memory Storage (Next)
- Open dedicated memory database in Hippocampus
- Implement STM ring buffer
- Basic consolidation (importance threshold)
- Simple recall API (recent memories)

### 📋 Phase 2: Advanced Retrieval
- Tag-based search
- Type filtering
- Related memory queries
- Access tracking (LRU)

### 📋 Phase 3: Smart Consolidation
- Context-aware importance scoring
- Adaptive thresholds (learn over time)
- Batch consolidation optimization
- Background pruning

### 📋 Phase 4: Production-Ready
- Performance benchmarks (1M+ memories)
- Memory leak testing
- Database migration tools
- Monitoring/observability

---

## Success Metrics

**Phase 1 Goals**:
- ✅ Each persona has isolated memory database
- ✅ Memories persist across restarts
- ✅ STM + LTM queries working
- ✅ Basic consolidation preventing STM overflow
- ✅ No memory leaks after 24-hour runtime

**Performance Targets**:
- STM operations: <1ms (in-memory)
- LTM queries: <10ms (with indexes)
- Consolidation: <50ms per batch (non-blocking)
- Database size: <100MB per persona (10k memories)

---

## References

- `system/user/server/modules/PersonaSubprocess.ts` - Base subprocess class
- `system/user/server/modules/PersonaLogger.ts` - Non-blocking logging pattern
- `daemons/data-daemon/server/DatabaseHandleRegistry.ts` - Multi-database handles
- `commands/data/open/shared/DataOpenTypes.ts` - Database connection API
- `docs/MULTI-DATABASE-HANDLES.md` - Handle architecture (if exists)
