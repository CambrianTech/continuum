# RAG Memory Integration: PersonaUser Episodic Memory System

**Date**: 2025-01-24
**Status**: Design Phase
**Philosophy**: "Memory is not storage - it's retrieval-augmented context injection"

---

## The Vision: Context-Aware AI Citizens

**Core Insight**: PersonaUsers need memory, not just message history. Vector search enables semantic retrieval - "what's relevant?" not "what happened when?"

### What We're Building

PersonaUsers that remember and learn:
- **Episodic Memory**: Past conversations, interactions, events
- **Semantic Memory**: Facts, relationships, extracted knowledge
- **Procedural Memory**: Learned patterns, preferences, behaviors
- **Contextual Retrieval**: Semantic search finds relevant memories, not just recent ones
- **RAG-Enhanced Responses**: Context injection improves relevance and continuity

**Key Property**: Memory retrieval happens automatically during response generation, using the vector search infrastructure we just built.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    PersonaUser Autonomous Loop               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  1. Message arrives in inbox (priority-sorted)              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  2. Decide to engage (state-aware traffic management)       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  3. RETRIEVE RELEVANT MEMORIES (Vector Search)              │
│     - Query: current message content                        │
│     - Search: episodic + semantic memories                  │
│     - Return: top-k most relevant (k=5 default)             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  4. BUILD CONTEXT (Inject Memories into Prompt)             │
│     - System prompt (identity, behavior)                    │
│     - Retrieved memories (relevant past context)            │
│     - Recent messages (immediate context)                   │
│     - Current message (what to respond to)                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  5. Generate response (LLM with RAG-enhanced context)       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  6. CREATE MEMORY (Store interaction for future retrieval)  │
│     - Extract key information                               │
│     - Generate embedding                                    │
│     - Store in memory collection                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  7. Coordinate with other personas (thought broadcasting)   │
└─────────────────────────────────────────────────────────────┘
```

---

## Memory Entity Design

### Core Memory Entity

```typescript
// system/user/shared/MemoryTypes.ts

export interface MemoryEntity extends BaseEntity {
  readonly id: UUID;
  readonly personaId: UUID;        // Owner of this memory
  readonly memoryType: MemoryType; // episodic | semantic | procedural
  readonly content: string;        // The actual memory text
  readonly context?: {             // Optional structured context
    readonly roomId?: UUID;
    readonly conversationId?: UUID;
    readonly participantIds?: UUID[];
    readonly tags?: string[];
  };
  readonly sourceMessageId?: UUID; // Original message that created this memory
  readonly importance: number;     // 0.0-1.0 (affects retrieval ranking)
  readonly timestamp: number;      // When memory was created
  readonly lastAccessedAt?: number;// When last retrieved (for LRU)
  readonly accessCount: number;    // How many times retrieved
  readonly embedding?: number[];   // Vector embedding (optional - stored separately)
}

export type MemoryType = 'episodic' | 'semantic' | 'procedural';

export interface MemoryRetrievalResult {
  readonly memory: MemoryEntity;
  readonly relevanceScore: number; // Cosine similarity score
  readonly context?: string;       // Optional additional context
}

export interface MemoryCreationRequest {
  readonly personaId: UUID;
  readonly content: string;
  readonly memoryType: MemoryType;
  readonly context?: MemoryEntity['context'];
  readonly sourceMessageId?: UUID;
  readonly importance?: number;
}

