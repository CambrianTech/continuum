# Hippocampus Vector-Based Memory Retrieval

**Status**: Design Phase (Phase 3)
**Created**: 2025-11-23
**Parent Doc**: `system/search/SEMANTIC-SEARCH-ARCHITECTURE.md`

## Overview

Hippocampus uses semantic vector search to recall relevant memories during conversation and task processing. This enables:
- **Context-aware recall**: Find memories semantically related to current situation
- **Hybrid retrieval**: Combine semantic similarity with metadata filters
- **Automatic memory injection**: Insert relevant past experiences into prompt context
- **Cross-domain recall**: Find related memories across different domains

**Key Innovation**: Memory recall uses the same DataDaemon vector search infrastructure as Genome discovery - consistent abstraction, swappable backends.

---

## Architecture

```
Incoming Message: "The system is stuck in a loop again"
    ↓
PersonaMemory.recallRelevant(messageText, domain)
    ↓
Commands.execute('data/vector-search', {
  collection: 'memories',
  queryText: messageText,
  filter: { importance: { $gte: 0.5 } }
})
    ↓
DataDaemon.vectorSearch() [via adapter strategy]
    ↓
SQLite/PostgreSQL/Elasticsearch backend
    ↓
Returns:
  - "System restarts resolve coordination failures"
  - "Repetition loops indicate broken coordination"
  - "Emergency protocol: stop all AIs"
    ↓
PersonaResponseGenerator injects memories into prompt
    ↓
Persona responds with context from past experiences
```

---

## Data Model

### Memory Entity (Already Exists)

```typescript
// system/user/server/modules/MemoryTypes.ts

export interface MemoryEntity extends BaseEntity {
  id: UUID;
  createdAt: ISOString;
  updatedAt: ISOString;
  version: number;

  // Core memory
  personaId: UUID;
  sessionId: UUID;
  type: MemoryType;              // OBSERVATION, DECISION, INSIGHT, etc.
  content: string;               // Synthesized insight (SEARCHABLE)

  // Context
  context: {
    domain: string;              // 'chat', 'debugging', 'architecture'
    contextId: string;           // Room or conversation ID
    thoughtCount?: number;
    synthesizedFrom?: UUID[];    // Original thought IDs
  };

  // Metadata
  timestamp: ISOString;
  consolidatedAt: ISOString;
  importance: number;            // 0-1
  accessCount: number;
  relatedTo: UUID[];
  tags: string[];                // ['chat', 'synthesized', 'coordination']
  source: string;                // 'semantic-compression', 'working-memory'
}
```

### Searchable Content for Memories

When indexing memories for vector search:

```typescript
function buildSearchableContent(memory: MemoryEntity): string {
  return `
${memory.content}

Domain: ${memory.context.domain}
Type: ${memory.type}
Tags: ${memory.tags.join(', ')}
Importance: ${memory.importance}
  `.trim();
}
```

---

## Implementation

### PersonaMemory Integration

