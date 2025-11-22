# Memory Consolidation Architecture

**Separate Thread/Process** - Not a method call, a concurrent worker

---

## The Problem With Current Approach

**Wrong (method call)**:
```typescript
// PersonaAutonomousLoop
async serviceLoop() {
  await processInbox();
  await consolidateMemory();  // ← Blocking, synchronous
  await activateMemory();     // ← Method call
}
```

**Right (separate thread)**:
```
Main Persona Thread              Memory Consolidation Thread
    │                                    │
    │ writes to inbox                    │ peeks at inbox
    │                                    │ detects patterns
    │                                    │ consolidates when triggered
    │                                    │ activates relevant memories
    │                                    │
    └────────────────────────────────────┘
         Non-blocking, concurrent
```

---

## Architecture: Separate Memory Management Process

### **Like cbar (AR-based) Concurrent Architecture**

The memory process runs **independently**, observing the persona's activity and managing consolidation/activation based on patterns, not timers.

```
┌─────────────────────────────────────────────────────┐
│ PersonaUser Main Thread                             │
│ - Processes messages                                │
│ - Executes tasks                                    │
│ - Writes to inbox                                   │
│ - Updates WorkingMemory                             │
└─────────────────────────────────────────────────────┘
                    ↓ (inbox, workingMemory)
                    ↓ non-blocking peek
┌─────────────────────────────────────────────────────┐
│ MemoryConsolidationProcess (separate thread)        │
│ - Peeks at inbox (non-blocking)                     │
│ - Observes WorkingMemory changes                    │
│ - Detects patterns via cosine similarity            │
│ - Consolidates to LongTerm when patterns emerge     │
│ - Activates relevant memories when triggered        │
└─────────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────────┐
│ LongTermMemory (persistent, per-persona)            │
│ - SQLite: .continuum/personas/{id}/memory.sqlite    │
│ - Embeddings for cosine similarity                  │
│ - Append-only, no complex graph                     │
└─────────────────────────────────────────────────────┘
```

---

## Triggering: Event-Driven, Not Polling

**Not**: "Check every N seconds if should consolidate"

**Instead**: "Tasks/events remind the memory process"

### **Triggers for Consolidation**

Memory process wakes up when:

1. **Pattern detected in inbox**: Multiple similar messages arrive
2. **Working memory pressure**: Capacity approaching limit
3. **Repeated thought pattern**: Same concept appears multiple times
4. **Task completion**: End of conversation, task done
5. **Explicit signal**: Persona enters idle state

### **Triggers for Activation**

Memory process activates when:

1. **New message arrives**: Check if similar to long-term patterns
2. **Context shift**: Persona switches rooms/domains
3. **Pattern recognition**: Inbox items match consolidated patterns
4. **Low working memory**: Need to pull in relevant context

---

## Implementation: Separate Process/Worker

### **Memory Consolidation Worker**