export interface MemoryRetrievalRequest {
  readonly personaId: UUID;
  readonly queryText: string;
  readonly memoryTypes?: MemoryType[];  // Filter by type
  readonly k?: number;                  // Top-k results (default 5)
  readonly minRelevance?: number;       // Minimum similarity score
  readonly timeWindow?: {               // Optional time filter
    readonly start?: number;
    readonly end?: number;
  };
}
```

### Memory Types Explained

**1. Episodic Memory** (Past Experiences)
- Conversations: "Joel asked about vector search performance"
- Events: "Helped debug TypeScript compilation error"
- Interactions: "Collaborated with Helper AI on RAG design"
- **When created**: After meaningful interactions
- **Content format**: Narrative summary of interaction
- **Importance**: Based on conversation length, user engagement, outcomes

**2. Semantic Memory** (Facts and Knowledge)
- Facts: "Vector embeddings are 384 dimensions using all-minilm"
- Relationships: "Joel is the owner, prefers TypeScript over JavaScript"
- Preferences: "User likes concise explanations with code examples"
- **When created**: Extracted from conversations over time
- **Content format**: Declarative statements
- **Importance**: Based on frequency of reference, user corrections

**3. Procedural Memory** (How-To Knowledge)
- Patterns: "When user says 'fix this', read file first, then edit"
- Workflows: "For vector search: create docs → backfill → query"
- Learned behaviors: "User prefers seeing test results before deployment"
- **When created**: After repeated successful patterns
- **Content format**: Action sequences or decision rules
- **Importance**: Based on success rate, frequency of use

---

## Memory Creation Strategies

### Strategy 1: Immediate Memory (Simple, Phase 1)

**When**: After EVERY interaction where PersonaUser responds

```typescript
async createImmediateMemory(
  messageEntity: ChatMessageEntity,
  responseEntity: ChatMessageEntity
): Promise<MemoryEntity> {

  const content = `Conversation in ${messageEntity.roomId}:
User: ${messageEntity.content}
${this.displayName}: ${responseEntity.content}`;

  return await this.memoryManager.createMemory({
    personaId: this.id,
    content,
    memoryType: 'episodic',
    context: {
      roomId: messageEntity.roomId,
      conversationId: messageEntity.conversationId,
      participantIds: [messageEntity.authorId, this.id],
      tags: this.extractTags(messageEntity.content)
    },
    sourceMessageId: messageEntity.id,
    importance: this.calculateImportance(messageEntity, responseEntity)
  });
}
```

**Pros**: Simple, captures everything
**Cons**: High volume, potential noise

---

### Strategy 2: Summarized Memory (Better, Phase 2)

**When**: After conversation thread completes or periodically (e.g., every 5 messages)

```typescript
async createSummarizedMemory(
  messages: ChatMessageEntity[]
): Promise<MemoryEntity> {

  // Use LLM to summarize conversation
  const summary = await this.summarizeConversation(messages);

  // Extract key facts/learnings
  const extractedFacts = await this.extractFacts(summary);

  // Create episodic memory for the conversation
  const episodicMemory = await this.memoryManager.createMemory({
    personaId: this.id,
    content: summary,
    memoryType: 'episodic',
    context: {
      roomId: messages[0].roomId,
      conversationId: messages[0].conversationId,
      participantIds: [...new Set(messages.map(m => m.authorId))],
      tags: this.extractTags(summary)
    },
    sourceMessageId: messages[messages.length - 1].id,
    importance: this.calculateImportance(messages)
  });

  // Create semantic memories for extracted facts
  for (const fact of extractedFacts) {
    await this.memoryManager.createMemory({
      personaId: this.id,
      content: fact,
      memoryType: 'semantic',
      context: {
        roomId: messages[0].roomId
      },
      importance: 0.7
    });
  }

  return episodicMemory;
}
```

**Pros**: Reduced noise, better quality, extracts facts
**Cons**: More complex, requires LLM calls

---

### Strategy 3: Importance-Filtered Memory (Best, Phase 3)

**When**: Only create memories for important interactions

```typescript
async maybeCreateMemory(
  messageEntity: ChatMessageEntity,
  responseEntity: ChatMessageEntity
): Promise<MemoryEntity | null> {

  const importance = this.calculateImportance(messageEntity, responseEntity);

  // Only create memory if important enough
  if (importance < 0.5) {
    console.debug(`Skipping low-importance memory (${importance.toFixed(2)})`);
    return null;
  }

  // High importance: create immediately
  if (importance > 0.8) {
    return await this.createImmediateMemory(messageEntity, responseEntity);
  }

  // Medium importance: batch for summarization
  await this.pendingMemories.add(messageEntity, responseEntity);

  // Trigger summarization if batch full
  if (this.pendingMemories.size >= 5) {
    return await this.createSummarizedMemory(this.pendingMemories.getAll());
  }

  return null;
}

private calculateImportance(
  messageEntity: ChatMessageEntity,
  responseEntity: ChatMessageEntity
): number {
  let importance = 0.5; // Base

  // Direct mention/question: higher importance
  if (messageEntity.content.includes(this.displayName)) {
    importance += 0.2;
  }

  // Long conversation: higher importance
  if (messageEntity.content.length > 200 || responseEntity.content.length > 200) {
    importance += 0.1;
  }

  // Code/technical: higher importance
  if (messageEntity.content.includes('```') || /\bfunction\b|\bclass\b/.test(messageEntity.content)) {
    importance += 0.1;
  }

  // User feedback/correction: highest importance
  if (/thanks|thank you|good|excellent|perfect/i.test(messageEntity.content)) {
    importance += 0.2;
  }
  if (/wrong|incorrect|no|error|mistake/i.test(messageEntity.content)) {
    importance += 0.3;
  }

  return Math.min(1.0, importance);
}
```

**Pros**: Optimal noise/signal ratio, efficient storage
**Cons**: Most complex, risk of missing important context

---

## Memory Retrieval Integration

### PersonaMemoryManager Module

```typescript
// system/user/server/modules/PersonaMemoryManager.ts