```typescript
// system/user/server/modules/cognitive/memory/PersonaMemory.ts

export class PersonaMemory {
  private personaId: UUID;
  private displayName: string;

  /**
   * Recall relevant memories for current context
   */
  async recallRelevant(
    currentContext: string,
    options: {
      domain?: string;
      minImportance?: number;
      limit?: number;
      timeRange?: { start?: Date; end?: Date };
      types?: MemoryType[];
    } = {}
  ): Promise<MemoryEntity[]> {

    // Build filter
    const filter: Record<string, any> = {
      personaId: this.personaId,
      importance: { $gte: options.minImportance || 0.5 }
    };

    if (options.domain) {
      filter['context.domain'] = options.domain;
    }

    if (options.types) {
      filter.type = { $in: options.types };
    }

    if (options.timeRange) {
      filter.timestamp = {
        ...(options.timeRange.start && { $gte: options.timeRange.start.toISOString() }),
        ...(options.timeRange.end && { $lte: options.timeRange.end.toISOString() })
      };
    }

    // Vector search through DataDaemon
    const results = await Commands.execute('data/vector-search', {
      collection: 'memories',
      queryText: currentContext,
      k: options.limit || 5,
      similarityThreshold: 0.6,
      hybridMode: 'hybrid',  // Semantic + keyword
      hybridRatio: 0.7,      // 70% semantic, 30% keyword
      filter
    });

    // Update access counts
    await this.updateAccessCounts(results.results.map(r => r.data.id));

    return results.results.map(r => r.data as MemoryEntity);
  }

  /**
   * Recall by specific criteria (keyword-based)
   */
  async recallByKeywords(
    keywords: string[],
    options: RecallOptions = {}
  ): Promise<MemoryEntity[]> {

    const results = await Commands.execute('data/vector-search', {
      collection: 'memories',
      queryText: keywords.join(' '),
      k: options.limit || 10,
      hybridMode: 'keyword',  // Keyword-only search
      filter: {
        personaId: this.personaId,
        tags: { $in: keywords },
        ...this.buildFilterFromOptions(options)
      }
    });

    return results.results.map(r => r.data as MemoryEntity);
  }

  /**
   * Recall similar to a specific memory
   */
  async recallSimilarTo(
    memoryId: UUID,
    options: { limit?: number; excludeSelf?: boolean } = {}
  ): Promise<MemoryEntity[]> {

    // Get the source memory
    const sourceMemory = await Commands.execute('data/get', {
      collection: 'memories',
      id: memoryId
    });

    if (!sourceMemory.data) {
      return [];
    }

    // Search using source memory's content
    const results = await Commands.execute('data/vector-search', {
      collection: 'memories',
      queryText: sourceMemory.data.content,
      k: (options.limit || 5) + (options.excludeSelf ? 1 : 0),
      similarityThreshold: 0.7,
      hybridMode: 'semantic',
      filter: {
        personaId: this.personaId
      }
    });

    // Filter out source memory if requested
    let filtered = results.results.map(r => r.data as MemoryEntity);
    if (options.excludeSelf) {
      filtered = filtered.filter(m => m.id !== memoryId);
    }

    return filtered.slice(0, options.limit || 5);
  }

  /**
   * Store memory with vector indexing
   */
  async storeMemory(memory: MemoryEntity): Promise<void> {
    // 1. Store memory entity
    await Commands.execute('data/create', {
      collection: 'memories',
      data: memory
    });

    // 2. Generate and index embedding
    const searchableContent = buildSearchableContent(memory);
    const embedding = await Commands.execute('data/generate-embedding', {
      text: searchableContent,
      model: 'all-minilm'
    });

    await Commands.execute('data/index-vector', {
      collection: 'memories',
      id: memory.id,
      embedding: embedding.vector
    });

    console.log(`💾 [${this.displayName}] Stored and indexed memory: ${memory.content.substring(0, 50)}...`);
  }

  /**
   * Update access counts (for spaced repetition)
   */
  private async updateAccessCounts(memoryIds: UUID[]): Promise<void> {
    for (const id of memoryIds) {
      await Commands.execute('data/update', {
        collection: 'memories',
        id,
        updates: {
          $inc: { accessCount: 1 },
          lastAccessedAt: new Date().toISOString()
        }
      });
    }
  }
}
```

### Integration with Response Generation

```typescript
// system/user/server/modules/PersonaResponseGenerator.ts

export class PersonaResponseGenerator {

  async generateResponse(
    message: ChatMessageEntity,
    context: ConversationContext
  ): Promise<string> {

    // 1. Recall relevant memories
    const memories = await this.persona.memory.recallRelevant(
      message.content.text,
      {
        domain: context.domain || 'chat',
        minImportance: 0.6,
        limit: 3,
        timeRange: {
          start: this.getRecentTimeThreshold()  // Last 30 days
        }
      }
    );

    // 2. Build prompt with memories
    const prompt = this.buildPromptWithMemories(message, memories, context);

    // 3. Generate response with context
    const response = await this.persona.generateText({
      prompt,
      temperature: 0.7,
      maxTokens: 500,
      context: 'chat-response'
    });

    return response;
  }

  private buildPromptWithMemories(
    message: ChatMessageEntity,
    memories: MemoryEntity[],
    context: ConversationContext
  ): string {

    const systemPrompt = this.buildSystemPrompt(context);
    const recentMessages = this.getRecentMessages(context.roomId, 10);

    // Format memories for injection
    const memoryContext = memories.length > 0
      ? `## Relevant Past Experiences\n${memories.map(m =>
          `- ${m.content} (${this.formatTimeAgo(m.timestamp)})`
        ).join('\n')}`
      : '';

    return `
${systemPrompt}

${memoryContext}

## Recent Conversation
${recentMessages.map(m => `${m.senderName}: ${m.content.text}`).join('\n')}

## Current Message
${message.senderName}: ${message.content.text}

Respond naturally, incorporating relevant past experiences where appropriate.
    `.trim();
  }

  private getRecentTimeThreshold(): Date {
    // Memories from last 30 days
    return new Date(Date.now() - (30 * 24 * 60 * 60 * 1000));
  }

  private formatTimeAgo(timestamp: ISOString): string {
    const date = new Date(timestamp);
    const daysAgo = Math.floor((Date.now() - date.getTime()) / (24 * 60 * 60 * 1000));

    if (daysAgo === 0) return 'today';
    if (daysAgo === 1) return 'yesterday';
    if (daysAgo < 7) return `${daysAgo} days ago`;
    if (daysAgo < 30) return `${Math.floor(daysAgo / 7)} weeks ago`;
    return `${Math.floor(daysAgo / 30)} months ago`;
  }
}
```