```typescript
/**
 * MemoryConsolidationWorker - Runs as separate thread/worker
 *
 * Observes PersonaUser activity and manages memory consolidation/activation
 * Non-blocking, pattern-driven, uses cosine similarity
 */
export class MemoryConsolidationWorker {
  private readonly personaId: UUID;
  private readonly workingMemory: WorkingMemoryManager;
  private readonly longTermMemory: LongTermMemoryStore;
  private running: boolean = false;

  // Observables (non-blocking peek)
  private inboxObserver: InboxObserver;
  private memoryObserver: WorkingMemoryObserver;

  constructor(personaId: UUID) {
    this.personaId = personaId;
    this.workingMemory = new WorkingMemoryManager(personaId);
    this.longTermMemory = new LongTermMemoryStore(personaId);

    // Set up observers (peek at activity, don't block)
    this.inboxObserver = new InboxObserver(personaId);
    this.memoryObserver = new WorkingMemoryObserver(this.workingMemory);
  }

  /**
   * Start the worker (separate thread/process)
   */
  async start(): Promise<void> {
    this.running = true;

    // Run in background (non-blocking)
    this.serviceLoop();
  }

  /**
   * Main service loop - runs independently
   */
  private async serviceLoop(): Promise<void> {
    while (this.running) {
      // 1. Check triggers (non-blocking)
      const triggers = await this.checkTriggers();

      // 2. Consolidate if triggered
      if (triggers.shouldConsolidate) {
        await this.consolidate(triggers.reason);
      }

      // 3. Activate if triggered
      if (triggers.shouldActivate) {
        await this.activate(triggers.context);
      }

      // 4. Sleep briefly (non-blocking, yields to other threads)
      await this.sleep(100);  // 100ms, not blocking main thread
    }
  }

  /**
   * Check for consolidation/activation triggers
   * Non-blocking peek at inbox and memory state
   */
  private async checkTriggers(): Promise<TriggerState> {
    // Peek at inbox (non-blocking)
    const inboxItems = await this.inboxObserver.peek(10);

    // Peek at working memory (non-blocking)
    const recentThoughts = await this.memoryObserver.getRecent(20);

    // Detect patterns via cosine similarity (not hard-coded)
    const patterns = await this.detectPatterns(inboxItems, recentThoughts);

    return {
      shouldConsolidate: patterns.consolidationTriggered,
      shouldActivate: patterns.activationTriggered,
      reason: patterns.reason,
      context: patterns.context
    };
  }

  /**
   * Detect patterns using cosine similarity
   * No hard-coded enums, pure vector similarity
   */
  private async detectPatterns(
    inboxItems: QueueItem[],
    thoughts: WorkingMemoryEntry[]
  ): Promise<PatternDetection> {
    // 1. Extract embeddings from recent activity
    const inboxEmbeddings = await Promise.all(
      inboxItems.map(item => this.embed(item.content))
    );

    const thoughtEmbeddings = await Promise.all(
      thoughts.map(t => this.embed(t.thoughtContent))
    );

    // 2. Compute pairwise cosine similarities
    const similarities = this.computePairwiseSimilarities(
      [...inboxEmbeddings, ...thoughtEmbeddings]
    );

    // 3. Detect clusters (pattern = high similarity cluster)
    const clusters = this.detectClusters(similarities, {
      minSimilarity: 0.75,  // Cosine threshold
      minClusterSize: 3     // At least 3 similar items
    });

    // 4. Trigger consolidation if strong pattern emerges
    const consolidationTriggered = clusters.some(c => c.strength > 0.8);

    // 5. Trigger activation if inbox matches existing patterns
    const activationTriggered = await this.matchesLongTermPatterns(inboxEmbeddings);

    return {
      consolidationTriggered,
      activationTriggered,
      reason: consolidationTriggered ? `Cluster detected: ${clusters[0].representative}` : null,
      context: activationTriggered ? inboxEmbeddings[0] : null,
      patterns: clusters
    };
  }

  /**
   * Consolidate to long-term when pattern emerges
   */
  private async consolidate(reason: string): Promise<void> {
    console.log(`💾 [MemoryWorker] Consolidation triggered: ${reason}`);

    // 1. Get high-importance working memories
    const candidates = await this.workingMemory.recall({
      minImportance: 0.6,
      limit: 50
    });

    // 2. Encode and store (batch)
    const batch = await Promise.all(
      candidates.map(async (memory) => ({
        ...memory,
        embedding: await this.embed(memory.thoughtContent),
        consolidatedAt: Date.now()
      }))
    );

    await this.longTermMemory.appendBatch(batch);

    // 3. Clear consolidated from working memory
    await this.workingMemory.clearBatch(candidates.map(c => c.id));

    console.log(`💾 [MemoryWorker] Consolidated ${batch.length} memories`);
  }

  /**
   * Activate relevant long-term memories
   */
  private async activate(contextEmbedding: number[]): Promise<void> {
    console.log(`🔗 [MemoryWorker] Activation triggered`);

    // 1. Find similar in long-term (cosine similarity)
    const relevant = await this.longTermMemory.findSimilar(contextEmbedding, {
      limit: 5,
      threshold: 0.75
    });

    // 2. Load into working memory (decompression)
    for (const memory of relevant) {
      await this.workingMemory.store({
        ...memory,
        metadata: {
          source: 'long-term-activation',
          activatedAt: Date.now()
        }
      });
    }

    console.log(`🔗 [MemoryWorker] Activated ${relevant.length} memories`);
  }

  /**
   * Check if inbox matches existing long-term patterns
   */
  private async matchesLongTermPatterns(
    inboxEmbeddings: number[][]
  ): Promise<boolean> {
    for (const embedding of inboxEmbeddings) {
      const matches = await this.longTermMemory.findSimilar(embedding, {
        limit: 1,
        threshold: 0.8  // High threshold
      });

      if (matches.length > 0) {
        return true;  // Inbox matches a long-term pattern
      }
    }

    return false;
  }

  /**
   * Compute pairwise cosine similarities
   */
  private computePairwiseSimilarities(embeddings: number[][]): number[][] {
    const n = embeddings.length;
    const similarities: number[][] = Array(n).fill(0).map(() => Array(n).fill(0));

    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        similarities[i][j] = this.cosineSimilarity(embeddings[i], embeddings[j]);
        similarities[j][i] = similarities[i][j];  // Symmetric
      }
      similarities[i][i] = 1.0;  // Self-similarity
    }

    return similarities;
  }

  /**
   * Cosine similarity between two vectors
   */
  private cosineSimilarity(a: number[], b: number[]): number {
    const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
    const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
    const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));
    return dotProduct / (magnitudeA * magnitudeB);
  }

  /**
   * Detect clusters in similarity matrix
   */
  private detectClusters(
    similarities: number[][],
    options: { minSimilarity: number; minClusterSize: number }
  ): Cluster[] {
    // Simple clustering: find connected components above threshold
    const n = similarities.length;
    const visited = new Set<number>();
    const clusters: Cluster[] = [];

    for (let i = 0; i < n; i++) {
      if (visited.has(i)) continue;

      const cluster = this.expandCluster(i, similarities, visited, options.minSimilarity);

      if (cluster.length >= options.minClusterSize) {
        clusters.push({
          indices: cluster,
          strength: this.computeClusterStrength(cluster, similarities),
          representative: cluster[0]  // Could be centroid
        });
      }
    }

    return clusters.sort((a, b) => b.strength - a.strength);
  }

  /**
   * Expand cluster from seed node
   */
  private expandCluster(
    seed: number,
    similarities: number[][],
    visited: Set<number>,
    threshold: number
  ): number[] {
    const cluster: number[] = [];
    const queue: number[] = [seed];

    while (queue.length > 0) {
      const node = queue.shift()!;
      if (visited.has(node)) continue;

      visited.add(node);
      cluster.push(node);

      // Add neighbors above threshold
      for (let i = 0; i < similarities[node].length; i++) {
        if (!visited.has(i) && similarities[node][i] >= threshold) {
          queue.push(i);
        }
      }
    }

    return cluster;
  }

  /**
   * Compute cluster strength (average internal similarity)
   */
  private computeClusterStrength(cluster: number[], similarities: number[][]): number {
    if (cluster.length <= 1) return 0;

    let sum = 0;
    let count = 0;

    for (let i = 0; i < cluster.length; i++) {
      for (let j = i + 1; j < cluster.length; j++) {
        sum += similarities[cluster[i]][cluster[j]];
        count++;
      }
    }

    return count > 0 ? sum / count : 0;
  }

  private async embed(text: string): Promise<number[]> {
    // Use embedding service (Ollama, OpenAI, etc.)
    // For now, placeholder
    return [];
  }

  private async sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  async stop(): Promise<void> {
    this.running = false;
  }
}

interface TriggerState {
  shouldConsolidate: boolean;
  shouldActivate: boolean;
  reason: string | null;
  context: number[] | null;
}

interface PatternDetection {
  consolidationTriggered: boolean;
  activationTriggered: boolean;
  reason: string | null;
  context: number[] | null;
  patterns: Cluster[];
}

interface Cluster {
  indices: number[];
  strength: number;
  representative: number;
}
```