import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { MemoryEntity, MemoryRetrievalRequest, MemoryRetrievalResult } from '../../shared/MemoryTypes';

const MEMORY_COLLECTION = 'persona_memories';

export class PersonaMemoryManager {
  private personaId: UUID;
  private cache: Map<string, MemoryRetrievalResult[]> = new Map();
  private cacheTTL: number = 60000; // 1 minute cache

  constructor(personaId: UUID) {
    this.personaId = personaId;
  }

  /**
   * Retrieve relevant memories using vector search
   */
  async retrieveMemories(request: MemoryRetrievalRequest): Promise<MemoryRetrievalResult[]> {
    // Check cache first
    const cacheKey = this.getCacheKey(request);
    const cached = this.cache.get(cacheKey);
    if (cached) {
      console.debug(`📦 Memory cache hit: ${cacheKey}`);
      return cached;
    }

    console.debug(`🔍 Retrieving memories for: "${request.queryText.substring(0, 50)}..."`);

    // Build filter for this persona's memories
    const filter: UniversalFilter = {
      personaId: { $eq: request.personaId }
    };

    // Optional: Filter by memory type
    if (request.memoryTypes && request.memoryTypes.length > 0) {
      filter.memoryType = { $in: request.memoryTypes };
    }

    // Optional: Time window filter
    if (request.timeWindow) {
      filter.timestamp = {};
      if (request.timeWindow.start) {
        filter.timestamp.$gte = request.timeWindow.start;
      }
      if (request.timeWindow.end) {
        filter.timestamp.$lte = request.timeWindow.end;
      }
    }

    // Vector search via DataDaemon
    const searchResult = await DataDaemon.vectorSearch({
      collection: MEMORY_COLLECTION,
      queryText: request.queryText,
      k: request.k || 5,
      filter
    });

    if (!searchResult.success || !searchResult.data) {
      console.error('❌ Memory retrieval failed:', searchResult.error);
      return [];
    }

    // Transform results
    const results: MemoryRetrievalResult[] = searchResult.data.results
      .filter(r => !request.minRelevance || r.score >= request.minRelevance)
      .map(r => ({
        memory: r.record as MemoryEntity,
        relevanceScore: r.score,
        context: this.buildContext(r.record as MemoryEntity)
      }));

    // Update access tracking (fire and forget)
    this.updateAccessTracking(results.map(r => r.memory.id));

    // Cache results
    this.cache.set(cacheKey, results);
    setTimeout(() => this.cache.delete(cacheKey), this.cacheTTL);

    console.debug(`✅ Retrieved ${results.length} relevant memories`);
    return results;
  }

  /**
   * Create new memory with embedding
   */
  async createMemory(request: MemoryCreationRequest): Promise<MemoryEntity> {
    const memory: MemoryEntity = {
      id: generateUUID(),
      personaId: request.personaId,
      memoryType: request.memoryType,
      content: request.content,
      context: request.context,
      sourceMessageId: request.sourceMessageId,
      importance: request.importance || 0.5,
      timestamp: Date.now(),
      accessCount: 0,
      createdAt: Date.now(),
      updatedAt: Date.now()
    };

    // Store memory (without embedding first)
    const storeResult = await DataDaemon.store({
      collection: MEMORY_COLLECTION,
      data: memory
    });

    if (!storeResult.success) {
      throw new Error(`Failed to store memory: ${storeResult.error}`);
    }

    // Generate and store embedding (asynchronous)
    this.generateEmbeddingAsync(memory.id, memory.content);

    return memory;
  }

  /**
   * Generate embedding asynchronously (fire and forget)
   */
  private async generateEmbeddingAsync(memoryId: UUID, content: string): Promise<void> {
    try {
      const result = await DataDaemon.generateEmbedding({
        collection: MEMORY_COLLECTION,
        recordId: memoryId,
        textField: 'content',
        text: content
      });

      if (!result.success) {
        console.error(`❌ Failed to generate embedding for memory ${memoryId}:`, result.error);
      }
    } catch (error) {
      console.error(`❌ Error generating embedding for memory ${memoryId}:`, error);
    }
  }