### Integration with Hippocampus

```typescript
// system/user/server/modules/cognitive/memory/Hippocampus.ts

export class Hippocampus {

  async consolidate(): Promise<void> {
    // ... existing consolidation logic ...

    // After creating synthesized memory
    const memory = await this.consolidationAdapter.synthesize(thoughts, context);

    // Store with vector indexing
    await this.personaMemory.storeMemory(memory);

    console.log(`✅ [${this.displayName}] Consolidated and indexed memory`);
  }
}
```

---

## CLI Commands

### Memory Search

```bash
# Search memories semantically
./jtag memory/search --persona="helper-ai" --query="coordination failures"
# Results:
# 1. System restarts resolve coordination failures (0.89, 2 days ago)
# 2. Repetition loops indicate broken coordination (0.85, 1 week ago)
# 3. Emergency protocol: stop all AIs (0.78, 1 week ago)

# Search by keywords
./jtag memory/search --persona="helper-ai" \
  --keywords="debugging,async,errors" \
  --min-importance=0.7

# Find similar memories
./jtag memory/similar --memory-id=<uuid> --limit=5

# Search with filters
./jtag data/vector-search \
  --collection=memories \
  --query="error handling strategies" \
  --filter='{"context.domain":"debugging","importance":{"$gte":0.6}}'
```

### Memory Management

```bash
# Backfill vectors for existing memories
./jtag data/backfill-vectors --collection=memories --batch-size=100

# Check vector index status
./jtag data/vector-stats --collection=memories
# Output:
# Collection: memories
# Total documents: 1247
# Documents with vectors: 1247
# Vector dimensions: 384
# Index size: 1.9MB

# Export memories with similarity scores
./jtag memory/export --persona="helper-ai" \
  --query="debugging experiences" \
  --format=markdown \
  --include-scores
```

---

## Usage Examples

### Example 1: Debugging Recall

```typescript
// User: "The async handler is crashing again"

// Recall relevant debugging memories
const memories = await persona.memory.recallRelevant(
  "async handler crashing errors",
  {
    domain: 'debugging',
    minImportance: 0.7,
    limit: 3,
    types: [MemoryType.INSIGHT, MemoryType.DECISION]
  }
);

// Memories retrieved:
// 1. "Async crashes often due to unhandled promise rejections - wrap in try/catch"
// 2. "Race conditions in async handlers resolved by adding mutex locks"
// 3. "Logging async errors to file helped identify root cause"

// AI response incorporates past solutions:
// "Based on past debugging sessions, async handler crashes are commonly caused by
//  unhandled promise rejections. I recommend wrapping the handler in try/catch and
//  adding detailed error logging, which successfully resolved similar issues before."
```

### Example 2: Pattern Recognition

```typescript
// User: "Users are reporting slow response times"

// Recall performance-related memories
const memories = await persona.memory.recallRelevant(
  "slow response times performance issues",
  {
    domain: 'performance',
    minImportance: 0.6,
    limit: 5
  }
);

// AI recognizes pattern from past memories:
// "We've seen this pattern before - slow responses were caused by database query
//  inefficiency. The solution was adding indexes on frequently queried columns.
//  Should we check the query performance logs?"
```

### Example 3: Cross-Domain Recall

```typescript
// Current: Architectural discussion
// Find related patterns from debugging domain

const architectureMemories = await persona.memory.recallRelevant(
  "microservices communication patterns",
  { domain: 'architecture', limit: 3 }
);

const debuggingMemories = await persona.memory.recallRelevant(
  "service communication failures",
  { domain: 'debugging', limit: 2 }
);

// Combine insights across domains:
// "When designing microservices communication, remember we debugged similar
//  issues with message queue timeouts. Consider implementing circuit breakers
//  based on those lessons."
```

---

## Advanced Features

### Temporal Decay

```typescript
// Boost recent memories, decay old ones
async recallWithDecay(
  query: string,
  options: RecallOptions
): Promise<MemoryEntity[]> {

  const results = await this.recallRelevant(query, options);

  // Apply temporal decay to scores
  return results.map(memory => ({
    ...memory,
    score: this.applyTemporalDecay(memory)
  })).sort((a, b) => b.score - a.score);
}

private applyTemporalDecay(memory: MemoryEntity): number {
  const daysOld = this.getDaysOld(memory.timestamp);
  const decayFactor = Math.exp(-daysOld / 90);  // Half-life of 90 days

  return memory.importance * decayFactor;
}
```

### Spaced Repetition

