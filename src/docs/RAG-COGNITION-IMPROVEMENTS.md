# RAG & Cognition System: Current State and Improvements

## Current Architecture Summary

### What Exists and Works

#### 1. Two-Dimensional RAG Budget (Flexbox-like)
**Location**: `ChatRAGBuilder.ts:689-748`

```
Dimension 1 (Vertical): Message Count
├── contextWindow / avgTokensPerMessage / targetUtilization
├── Clamped [5, 50]
└── Example: 128K window → ~400 messages max

Dimension 2 (Horizontal): Token Allocation
├── adjustedMaxTokens = min(requested, window - input - safety)
├── Safety margin: 100 tokens
└── Minimum floor: 500 tokens
```

#### 2. Hippocampus Memory (Two-Tier)
**Location**: `system/user/server/modules/cognitive/memory/Hippocampus.ts`

```
┌─────────────────────────────────────────┐
│         Working Memory (STM)             │
│  - In-memory ring buffer                 │
│  - ~100 entries max                      │
│  - Lost on restart                       │
└─────────────────┬───────────────────────┘
                  │ consolidate()
                  ▼
┌─────────────────────────────────────────┐
│         Long-Term Memory (LTM)           │
│  - Per-persona SQLite                    │
│  - .continuum/personas/{id}/memory.sqlite│
│  - Persists across restarts              │
└─────────────────────────────────────────┘
```

#### 3. Adaptive Consolidation Threshold
- Sigmoid function (activity-responsive)
- Exponential decay (time-responsive)
- Consolidates MORE during quiet periods

#### 4. RAG Builder Pattern
```typescript
RAGBuilder (abstract)
├── ChatRAGBuilder      // Chat rooms
├── CodebaseRAGBuilder  // Source code (partial)
└── Future: GameRAGBuilder, UIStateRAGBuilder, etc.
```

---

## What's Broken / Missing

### Critical Issues

#### 1. Memory Recall Integration is Fragile
**Problem**: `loadPrivateMemories()` accesses hippocampus through unsafe casting
```typescript
const persona = personaUser as any;
if (!persona.hippocampus) return [];
```

**Impact**: Memories silently fail to load, AI has no long-term knowledge

#### 2. Access Stats Not Persisted
**Location**: `Hippocampus.ts:210`
```typescript
// TODO: batch update in LTM
memory.accessCount++;
memory.lastAccessedAt = new Date();
// But never saved back to database!
```

**Impact**: LRU eviction doesn't work properly, access patterns lost on restart

#### 3. LTM Count Always Zero
**Location**: `Hippocampus.ts:464`
```typescript
// TODO: Query LTM count from database
ltmCount: 0  // Always returns 0
```

**Impact**: Can't monitor memory growth, no visibility into system health

#### 4. Activity Calculation Only Uses Chat
**Location**: `Hippocampus.ts:458`
```typescript
// TODO: aggregate all domains
const chatCapacity = this.persona.mind.workingMemory.getCapacity('chat');
```

**Impact**: Games, UI state, browsing activity not considered for consolidation

#### 5. Vector Embeddings Not Implemented
- Field exists in MemoryEntity but never populated
- No semantic search across memories
- Can't find conceptually related memories

---

## Proposed Architecture: RAG Allocation System v2

### Core Concept: Entity → RAG Pipeline

Everything that wants to be in AI context goes through a unified pipeline:

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌─────────────┐
│   Entity    │ => │  RAG Source  │ => │   Budget    │ => │  Context    │
│  (any type) │    │  (adapter)   │    │  Allocator  │    │  (to LLM)   │
└─────────────┘    └──────────────┘    └─────────────┘    └─────────────┘
     │                    │                   │
     │                    │                   │
  ChatMessage        ChatRAGSource      FlexboxBudget
  UIStateEntity      UIStateRAGSource   ├── flex-basis
  GameState          GameRAGSource      ├── flex-grow
  Memory             MemoryRAGSource    ├── max-width
  Artifact           ArtifactRAGSource  └── priority