  /**
   * Update access tracking for retrieved memories
   */
  private async updateAccessTracking(memoryIds: UUID[]): Promise<void> {
    try {
      for (const id of memoryIds) {
        await DataDaemon.update({
          collection: MEMORY_COLLECTION,
          filter: { id: { $eq: id } },
          updates: {
            lastAccessedAt: Date.now(),
            $inc: { accessCount: 1 }
          }
        });
      }
    } catch (error) {
      console.error('❌ Failed to update memory access tracking:', error);
    }
  }

  /**
   * Build contextual information for memory
   */
  private buildContext(memory: MemoryEntity): string {
    const parts: string[] = [];

    if (memory.context?.roomId) {
      parts.push(`Room: ${memory.context.roomId}`);
    }

    if (memory.timestamp) {
      const relativeTime = this.getRelativeTime(memory.timestamp);
      parts.push(`Time: ${relativeTime}`);
    }

    if (memory.importance) {
      parts.push(`Importance: ${(memory.importance * 100).toFixed(0)}%`);
    }

    return parts.join(' | ');
  }

  /**
   * Get human-readable relative time
   */
  private getRelativeTime(timestamp: number): string {
    const now = Date.now();
    const diffMs = now - timestamp;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 60) return `${diffMins} minutes ago`;
    if (diffHours < 24) return `${diffHours} hours ago`;
    if (diffDays < 7) return `${diffDays} days ago`;
    return new Date(timestamp).toLocaleDateString();
  }

  /**
   * Generate cache key for retrieval request
   */
  private getCacheKey(request: MemoryRetrievalRequest): string {
    return `${request.personaId}:${request.queryText}:${request.memoryTypes?.join(',')}:${request.k}`;
  }

  /**
   * Clear cache (useful for testing)
   */
  clearCache(): void {
    this.cache.clear();
  }
}
```

---

## Context Injection Patterns

### Pattern 1: Simple Injection (Phase 1)

```typescript
async buildPrompt(
  messageEntity: ChatMessageEntity,
  recentMessages: ChatMessageEntity[]
): Promise<string> {

  // System prompt
  const systemPrompt = `You are ${this.displayName}, ${this.persona.description}`;

  // Retrieve relevant memories
  const memories = await this.memoryManager.retrieveMemories({
    personaId: this.id,
    queryText: messageEntity.content,
    k: 5,
    minRelevance: 0.3
  });

  // Inject memories
  const memoryContext = memories.length > 0
    ? `\n\nRelevant memories:\n${memories.map((m, i) =>
        `${i + 1}. ${m.memory.content} (${m.context})`
      ).join('\n')}`
    : '';

  // Recent messages
  const conversationContext = recentMessages
    .map(m => `${m.authorName}: ${m.content}`)
    .join('\n');

  // Build final prompt
  return `${systemPrompt}${memoryContext}

Recent conversation:
${conversationContext}

Current message:
${messageEntity.authorName}: ${messageEntity.content}

Please respond as ${this.displayName}:`;
}
```

---

### Pattern 2: Structured Injection (Phase 2)

```typescript
async buildStructuredPrompt(
  messageEntity: ChatMessageEntity,
  recentMessages: ChatMessageEntity[]
): Promise<{
  system: string;
  context: string;
  conversation: string;
  query: string;
}> {

  // Retrieve memories by type
  const episodicMemories = await this.memoryManager.retrieveMemories({
    personaId: this.id,
    queryText: messageEntity.content,
    memoryTypes: ['episodic'],
    k: 3
  });

  const semanticMemories = await this.memoryManager.retrieveMemories({
    personaId: this.id,
    queryText: messageEntity.content,
    memoryTypes: ['semantic'],
    k: 5
  });

  const proceduralMemories = await this.memoryManager.retrieveMemories({
    personaId: this.id,
    queryText: messageEntity.content,
    memoryTypes: ['procedural'],
    k: 2
  });

  return {
    system: `You are ${this.displayName}, ${this.persona.description}`,

    context: this.formatMemoryContext({
      episodic: episodicMemories,
      semantic: semanticMemories,
      procedural: proceduralMemories
    }),

    conversation: recentMessages
      .map(m => `${m.authorName}: ${m.content}`)
      .join('\n'),

    query: messageEntity.content
  };
}