---

## Integration with PersonaUser

```typescript
// PersonaUser starts the memory worker (separate process)
export class PersonaUser extends AIUser {
  private memoryWorker: MemoryConsolidationWorker;

  async initialize(): Promise<void> {
    // ... existing initialization

    // Start memory consolidation worker (separate thread)
    this.memoryWorker = new MemoryConsolidationWorker(this.id);
    await this.memoryWorker.start();

    console.log(`🧠 [PersonaUser] Memory consolidation worker started`);
  }

  async destroy(): Promise<void> {
    // Stop memory worker
    await this.memoryWorker.stop();

    // ... existing cleanup
  }
}
```

---

## Observers: Non-Blocking Peek

```typescript
/**
 * InboxObserver - Peek at inbox without blocking
 */
export class InboxObserver {
  constructor(private personaId: UUID) {}

  /**
   * Peek at recent inbox items (non-blocking)
   */
  async peek(limit: number): Promise<QueueItem[]> {
    // Get PersonaInbox reference
    const persona = PersonaRegistry.get(this.personaId);
    if (!persona) return [];

    // Non-blocking peek
    return persona.inbox.peek(limit);  // Already non-blocking in PersonaInbox
  }
}

/**
 * WorkingMemoryObserver - Observe memory changes
 */
export class WorkingMemoryObserver {
  constructor(private workingMemory: WorkingMemoryManager) {}

  /**
   * Get recent thoughts (non-blocking)
   */
  async getRecent(limit: number): Promise<WorkingMemoryEntry[]> {
    return await this.workingMemory.recall({
      sortBy: 'recent',
      limit
    });
  }
}
```