```

### Flexbox-Inspired Budget Allocation

Each RAG source declares its space needs:

```typescript
interface RAGSourceConfig {
  name: string;

  // Flexbox-like properties
  flexBasis: number;      // Minimum tokens needed (like min-width)
  flexGrow: number;       // How much extra space to claim (0 = fixed, 1+ = flexible)
  flexShrink: number;     // How much to shrink under pressure (0 = rigid)
  maxTokens: number;      // Maximum tokens allowed (like max-width)

  // Priority for conflict resolution
  priority: 'critical' | 'high' | 'medium' | 'low';

  // Content characteristics
  compressible: boolean;  // Can content be summarized if tight?
  droppable: boolean;     // Can be entirely dropped if desperate?
}
```

### Example Source Configurations

```typescript
const RAG_SOURCE_CONFIGS: Record<string, RAGSourceConfig> = {
  // Identity is critical and fixed
  identity: {
    name: 'identity',
    flexBasis: 500,
    flexGrow: 0,
    flexShrink: 0,
    maxTokens: 800,
    priority: 'critical',
    compressible: false,
    droppable: false
  },

  // Conversation is the primary content, highly flexible
  conversation: {
    name: 'conversation',
    flexBasis: 2000,
    flexGrow: 2,         // Takes 2x extra space
    flexShrink: 1,       // Can shrink under pressure
    maxTokens: 50000,
    priority: 'high',
    compressible: true,  // Can summarize old messages
    droppable: false
  },

  // Memories support context, medium flexibility
  memories: {
    name: 'memories',
    flexBasis: 500,
    flexGrow: 1,
    flexShrink: 2,       // Shrinks faster than conversation
    maxTokens: 3000,
    priority: 'medium',
    compressible: true,
    droppable: true      // Can drop if critical
  },

  // UI State is new, low baseline but useful
  uiState: {
    name: 'uiState',
    flexBasis: 200,
    flexGrow: 0.5,
    flexShrink: 2,
    maxTokens: 1000,
    priority: 'medium',
    compressible: true,
    droppable: true
  },

  // Artifacts (images) are expensive but valuable
  artifacts: {
    name: 'artifacts',
    flexBasis: 0,        // Only if available
    flexGrow: 0,         // Fixed size when present
    flexShrink: 0,
    maxTokens: 10000,    // Images are big
    priority: 'high',
    compressible: false, // Can't compress images
    droppable: true      // Can drop under extreme pressure
  }
};
```

### Budget Allocation Algorithm

```typescript
class FlexboxBudgetAllocator {
  allocate(
    sources: Map<string, { config: RAGSourceConfig; content: string; tokens: number }>,
    contextWindow: number,
    reservedForOutput: number
  ): Map<string, number> {

    const available = contextWindow - reservedForOutput;
    const allocations = new Map<string, number>();

    // Phase 1: Allocate flex-basis to all sources
    let totalBasis = 0;
    for (const [name, source] of sources) {
      const basis = Math.min(source.config.flexBasis, source.tokens);
      allocations.set(name, basis);
      totalBasis += basis;
    }

    // Phase 2: If over budget, shrink
    if (totalBasis > available) {
      this.shrinkToFit(sources, allocations, available);
    }

    // Phase 3: If under budget, grow
    if (totalBasis < available) {
      this.growToFill(sources, allocations, available);
    }

    // Phase 4: If still over, drop low-priority droppables
    if (this.totalAllocated(allocations) > available) {
      this.dropLowPriority(sources, allocations, available);
    }

    return allocations;
  }

  private shrinkToFit(...) {
    // Sort by shrink factor (highest first)
    // Reduce proportionally to flex-shrink
    // Respect flex-basis as minimum
  }

  private growToFill(...) {
    // Sort by grow factor (highest first)
    // Distribute extra space proportionally
    // Respect max-tokens as ceiling
  }