private formatMemoryContext(memories: {
  episodic: MemoryRetrievalResult[];
  semantic: MemoryRetrievalResult[];
  procedural: MemoryRetrievalResult[];
}): string {
  const parts: string[] = [];

  if (memories.episodic.length > 0) {
    parts.push('## Past Experiences');
    memories.episodic.forEach((m, i) => {
      parts.push(`${i + 1}. ${m.memory.content} (${m.context})`);
    });
  }

  if (memories.semantic.length > 0) {
    parts.push('\n## Known Facts');
    memories.semantic.forEach((m, i) => {
      parts.push(`${i + 1}. ${m.memory.content}`);
    });
  }

  if (memories.procedural.length > 0) {
    parts.push('\n## Learned Patterns');
    memories.procedural.forEach((m, i) => {
      parts.push(`${i + 1}. ${m.memory.content}`);
    });
  }

  return parts.join('\n');
}
```

---

### Pattern 3: Adaptive Injection (Phase 3)

**Idea**: Adjust retrieval based on context complexity and persona state

```typescript
async buildAdaptivePrompt(
  messageEntity: ChatMessageEntity,
  recentMessages: ChatMessageEntity[]
): Promise<string> {

  const state = this.stateManager.getState();

  // Adjust retrieval parameters based on state
  let k = 5; // Default
  let minRelevance = 0.3;

  // High energy/attention: retrieve more memories
  if (state.energy > 0.7 && state.attention > 0.7) {
    k = 10;
    minRelevance = 0.2;
  }

  // Low energy: fewer, more relevant memories
  if (state.energy < 0.3) {
    k = 3;
    minRelevance = 0.5;
  }

  // Complex message: retrieve more context
  const complexity = this.estimateComplexity(messageEntity.content);
  if (complexity > 0.7) {
    k = Math.min(15, k + 5);
  }

  const memories = await this.memoryManager.retrieveMemories({
    personaId: this.id,
    queryText: messageEntity.content,
    k,
    minRelevance
  });

  return this.buildPrompt(messageEntity, recentMessages, memories);
}

private estimateComplexity(content: string): number {
  let complexity = 0.3; // Base

  // Long message: more complex
  if (content.length > 500) complexity += 0.2;
  if (content.length > 1000) complexity += 0.2;

  // Code: more complex
  if (content.includes('```')) complexity += 0.2;

  // Technical terms: more complex
  const technicalTerms = ['function', 'class', 'interface', 'type', 'async', 'await'];
  const termCount = technicalTerms.filter(term => content.includes(term)).length;
  complexity += termCount * 0.05;

  // Questions: more complex
  if (content.includes('?')) complexity += 0.1;
  if (content.split('?').length > 2) complexity += 0.1;

  return Math.min(1.0, complexity);
}
```

---

## Integration with Autonomous Loop

### Modified PersonaUser Processing Flow

```typescript
// system/user/server/PersonaUser.ts

export class PersonaUser extends AIUser {
  private memoryManager: PersonaMemoryManager;

  constructor(entity: UserEntity) {
    super(entity);
    this.memoryManager = new PersonaMemoryManager(this.id);
  }

  /**
   * Process messages (with RAG integration)
   */
  private async processMessages(messages: InboxMessage[]): Promise<void> {
    const startTime = Date.now();

    for (const message of messages) {
      await this.inbox.pop();

      const messageEntity = await this.loadMessageEntity(message.messageId);
      if (!messageEntity) continue;

      // === RAG INTEGRATION POINT 1: RETRIEVE MEMORIES ===
      const memories = await this.memoryManager.retrieveMemories({
        personaId: this.id,
        queryText: messageEntity.content,
        k: 5,
        minRelevance: 0.3
      });

      console.debug(`🧠 Retrieved ${memories.length} relevant memories for context`);

      // === RAG INTEGRATION POINT 2: BUILD CONTEXT ===
      const prompt = await this.buildPromptWithMemories(
        messageEntity,
        await this.getRecentMessages(messageEntity.roomId),
        memories
      );

      // === EXISTING: Generate response ===
      const response = await this.generateResponse(prompt);

      // === EXISTING: Coordinate with other personas ===
      const decision = await this.coordinate(messageEntity, response);

      if (decision.granted.includes(this.id)) {
        // === EXISTING: Send response ===
        const responseEntity = await this.sendMessage(
          messageEntity.roomId,
          response,
          messageEntity.id
        );

        // === RAG INTEGRATION POINT 3: CREATE MEMORY ===
        await this.memoryManager.createMemory({
          personaId: this.id,
          content: this.summarizeInteraction(messageEntity, responseEntity),
          memoryType: 'episodic',
          context: {
            roomId: messageEntity.roomId,
            conversationId: messageEntity.conversationId,
            participantIds: [messageEntity.authorId, this.id]
          },
          sourceMessageId: messageEntity.id,
          importance: this.calculateMemoryImportance(messageEntity, responseEntity)
        });

        console.debug(`💾 Memory created for interaction`);
      }
    }

    const durationMs = Date.now() - startTime;
    await this.stateManager.recordActivity(durationMs, messages.length);
  }