---

## Key Principles

### 1. **Separate Thread/Process**
Not a method call - runs independently, non-blocking

### 2. **Event-Driven Triggers**
Not polling - triggered by patterns, not timers

### 3. **Cosine Similarity**
No hard-coded enums - pure vector similarity for pattern detection

### 4. **Non-Blocking Observation**
Peeks at inbox/memory without blocking main thread

### 5. **Pattern-Driven Consolidation**
Consolidates when patterns emerge, not on schedule

### 6. **Automatic Activation**
Activates relevant memories when similar patterns detected in inbox

---

## Benefits

1. **True RTOS**: Separate thread, non-blocking, concurrent
2. **Intelligent**: Detects patterns via cosine similarity, not hard-coded rules
3. **Efficient**: Only consolidates/activates when needed (pattern-driven)
4. **Scalable**: Doesn't slow down main persona thread
5. **Biological**: Mimics how brain consolidates memories during low activity

---

## Future: Small Model for Memory Management

```typescript
// Use a tiny local model to decide consolidation/activation
const memoryManagerAI = new PersonaUser({
  id: 'memory-manager',
  modelConfig: {
    provider: 'ollama',
    model: 'llama3.2:1b',  // Tiny, fast
    temperature: 0.3
  }
});

// It decides based on patterns
await memoryManagerAI.decide({
  prompt: `Inbox patterns: ${patterns}. Should I consolidate?`,
  options: ['yes', 'no']
});
```

---

## Implementation Path

**Phase 1**: Basic worker (pattern detection via cosine)
**Phase 2**: Non-blocking observers (peek inbox/memory)
**Phase 3**: Intelligent triggers (not just thresholds)
**Phase 4**: Small AI model for decision-making

This is the **true architecture** - separate, concurrent, pattern-driven memory management.