```typescript
// Resurface memories that need reinforcement
async findMemoriesNeedingReview(): Promise<MemoryEntity[]> {

  const results = await Commands.execute('data/list', {
    collection: 'memories',
    filter: {
      personaId: this.personaId,
      importance: { $gte: 0.7 },
      accessCount: { $lt: 5 },  // Important but rarely accessed
      timestamp: { $lte: this.getDateNDaysAgo(30) }  // Older than 30 days
    },
    orderBy: [
      { field: 'importance', direction: 'desc' },
      { field: 'timestamp', direction: 'asc' }
    ],
    limit: 10
  });

  return results.data;
}
```

### Memory Clustering

```typescript
// Find clusters of related memories
async findMemoryClusters(
  seedMemoryId: UUID,
  depth: number = 2
): Promise<MemoryEntity[][]> {

  const visited = new Set<UUID>();
  const clusters: MemoryEntity[][] = [];

  async function explore(memoryId: UUID, currentDepth: number) {
    if (currentDepth >= depth || visited.has(memoryId)) return;

    visited.add(memoryId);

    const similar = await this.recallSimilarTo(memoryId, {
      limit: 5,
      excludeSelf: true
    });

    if (similar.length > 0) {
      clusters.push(similar);

      // Recursively explore
      for (const mem of similar) {
        await explore(mem.id, currentDepth + 1);
      }
    }
  }

  await explore(seedMemoryId, 0);
  return clusters;
}
```

---

## Performance Optimization

### Caching

```typescript
// Cache recent query embeddings
private queryCache = new LRUCache<string, number[]>({
  max: 100,
  ttl: 1000 * 60 * 10  // 10 minutes
});

async recallRelevant(context: string, options: RecallOptions): Promise<MemoryEntity[]> {
  // Check cache
  let queryEmbedding = this.queryCache.get(context);

  if (!queryEmbedding) {
    queryEmbedding = await this.generateEmbedding(context);
    this.queryCache.set(context, queryEmbedding);
  }

  // Use cached embedding
  return this.vectorSearch(queryEmbedding, options);
}
```

### Batch Processing

```typescript
// Backfill embeddings in batches
async backfillEmbeddings(batchSize: number = 100): Promise<void> {
  let offset = 0;
  let processed = 0;

  while (true) {
    const batch = await Commands.execute('data/list', {
      collection: 'memories',
      filter: { personaId: this.personaId },
      offset,
      limit: batchSize
    });

    if (batch.data.length === 0) break;

    // Generate embeddings in parallel
    const embeddings = await Promise.all(
      batch.data.map(m =>
        this.generateEmbedding(buildSearchableContent(m))
      )
    );

    // Index in parallel
    await Promise.all(
      batch.data.map((m, i) =>
        Commands.execute('data/index-vector', {
          collection: 'memories',
          id: m.id,
          embedding: embeddings[i]
        })
      )
    );

    processed += batch.data.length;
    offset += batchSize;

    console.log(`Backfilled ${processed} memories...`);
  }
}
```

---

## Migration Path

### Phase 3.0: Basic Vector Retrieval (Week 1)

1. Update PersonaMemory with `recallRelevant()`
2. Integrate with PersonaResponseGenerator
3. Backfill vectors for existing memories
4. Test with real conversations

### Phase 3.5: Advanced Features (Week 2-3)

1. Temporal decay
2. Spaced repetition
3. Memory clustering
4. Cross-domain recall

### Phase 4.0: Optimization (Week 4)

1. Query caching
2. Batch processing
3. Performance monitoring
4. Scale testing

---

## Testing Strategy

```typescript
// tests/integration/memory-recall.test.ts

describe('Memory Recall', () => {
  it('should recall semantically similar memories', async () => {
    // Store test memories
    await memory.storeMemory({
      content: "System restarts resolve coordination failures",
      domain: "debugging",
      importance: 0.8
    });

    await memory.storeMemory({
      content: "Emergency protocol stops all AIs",
      domain: "debugging",
      importance: 0.7
    });

    // Query with similar but different words
    const recalled = await memory.recallRelevant(
      "systems stuck in loops need intervention",
      { domain: "debugging", limit: 2 }
    );

    expect(recalled).toHaveLength(2);
    expect(recalled[0].content).toContain("coordination");
  });
});
```

---

## Next Steps

1. **Read**: `SEMANTIC-SEARCH-ARCHITECTURE.md` for DataDaemon implementation
2. **Read**: `PERSONA-GENOME-VECTOR-SEARCH.md` for genome discovery patterns
3. **Implement**: Start with `recallRelevant()` method
4. **Integrate**: Hook into PersonaResponseGenerator
5. **Test**: Verify memories improve response quality

---

## References

- Parent doc: `system/search/SEMANTIC-SEARCH-ARCHITECTURE.md`
- Related: `HIPPOCAMPUS-MEMORY-DESIGN.md` (consolidation process)
- Related: `PERSONA-AS-INTERFACE.md` (unified inference)
- Related: `NESTED-LEARNING-CONNECTION.md` (multi-tier memory)