  /**
   * Build prompt with retrieved memories
   */
  private async buildPromptWithMemories(
    messageEntity: ChatMessageEntity,
    recentMessages: ChatMessageEntity[],
    memories: MemoryRetrievalResult[]
  ): Promise<string> {

    const systemPrompt = `You are ${this.displayName}.`;

    const memoryContext = memories.length > 0
      ? `\n\nRelevant past context:\n${memories.map((m, i) =>
          `${i + 1}. ${m.memory.content} (relevance: ${(m.relevanceScore * 100).toFixed(0)}%)`
        ).join('\n')}`
      : '';

    const conversationContext = recentMessages
      .map(m => `${m.authorName}: ${m.content}`)
      .join('\n');

    return `${systemPrompt}${memoryContext}

Recent conversation:
${conversationContext}

Current message:
${messageEntity.authorName}: ${messageEntity.content}

Respond as ${this.displayName}:`;
  }

  /**
   * Summarize interaction for memory storage
   */
  private summarizeInteraction(
    messageEntity: ChatMessageEntity,
    responseEntity: ChatMessageEntity
  ): string {
    // Simple summary (Phase 1)
    return `${messageEntity.authorName} said: "${messageEntity.content.substring(0, 200)}..."
I responded: "${responseEntity.content.substring(0, 200)}..."`;

    // TODO Phase 2: Use LLM to generate better summaries
  }

  /**
   * Calculate memory importance
   */
  private calculateMemoryImportance(
    messageEntity: ChatMessageEntity,
    responseEntity: ChatMessageEntity
  ): number {
    let importance = 0.5;

    // Direct mention: higher importance
    if (messageEntity.content.includes(this.displayName)) {
      importance += 0.2;
    }

    // Long interaction: higher importance
    if (messageEntity.content.length + responseEntity.content.length > 400) {
      importance += 0.1;
    }

    // Code/technical: higher importance
    if (messageEntity.content.includes('```')) {
      importance += 0.1;
    }

    return Math.min(1.0, importance);
  }
}
```

---

## Memory Management & Lifecycle

### Memory Consolidation (Phase 2+)

**Problem**: Too many small memories create noise and slow retrieval.

**Solution**: Periodically consolidate related memories into summaries.

```typescript
// Run as background task in autonomous loop
async consolidateMemories(): Promise<void> {
  console.debug('🔄 Starting memory consolidation...');

  // Get recent episodic memories (last 24 hours)
  const recentMemories = await DataDaemon.list({
    collection: MEMORY_COLLECTION,
    filter: {
      personaId: { $eq: this.id },
      memoryType: { $eq: 'episodic' },
      timestamp: { $gte: Date.now() - 86400000 }
    }
  });

  if (!recentMemories.success || recentMemories.data.length < 10) {
    return; // Not enough to consolidate
  }

  // Group by context (room, conversation)
  const groups = this.groupMemoriesByContext(recentMemories.data);

  for (const group of groups) {
    if (group.length < 5) continue;

    // Use LLM to create consolidated summary
    const summary = await this.summarizeMemoryGroup(group);

    // Create new consolidated memory
    await this.memoryManager.createMemory({
      personaId: this.id,
      content: summary,
      memoryType: 'episodic',
      context: group[0].context,
      importance: Math.max(...group.map(m => m.importance))
    });

    // Delete old individual memories
    for (const memory of group) {
      await DataDaemon.delete({
        collection: MEMORY_COLLECTION,
        filter: { id: { $eq: memory.id } }
      });
    }
  }

  console.debug('✅ Memory consolidation complete');
}
```

### Memory Pruning (Phase 3+)

**Problem**: Memory collection grows unbounded.

**Solution**: Prune low-importance, rarely-accessed memories.

```typescript
async pruneMemories(): Promise<void> {
  console.debug('🗑️ Starting memory pruning...');

  const cutoffDate = Date.now() - (30 * 86400000); // 30 days ago

  // Find candidates for pruning
  const candidates = await DataDaemon.list({
    collection: MEMORY_COLLECTION,
    filter: {
      personaId: { $eq: this.id },
      importance: { $lt: 0.5 },
      accessCount: { $lt: 3 },
      lastAccessedAt: { $lt: cutoffDate }
    }
  });

  if (!candidates.success || candidates.data.length === 0) {
    return;
  }

  // Delete low-value memories
  for (const memory of candidates.data) {
    await DataDaemon.delete({
      collection: MEMORY_COLLECTION,
      filter: { id: { $eq: memory.id } }
    });
  }

  console.debug(`✅ Pruned ${candidates.data.length} low-value memories`);
}
```

---

## Performance Optimization

### 1. Caching Strategy

```typescript
// Cache retrieval results for 1 minute
private cache: Map<string, MemoryRetrievalResult[]> = new Map();
private cacheTTL: number = 60000;
```

### 2. Batch Embedding Generation

```typescript
// Don't wait for embedding - generate asynchronously
this.generateEmbeddingAsync(memory.id, memory.content);
```

### 3. Incremental Retrieval

```typescript
// Start with small k, expand if needed
let k = 3;
let memories = await this.retrieveMemories({ k });