  private dropLowPriority(...) {
    // Sort by priority (lowest first)
    // Drop droppable sources until fits
    // Never drop critical sources
  }
}
```

### Compression Strategies

When shrinking isn't enough, sources can compress:

```typescript
interface CompressionStrategy {
  // Summarize old messages
  summarizeConversation(messages: Message[], targetTokens: number): Message[];

  // Compress memories into key insights
  compressMemories(memories: Memory[], targetTokens: number): string;

  // Summarize UI state
  summarizeUIState(states: UIState[], targetTokens: number): string;
}
```

---

## New RAG Sources for Future Use Cases

### 1. UIStateRAGSource

```typescript
class UIStateRAGSource implements RAGSource {
  async load(contextId: UUID, userId: UUID): Promise<RAGSourceContent> {
    const states = await Commands.execute('data/list', {
      collection: 'ui_state',
      filter: { userId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: 5
    });

    return {
      content: states.items.map(s => s.summary).join('\n'),
      tokens: this.estimateTokens(states),
      metadata: { widgetCount: states.items.length }
    };
  }
}
```

### 2. GameStateRAGSource

```typescript
class GameStateRAGSource implements RAGSource {
  async load(contextId: UUID, gameId: UUID): Promise<RAGSourceContent> {
    const gameState = await Commands.execute('game/state/get', { gameId });

    return {
      content: this.formatGameState(gameState),
      tokens: this.estimateTokens(gameState),
      metadata: {
        gameType: gameState.type,
        moveCount: gameState.moveCount
      }
    };
  }

  formatGameState(state: GameState): string {
    // Format for LLM understanding
    return `
Game: ${state.type}
Turn: ${state.currentPlayer}
Move: ${state.moveCount}
Position: ${state.position}
Last Move: ${state.lastMove}
Status: ${state.status}
    `.trim();
  }
}
```

### 3. BrowsingContextRAGSource

```typescript
class BrowsingContextRAGSource implements RAGSource {
  async load(contextId: UUID, sessionId: UUID): Promise<RAGSourceContent> {
    const history = await Commands.execute('browsing/history/recent', {
      sessionId,
      limit: 10
    });

    const currentPage = await Commands.execute('browsing/current-page', {
      sessionId
    });

    return {
      content: this.formatBrowsingContext(currentPage, history),
      tokens: this.estimateTokens(currentPage, history),
      metadata: {
        currentUrl: currentPage.url,
        historyLength: history.length
      }
    };
  }
}
```

---

## Memory System Improvements

### Fix 1: Proper Memory Integration

```typescript
// In PersonaUser - expose hippocampus properly
class PersonaUser extends AIUser {
  private _hippocampus: Hippocampus;

  get hippocampus(): Hippocampus {
    return this._hippocampus;
  }