if (memories.length === 0 || memories[0].relevanceScore < 0.5) {
  // No good matches, try broader search
  k = 10;
  memories = await this.retrieveMemories({ k, minRelevance: 0.2 });
}
```

### 4. Lazy Loading

```typescript
// Only load memory content when needed
interface MemoryReference {
  id: UUID;
  relevanceScore: number;
}

// Retrieve references first
const refs = await this.retrieveMemoryReferences({ k: 10 });

// Load full content only for top results
const top3 = refs.slice(0, 3);
const memories = await this.loadMemoryContent(top3.map(r => r.id));
```

---

## CLI Commands for Testing

### Memory Creation

```bash
# Create test memory manually
./jtag memory/create \
  --personaId="helper-ai-uuid" \
  --content="Joel prefers TypeScript over JavaScript for type safety" \
  --memoryType="semantic" \
  --importance=0.8

# Batch create from conversation
./jtag memory/create-from-conversation \
  --roomId="general-uuid" \
  --startMessageId="msg-123" \
  --endMessageId="msg-456"
```

### Memory Retrieval

```bash
# Search memories
./jtag memory/search \
  --personaId="helper-ai-uuid" \
  --query="What does Joel prefer for coding?" \
  --k=5

# List all memories
./jtag memory/list \
  --personaId="helper-ai-uuid" \
  --memoryType="semantic"
```

### Memory Management

```bash
# Consolidate memories
./jtag memory/consolidate --personaId="helper-ai-uuid"

# Prune low-value memories
./jtag memory/prune --personaId="helper-ai-uuid" --dryRun=true

# Stats
./jtag memory/stats --personaId="helper-ai-uuid"
```

---

## Phased Implementation Plan

### Phase 1: Basic RAG (Weeks 1-2)
- ✅ Memory entity types
- Create `PersonaMemoryManager` module
- Implement simple memory creation (immediate after each response)
- Implement basic retrieval (vector search integration)
- Implement simple context injection (append memories to prompt)
- CLI commands for testing
- Integration tests

**Success Criteria**: PersonaUser can create and retrieve memories, responses show awareness of past context.

---

### Phase 2: Smart Memory Creation (Weeks 3-4)
- Implement summarization (use LLM to create memory summaries)
- Implement importance filtering (only create important memories)
- Implement fact extraction (semantic memories from conversations)
- Add memory type classification (auto-detect episodic vs semantic)
- Memory consolidation (periodic summarization of related memories)

**Success Criteria**: Memory quality improves, less noise, better retrieval relevance.

---

### Phase 3: Adaptive RAG (Weeks 5-6)
- Implement adaptive retrieval (adjust k based on context/state)
- Implement structured injection (separate episodic/semantic/procedural)
- Memory pruning (remove low-value memories)
- Performance optimization (caching, batching, lazy loading)
- Memory analytics (track access patterns, importance decay)

**Success Criteria**: System scales efficiently, memories remain relevant over time, retrieval performance optimized.

---

### Phase 4: Advanced Memory (Weeks 7+)
- Cross-persona memory sharing (shared semantic knowledge base)
- Memory learning (adjust importance based on usage)
- Procedural memory extraction (learn patterns from repeated behaviors)
- Multi-hop reasoning (chain memories for complex queries)
- Memory visualization (CLI/UI for exploring memory graph)

**Success Criteria**: PersonaUsers demonstrate long-term learning, share knowledge efficiently, exhibit sophisticated memory-based reasoning.

---

## Testing Strategy

### Unit Tests

```typescript
// tests/unit/PersonaMemoryManager.test.ts
describe('PersonaMemoryManager', () => {
  it('should create memory with embedding', async () => {
    const memory = await manager.createMemory({
      personaId: 'test-persona',
      content: 'Joel prefers TypeScript',
      memoryType: 'semantic',
      importance: 0.8
    });

    expect(memory.id).toBeDefined();
    expect(memory.content).toBe('Joel prefers TypeScript');
  });

  it('should retrieve relevant memories', async () => {
    const memories = await manager.retrieveMemories({
      personaId: 'test-persona',
      queryText: 'What language does Joel prefer?',
      k: 3
    });

    expect(memories.length).toBeGreaterThan(0);
    expect(memories[0].relevanceScore).toBeGreaterThan(0.5);
  });

  it('should cache retrieval results', async () => {
    await manager.retrieveMemories({ personaId: 'test', queryText: 'test', k: 3 });
    const cached = await manager.retrieveMemories({ personaId: 'test', queryText: 'test', k: 3 });

    // Second call should be much faster (cached)
    expect(cached).toBeDefined();
  });
});
```

### Integration Tests

```typescript
// tests/integration/rag-memory.test.ts
describe('RAG Memory Integration', () => {
  it('should create memory after PersonaUser responds', async () => {
    // Send message
    const message = await sendTestMessage('general', 'What is vector search?');

    // Wait for PersonaUser response
    await waitForResponse(message.id);

    // Check memory was created
    const memories = await DataDaemon.list({
      collection: 'persona_memories',
      filter: { sourceMessageId: { $eq: message.id } }
    });

    expect(memories.success).toBe(true);
    expect(memories.data.length).toBeGreaterThan(0);
  });

  it('should retrieve memories during response generation', async () => {
    // Create memory
    await createTestMemory({
      content: 'Joel prefers TypeScript for type safety',
      memoryType: 'semantic'
    });

    // Ask related question
    const message = await sendTestMessage('general', 'What language does Joel like?');

    // Wait for response
    const response = await waitForResponse(message.id);

    // Response should mention TypeScript (memory was retrieved)
    expect(response.content.toLowerCase()).toContain('typescript');
  });
});
```

### End-to-End Tests

```bash
# E2E workflow test
npm start
sleep 120

# Create memories
./jtag memory/create --personaId="helper-ai" \
  --content="Joel prefers concise explanations" \
  --memoryType="semantic" --importance=0.8

./jtag memory/create --personaId="helper-ai" \
  --content="Vector search uses all-minilm with 384 dimensions" \
  --memoryType="semantic" --importance=0.7

# Test retrieval
./jtag memory/search --personaId="helper-ai" \
  --query="How should I explain things to Joel?"

# Should return: "Joel prefers concise explanations" with high relevance

# Test in conversation
./jtag chat/send --room="general" \
  --message="Helper AI, how does vector search work?"

# Wait for response
sleep 10

# Export conversation
./jtag chat/export --room="general" --limit=5

# Response should demonstrate memory of vector search details
```

---

## Success Metrics

### Memory Quality
- **Coverage**: % of important interactions that create memories
- **Relevance**: Average similarity score of retrieved memories
- **Precision**: % of retrieved memories that are actually relevant
- **Recall**: % of relevant memories that are retrieved

### System Performance
- **Retrieval latency**: p95 < 500ms for k=5
- **Creation latency**: p95 < 200ms (with async embedding)
- **Storage efficiency**: < 1KB per memory average
- **Cache hit rate**: > 30% for repeated queries

### User Experience
- **Context continuity**: PersonaUsers reference past conversations
- **Knowledge retention**: PersonaUsers remember user preferences
- **Improved relevance**: Responses demonstrate long-term learning
- **Natural conversation**: Fewer repetitive explanations

---

## Philosophy Alignment

**Memory is not storage - it's retrieval-augmented context injection.**

Key principles:
- Memories are created selectively (importance-filtered)
- Retrieval is semantic (relevance, not recency)
- Context is adaptive (state-aware, complexity-aware)
- Management is autonomous (consolidation, pruning)
- Integration is transparent (no user-visible changes)

**The Goal**: PersonaUsers that remember, learn, and grow more helpful over time - without manual intervention.

---

**Next Steps**: Begin Phase 1 implementation with `PersonaMemoryManager` module and basic memory creation/retrieval integration.