  // Or better - expose through a formal interface
  async recallMemories(params: MemoryRecallParams): Promise<MemoryEntity[]> {
    return this._hippocampus.recall(params);
  }
}

// In ChatRAGBuilder - use proper interface
private async loadPrivateMemories(...): Promise<PersonaMemory[]> {
  const personaUser = userDaemon.getPersonaUser(personaId);
  if (!personaUser) return [];

  // Use proper method, not unsafe cast
  const memories = await personaUser.recallMemories({
    minImportance: 0.6,
    limit: maxMemories,
    since: sevenDaysAgo
  });

  return this.convertToRAGFormat(memories);
}
```

### Fix 2: Persist Access Stats

```typescript
// In Hippocampus.recall()
async recall(params: MemoryRecallParams): Promise<MemoryEntity[]> {
  const memories = await this.queryLTM(params);

  // Update access stats
  const updates = memories.map(m => ({
    id: m.id,
    accessCount: m.accessCount + 1,
    lastAccessedAt: new Date()
  }));

  // Batch update in background (don't block recall)
  this.batchUpdateAccessStats(updates).catch(err =>
    console.error('Failed to update access stats:', err)
  );

  return memories;
}

private async batchUpdateAccessStats(updates: AccessUpdate[]): Promise<void> {
  const sql = `
    UPDATE memories
    SET access_count = ?, last_accessed_at = ?
    WHERE id = ?
  `;

  for (const update of updates) {
    await this.db.run(sql, [update.accessCount, update.lastAccessedAt, update.id]);
  }
}
```

### Fix 3: Real LTM Count

```typescript
async getStats(): Promise<HippocampusStats> {
  const ltmCount = await this.db.get<{ count: number }>(
    'SELECT COUNT(*) as count FROM memories'
  );

  return {
    stmCount: this.workingMemory.size,
    ltmCount: ltmCount?.count || 0,
    consolidationRate: this.consolidationRate,
    adaptiveThreshold: this.adaptiveThreshold.getCurrentThreshold()
  };
}
```

### Fix 4: Multi-Domain Activity Tracking

```typescript
private calculateActivityLevel(): number {
  const domains = ['chat', 'game', 'ui', 'browsing', 'code'];

  let totalActivity = 0;
  for (const domain of domains) {
    const capacity = this.persona.mind.workingMemory.getCapacity(domain);
    if (capacity) {
      totalActivity += capacity.used;
    }
  }

  // Normalize to messages/minute equivalent
  return totalActivity / domains.length / 10.0;
}
```

---

## Vector Embedding Integration

### Phase 1: Generate Embeddings on Consolidation

```typescript
// In Hippocampus.consolidate()
async consolidateToLTM(thought: Thought): Promise<MemoryEntity> {
  // Generate embedding for semantic search
  const embedding = await Commands.execute('ai/embedding/generate', {
    text: thought.content,
    model: 'text-embedding-3-small'
  });

  const memory: MemoryEntity = {
    id: generateUUID(),
    content: thought.content,
    embedding: embedding.vector,
    // ... other fields
  };

  await this.insertToLTM(memory);
  return memory;
}
```

### Phase 2: Semantic Recall

```typescript
async recallSemantic(query: string, limit: number = 10): Promise<MemoryEntity[]> {
  // Generate query embedding
  const queryEmbedding = await Commands.execute('ai/embedding/generate', {
    text: query
  });

  // Vector search in LTM
  return await Commands.execute('data/vector-search', {
    collection: 'persona_memories',
    personaId: this.personaId,
    vector: queryEmbedding.vector,
    limit
  });
}
```

---

## Implementation Phases

### Phase 1: Fix Critical Bugs
- [ ] Proper hippocampus interface in PersonaUser
- [ ] Persist access stats to database
- [ ] Real LTM count query
- [ ] Multi-domain activity calculation

### Phase 2: Flexbox Budget System
- [ ] RAGSourceConfig interface
- [ ] FlexboxBudgetAllocator class
- [ ] Migrate ChatRAGBuilder to use allocator
- [ ] Compression strategies

### Phase 3: New RAG Sources
- [ ] UIStateRAGSource
- [ ] GameStateRAGSource
- [ ] BrowsingContextRAGSource
- [ ] Register in RAGBuilderFactory

### Phase 4: Vector Embeddings
- [ ] Generate embeddings on consolidation
- [ ] Semantic recall method
- [ ] Vector index in SQLite (or external)

### Phase 5: Cross-Domain Intelligence
- [ ] Unified context from all sources
- [ ] Cross-widget awareness
- [ ] Temporal context (what happened before)

---

## Summary

The current system has solid foundations:
- Two-dimensional budget allocation
- Hippocampus STM/LTM pattern
- RAG builder abstraction

But needs work:
- Memory integration is fragile
- Several TODOs blocking full functionality
- No semantic search
- No support for new domains (UI state, games, etc.)

The proposed improvements:
- Flexbox-inspired budget negotiation
- Entity → RAG pipeline for any content type
- Proper memory persistence
- Vector embeddings for semantic recall
- Extensible for games, browsing, exports

This creates a universal RAG system where any entity can participate in AI context, with intelligent budget allocation ensuring the most relevant information gets priority.
