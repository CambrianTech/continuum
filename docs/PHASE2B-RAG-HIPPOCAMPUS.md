# Phase 2B: RAG Hippocampus - Memory, Budget, Interrogation

**Date**: 2025-01-24
**Status**: Design Phase
**Philosophy**: "The universal context builder with full transparency and sophisticated memory"

---

## The Vision: RAG as the Hippocampus

The **hippocampus** is the brain's memory consolidation center - it retrieves relevant memories, packages them with current context, and sends them to the cortex for processing. Our RAG system does the same for PersonaUsers.

**Current State**:
- ✅ Basic token calculation (estimates)
- ✅ Message history loading
- ✅ Artifacts extraction
- ❌ No vector memory integration
- ❌ No media token calculation
- ❌ No interrogation capability
- ❌ No per-component tracking

**Target State**:
- ✅ Vector memory retrieval (semantic search)
- ✅ Precise token tracking (per component)
- ✅ Media token calculation (vision models)
- ✅ Full interrogation (logs + CLI)
- ✅ Circuit breakers & caching
- ✅ Batch optimizations

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    RAG HIPPOCAMPUS SYSTEM                    │
│         (Universal Context Builder for PersonaUser)          │
└─────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ RAGMemory        │  │ RAGBudget        │  │ RAGInspector     │
│ Integrator       │  │ Tracker          │  │                  │
│                  │  │                  │  │ - Logs           │
│ - Vector search  │  │ - Token counting │  │ - CLI commands   │
│ - Episodic/      │  │ - Per-component  │  │ - Real-time view │
│   semantic       │  │ - Media calc     │  │ - History query  │
│ - Batch embed    │  │ - Safety margin  │  └──────────────────┘
│ - Circuit breaker│  │ - Overflow check │
│ - LRU cache      │  └──────────────────┘
└──────────────────┘
         │                    │
         └────────────────────┼────────────────────┘
                              │
                              ▼
                   ┌─────────────────────┐
                   │   ChatRAGBuilder    │
                   │   (Enhanced)        │
                   │                     │
                   │   buildContext()    │
                   │   ↓                 │
                   │   RAGContext        │
                   │   + RAGBudget       │
                   │   + RAGSnapshot     │
                   └─────────────────────┘
                              │
                              ▼
                   ┌─────────────────────┐
                   │   PersonaUser       │
                   │   generate()        │
                   └─────────────────────┘
```

---

## Component 1: RAGBudgetTracker

**Purpose**: Track token usage per component with precision

### Interface

```typescript
// system/rag/shared/RAGBudgetTracker.ts

export interface RAGBudgetComponent {
  readonly name: string;
  readonly tokens: number;
  readonly items: number; // Count (messages, memories, images)
  readonly breakdown?: string[]; // Details
}

export interface RAGBudget {
  // Context window
  readonly contextWindow: number; // Model's max (e.g., 128000)

  // Token usage breakdown
  readonly components: {
    readonly systemPrompt: RAGBudgetComponent;
    readonly memories: RAGBudgetComponent;
    readonly recentMessages: RAGBudgetComponent;
    readonly media: RAGBudgetComponent;
    readonly currentMessage: RAGBudgetComponent;
  };

  // Calculated totals
  readonly used: number; // Sum of all components
  readonly reserved: number; // For completion + safety
  readonly remaining: number; // contextWindow - used - reserved
  readonly percentUsed: number; // (used / contextWindow) * 100

  // Safety checks
  readonly isOverBudget: boolean;
  readonly warnings: string[];
}

export class RAGBudgetTracker {
  private modelId: string;
  private contextWindow: number;
  private components: Map<string, RAGBudgetComponent> = new Map();
  private reservedTokens: number = 0;

  constructor(modelId: string, maxCompletionTokens: number = 3000) {
    this.modelId = modelId;
    this.contextWindow = this.getContextWindow(modelId);
    this.reservedTokens = maxCompletionTokens + this.getSafetyMargin();
  }

  /**
   * Add component with actual token count
   */
  addComponent(name: string, tokens: number, items: number, breakdown?: string[]): void {
    this.components.set(name, { name, tokens, items, breakdown });
  }

  /**
   * Calculate total budget state
   */
  getBudget(): RAGBudget {
    const systemPrompt = this.components.get('systemPrompt') || { name: 'systemPrompt', tokens: 0, items: 0 };
    const memories = this.components.get('memories') || { name: 'memories', tokens: 0, items: 0 };
    const recentMessages = this.components.get('recentMessages') || { name: 'recentMessages', tokens: 0, items: 0 };
    const media = this.components.get('media') || { name: 'media', tokens: 0, items: 0 };
    const currentMessage = this.components.get('currentMessage') || { name: 'currentMessage', tokens: 0, items: 0 };

    const used = systemPrompt.tokens + memories.tokens + recentMessages.tokens + media.tokens + currentMessage.tokens;
    const remaining = this.contextWindow - used - this.reservedTokens;
    const percentUsed = (used / this.contextWindow) * 100;

    const warnings: string[] = [];
    const isOverBudget = remaining < 0;

    if (isOverBudget) {
      warnings.push(`⚠️  OVER BUDGET by ${Math.abs(remaining)} tokens!`);
    }

    if (percentUsed > 80) {
      warnings.push(`⚠️  Using ${percentUsed.toFixed(1)}% of context window (>80%)`);
    }

    if (memories.tokens === 0 && memories.items === 0) {
      warnings.push('ℹ️  No memories retrieved (vector search may have failed)');
    }

    return {
      contextWindow: this.contextWindow,
      components: {
        systemPrompt,
        memories,
        recentMessages,
        media,
        currentMessage
      },
      used,
      reserved: this.reservedTokens,
      remaining,
      percentUsed,
      isOverBudget,
      warnings
    };
  }

  /**
   * Get formatted budget report for logging
   */
  getReport(): string {
    const budget = this.getBudget();

    const lines: string[] = [];
    lines.push(`\n┌─ RAG BUDGET: ${this.modelId} ─┐`);
    lines.push(`│ Context Window: ${budget.contextWindow.toLocaleString()} tokens`);
    lines.push(`│`);
    lines.push(`│ COMPONENTS:`);
    lines.push(`│   System Prompt:    ${this.formatTokens(budget.components.systemPrompt.tokens)}`);
    lines.push(`│   Memories:         ${this.formatTokens(budget.components.memories.tokens)} (${budget.components.memories.items} items)`);
    lines.push(`│   Recent Messages:  ${this.formatTokens(budget.components.recentMessages.tokens)} (${budget.components.recentMessages.items} msgs)`);
    lines.push(`│   Media:            ${this.formatTokens(budget.components.media.tokens)} (${budget.components.media.items} images)`);
    lines.push(`│   Current Message:  ${this.formatTokens(budget.components.currentMessage.tokens)}`);
    lines.push(`│   ─────────────────────────────`);
    lines.push(`│   TOTAL USED:       ${this.formatTokens(budget.used)} (${budget.percentUsed.toFixed(1)}%)`);
    lines.push(`│`);
    lines.push(`│   Reserved:         ${this.formatTokens(budget.reserved)} (completion + safety)`);
    lines.push(`│   ─────────────────────────────`);
    lines.push(`│   REMAINING:        ${this.formatTokens(budget.remaining)}`);

    if (budget.warnings.length > 0) {
      lines.push(`│`);
      lines.push(`│ WARNINGS:`);
      budget.warnings.forEach(w => lines.push(`│   ${w}`));
    }

    lines.push(`└────────────────────────────────┘`);

    return lines.join('\n');
  }

  private formatTokens(tokens: number): string {
    return tokens.toLocaleString().padStart(8);
  }

  private getContextWindow(modelId: string): number {
    const contextWindows: Record<string, number> = {
      'gpt-4': 8192,
      'gpt-4-turbo': 128000,
      'gpt-4o': 128000,
      'claude-3-5-sonnet': 200000,
      'qwen2.5:7b': 128000,
      'llama3.1:70b': 128000,
      'deepseek-chat': 64000
    };
    return contextWindows[modelId] || 8192;
  }

  private getSafetyMargin(): number {
    // 10% of context window or 1000 tokens, whichever is larger
    return Math.max(Math.floor(this.contextWindow * 0.1), 1000);
  }
}
```

### Usage in ChatRAGBuilder

```typescript
async buildContext(...): Promise<RAGContext> {
  const budgetTracker = new RAGBudgetTracker(
    options?.modelId || 'unknown',
    options?.maxTokens || 3000
  );

  // 1. System prompt
  const systemPrompt = await this.buildSystemPrompt(...);
  const systemPromptTokens = this.estimateTokens(systemPrompt);
  budgetTracker.addComponent('systemPrompt', systemPromptTokens, 1);

  // 2. Vector memory retrieval
  const memories = await this.retrieveMemories(...);
  const memoryTokens = memories.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);
  budgetTracker.addComponent('memories', memoryTokens, memories.length,
    memories.map(m => `${m.memory.memoryType}: ${m.memory.content.substring(0, 50)}...`)
  );

  // 3. Recent messages
  const messages = await this.loadMessages(...);
  const messageTokens = messages.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);
  budgetTracker.addComponent('recentMessages', messageTokens, messages.length);

  // 4. Media
  const media = await this.extractMedia(...);
  const mediaTokens = this.calculateMediaTokens(media, options?.modelId);
  budgetTracker.addComponent('media', mediaTokens, media.length,
    media.map(m => `${m.type}: ${m.url}`)
  );

  // 5. Current message
  const currentMessageTokens = this.estimateTokens(options?.currentMessage?.content || '');
  budgetTracker.addComponent('currentMessage', currentMessageTokens, 1);

  // Get final budget
  const budget = budgetTracker.getBudget();

  // Log detailed report
  console.log(budgetTracker.getReport());

  // Check for over-budget
  if (budget.isOverBudget) {
    console.error('❌ RAG context exceeds model capacity! Trimming...');
    // Trim logic here
  }

  return {
    ...ragContext,
    budget // Include budget in context
  };
}
```

---

## Component 2: RAGInspector - Interrogation Interface

**Purpose**: Query and inspect RAG state both in logs and via CLI

### CLI Commands

```bash
# 1. Inspect current RAG state for a persona
./jtag rag/inspect --personaId="helper-ai" --roomId="general"

# Output:
# ┌─ RAG INSPECTION: Helper AI in general ─┐
# │ Model: qwen2.5:7b
# │ Context Window: 128,000 tokens
# │
# │ COMPONENTS:
# │   System Prompt:    1,245 tokens
# │   Memories:         3,890 tokens (5 items)
# │     - episodic: "Discussed vector search performance..." (score: 0.82)
# │     - semantic: "Joel prefers TypeScript over JavaScript" (score: 0.76)
# │     - episodic: "Helped debug embedding timeouts..." (score: 0.71)
# │     - semantic: "Vector embeddings are 384 dimensions" (score: 0.68)
# │     - procedural: "When testing, run npm start first" (score: 0.65)
# │   Recent Messages:  8,120 tokens (20 msgs)
# │   Media:            2,560 tokens (2 images)
# │   Current Message:    245 tokens
# │   ─────────────────────────────────────
# │   TOTAL USED:      16,060 tokens (12.5%)
# │
# │   Reserved:        13,000 tokens (completion + safety)
# │   ─────────────────────────────────────
# │   REMAINING:       98,940 tokens
# │
# │ STATUS: ✅ Within budget, healthy margin
# └──────────────────────────────────────────┘

# 2. Dry-run budget calculation (doesn't build context)
./jtag rag/budget --modelId="qwen2.5:7b" --messages=20 --memories=5 --images=2

# Output:
# Estimated Budget:
#   System Prompt:    ~1,200 tokens
#   Memories (5):     ~4,000 tokens
#   Messages (20):    ~8,000 tokens
#   Images (2):       ~2,600 tokens
#   ─────────────────────────────
#   Total:            ~15,800 tokens (12.3%)
#   Reserved:         ~13,000 tokens
#   Remaining:        ~99,200 tokens
#
# ✅ Budget looks healthy

# 3. Explain context used for a past response
./jtag rag/explain --messageId="abc123"

# Output:
# ┌─ RAG CONTEXT for message #abc123 ─┐
# │ Persona: Helper AI
# │ Room: general
# │ Timestamp: 2025-01-24 11:30:15
# │
# │ CONTEXT COMPONENTS:
# │
# │ 1. SYSTEM PROMPT (1,234 tokens):
# │    "You are Helper AI, a coding assistant..."
# │
# │ 2. RETRIEVED MEMORIES (5 items, 3,890 tokens):
# │    Memory #1 (episodic, score: 0.82):
# │    "Discussed vector search performance with Joel.
# │     He was concerned about timeout handling..."
# │
# │    Memory #2 (semantic, score: 0.76):
# │    "Joel prefers TypeScript over JavaScript for
# │     type safety and better tooling..."
# │
# │    [... 3 more memories ...]
# │
# │ 3. RECENT MESSAGES (20 msgs, 8,120 tokens):
# │    [Shows recent conversation]
# │
# │ 4. MEDIA (2 images, 2,560 tokens):
# │    - screenshot.png (1,280 tokens)
# │    - diagram.png (1,280 tokens)
# │
# │ 5. USER MESSAGE (245 tokens):
# │    "How should I handle embedding timeouts?"
# │
# │ RESPONSE GENERATED:
# │   Model: qwen2.5:7b
# │   Completion Tokens: 487
# │   Total Tokens: 16,547
# │   Duration: 3.2s
# └──────────────────────────────────────┘

# 4. Real-time monitoring (watch mode)
./jtag rag/monitor --personaId="helper-ai" --follow

# Output (updates live):
# [11:30:15] RAG BUILD: Helper AI
#   ├─ System Prompt: 1,234 tokens
#   ├─ Memories: 3,890 tokens (5 items)
#   ├─ Messages: 8,120 tokens (20 msgs)
#   ├─ Media: 2,560 tokens (2 images)
#   └─ Total: 15,804 tokens (12.3%)
#
# [11:30:18] RESPONSE: 487 tokens generated
#
# [11:30:25] RAG BUILD: Helper AI
#   ├─ System Prompt: 1,234 tokens
#   ├─ Memories: 4,120 tokens (6 items) ← Changed!
#   ├─ Messages: 8,365 tokens (21 msgs)
#   ├─ Media: 0 tokens
#   └─ Total: 13,719 tokens (10.7%)

# 5. Historical analysis
./jtag rag/stats --personaId="helper-ai" --last=24h

# Output:
# RAG Statistics for Helper AI (last 24 hours):
#
#   Total Contexts Built: 47
#   Average Token Usage: 14,532 tokens (11.3%)
#   Max Token Usage: 28,910 tokens (22.6%)
#   Min Token Usage: 3,245 tokens (2.5%)
#
#   Component Breakdown (average):
#     System Prompt:    1,234 tokens (8.5%)
#     Memories:         4,123 tokens (28.4%)
#     Messages:         7,892 tokens (54.3%)
#     Media:           1,283 tokens (8.8%)
#
#   Memory Retrieval:
#     Total Memories Retrieved: 235
#     Average per Context: 5.0
#     Top Memory Types: episodic (60%), semantic (35%), procedural (5%)
#     Average Relevance Score: 0.74
#
#   Warnings:
#     Over-budget: 0 times
#     >80% usage: 3 times
#     No memories retrieved: 2 times (circuit breaker)
```

### Implementation

```typescript
// commands/rag/inspect/server/RAGInspectServerCommand.ts

export class RAGInspectServerCommand extends CommandBase<RAGInspectParams, RAGInspectResult> {
  async execute(params: RAGInspectParams): Promise<RAGInspectResult> {
    // Build context for persona
    const ragBuilder = RAGBuilderFactory.getBuilder('chat');
    const context = await ragBuilder.buildContext(
      params.roomId,
      params.personaId
    );

    // Get budget from context
    const budget = (context as any).budget as RAGBudget;

    // Format detailed report
    const report = this.formatInspectionReport(context, budget);

    return createRAGInspectResultFromParams(params, {
      success: true,
      report,
      budget,
      snapshot: {
        personaId: params.personaId,
        roomId: params.roomId,
        timestamp: Date.now(),
        components: budget.components,
        warnings: budget.warnings
      }
    });
  }

  private formatInspectionReport(context: RAGContext, budget: RAGBudget): string {
    const lines: string[] = [];

    lines.push(`\n┌─ RAG INSPECTION: ${context.identity.name} in ${context.contextId} ─┐`);
    lines.push(`│ Model: ${(context as any).modelId || 'unknown'}`);
    lines.push(`│ Context Window: ${budget.contextWindow.toLocaleString()} tokens`);
    lines.push(`│`);
    lines.push(`│ COMPONENTS:`);

    // System Prompt
    lines.push(`│   System Prompt:    ${this.formatTokens(budget.components.systemPrompt.tokens)}`);

    // Memories (with details)
    lines.push(`│   Memories:         ${this.formatTokens(budget.components.memories.tokens)} (${budget.components.memories.items} items)`);
    if (budget.components.memories.breakdown) {
      budget.components.memories.breakdown.forEach(b => {
        lines.push(`│     - ${b}`);
      });
    }

    // Recent Messages
    lines.push(`│   Recent Messages:  ${this.formatTokens(budget.components.recentMessages.tokens)} (${budget.components.recentMessages.items} msgs)`);

    // Media
    lines.push(`│   Media:            ${this.formatTokens(budget.components.media.tokens)} (${budget.components.media.items} images)`);
    if (budget.components.media.breakdown) {
      budget.components.media.breakdown.forEach(b => {
        lines.push(`│     - ${b}`);
      });
    }

    // Current Message
    lines.push(`│   Current Message:  ${this.formatTokens(budget.components.currentMessage.tokens)}`);

    lines.push(`│   ─────────────────────────────────────`);
    lines.push(`│   TOTAL USED:       ${this.formatTokens(budget.used)} (${budget.percentUsed.toFixed(1)}%)`);
    lines.push(`│`);
    lines.push(`│   Reserved:         ${this.formatTokens(budget.reserved)} (completion + safety)`);
    lines.push(`│   ─────────────────────────────────────`);
    lines.push(`│   REMAINING:        ${this.formatTokens(budget.remaining)}`);

    // Warnings
    if (budget.warnings.length > 0) {
      lines.push(`│`);
      lines.push(`│ WARNINGS:`);
      budget.warnings.forEach(w => lines.push(`│   ${w}`));
    } else {
      lines.push(`│`);
      lines.push(`│ STATUS: ✅ Within budget, healthy margin`);
    }

    lines.push(`└──────────────────────────────────────────┘`);

    return lines.join('\n');
  }

  private formatTokens(tokens: number): string {
    return tokens.toLocaleString().padStart(8);
  }
}
```

---

## Component 3: RAGMemoryIntegrator

**Purpose**: Vector search integration with circuit breakers and caching

```typescript
// system/rag/server/RAGMemoryIntegrator.ts

import { PersonaMemoryManager } from '../../user/server/modules/PersonaMemoryManager';
import type { MemoryRetrievalResult } from '../../user/shared/MemoryTypes';

export class RAGMemoryIntegrator {
  private memoryManager: PersonaMemoryManager;
  private cache: LRUCache<string, MemoryRetrievalResult[]>;
  private circuitBreaker: {
    failures: number;
    threshold: number;
    fallbackMode: boolean;
    lastFailure: number;
  };

  constructor(personaId: UUID) {
    this.memoryManager = new PersonaMemoryManager(personaId);
    this.cache = new LRUCache({ max: 100, ttl: 60000 }); // 1 minute
    this.circuitBreaker = {
      failures: 0,
      threshold: 3,
      fallbackMode: false,
      lastFailure: 0
    };
  }

  /**
   * Retrieve memories with circuit breaker and caching
   */
  async retrieveMemories(
    queryText: string,
    k: number,
    options?: {
      memoryTypes?: MemoryType[];
      minRelevance?: number;
      tokenBudget?: number; // Dynamic k based on budget
    }
  ): Promise<MemoryRetrievalResult[]> {
    // Check circuit breaker
    if (this.circuitBreaker.fallbackMode) {
      console.warn('⚠️  Memory circuit breaker OPEN - using recent memories fallback');
      return await this.fallbackRecent(k);
    }

    // Check cache
    const cacheKey = `${queryText}:${k}:${options?.memoryTypes?.join(',')}`;
    const cached = this.cache.get(cacheKey);
    if (cached) {
      console.debug('📦 Memory cache HIT');
      return cached;
    }

    try {
      // Try vector search
      const memories = await Promise.race([
        this.memoryManager.retrieveMemories({
          personaId: this.memoryManager['personaId'],
          queryText,
          k,
          memoryTypes: options?.memoryTypes,
          minRelevance: options?.minRelevance
        }),
        this.timeout(10000) // 10 second timeout
      ]);

      // Success - reset circuit breaker
      this.circuitBreaker.failures = 0;

      // Cache results
      this.cache.set(cacheKey, memories);

      return memories;

    } catch (error) {
      console.error('❌ Memory retrieval failed:', error);

      // Increment circuit breaker
      this.circuitBreaker.failures++;

      if (this.circuitBreaker.failures >= this.circuitBreaker.threshold) {
        // Open circuit
        this.circuitBreaker.fallbackMode = true;
        this.circuitBreaker.lastFailure = Date.now();

        console.warn(`🔴 Memory circuit breaker OPENED (${this.circuitBreaker.failures} failures)`);

        // Try to close after 5 minutes
        setTimeout(() => {
          this.circuitBreaker.fallbackMode = false;
          this.circuitBreaker.failures = 0;
          console.log('🟢 Memory circuit breaker CLOSED (cooldown complete)');
        }, 300000);
      }

      // Fallback to recent memories
      return await this.fallbackRecent(k);
    }
  }

  /**
   * Fallback: Most recent memories (when vector search fails)
   */
  private async fallbackRecent(k: number): Promise<MemoryRetrievalResult[]> {
    // Query most recent memories without vector search
    const recent = await DataDaemon.list({
      collection: 'persona_memories',
      filter: { personaId: { $eq: this.memoryManager['personaId'] } },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: k
    });

    if (!recent.success || !recent.data) {
      return [];
    }

    // Convert to MemoryRetrievalResult format
    return recent.data.map(m => ({
      memory: m as MemoryEntity,
      relevanceScore: 0.5, // Default score for fallback
      context: 'Recent memory (fallback mode)'
    }));
  }

  private timeout(ms: number): Promise<never> {
    return new Promise((_, reject) =>
      setTimeout(() => reject(new Error('Memory retrieval timeout')), ms)
    );
  }
}
```

---

## Component 4: Media Token Calculation

**Purpose**: Calculate vision model token costs for images

```typescript
// system/rag/server/RAGMediaCalculator.ts

export class RAGMediaCalculator {
  /**
   * Calculate token cost for images based on vision model
   */
  static calculateImageTokens(
    images: RAGArtifact[],
    modelId: string
  ): { total: number; breakdown: string[] } {
    let total = 0;
    const breakdown: string[] = [];

    for (const image of images) {
      const tokens = this.calculateSingleImage(image, modelId);
      total += tokens;
      breakdown.push(`${image.type}: ${image.url} (${tokens} tokens)`);
    }

    return { total, breakdown };
  }

  private static calculateSingleImage(image: RAGArtifact, modelId: string): number {
    // Image dimensions (from metadata or default)
    const width = image.metadata?.width || 1024;
    const height = image.metadata?.height || 1024;

    // Model-specific calculation
    if (modelId.startsWith('gpt-4')) {
      // GPT-4V: 85-170 tokens per 512px tile
      return this.calculateGPT4VTokens(width, height);
    }

    if (modelId.startsWith('claude-3')) {
      // Claude 3: ~1600 tokens per image (approximate)
      return 1600;
    }

    if (modelId.startsWith('qwen')) {
      // Qwen2-VL: Variable based on resolution
      return this.calculateQwenVLTokens(width, height);
    }

    if (modelId.includes('llava')) {
      // LLaVA: ~256 tokens per image (lightweight)
      return 256;
    }

    // Default estimate
    return Math.floor((width * height) / 750); // ~1.3 tokens per 1000 pixels
  }

  private static calculateGPT4VTokens(width: number, height: number): number {
    // GPT-4V tiles images into 512px squares
    const tilesX = Math.ceil(width / 512);
    const tilesY = Math.ceil(height / 512);
    const tileCount = tilesX * tilesY;

    // 85-170 tokens per tile (use 127 average)
    const tokensPerTile = 127;
    return tileCount * tokensPerTile + 85; // +85 for base image token
  }

  private static calculateQwenVLTokens(width: number, height: number): number {
    // Qwen2-VL: ~0.002 tokens per pixel
    const pixels = width * height;
    return Math.floor(pixels * 0.002);
  }
}
```

---

## Enhanced ChatRAGBuilder

Putting it all together:

```typescript
async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions
): Promise<RAGContext> {
  const startTime = Date.now();

  // 1. Initialize budget tracker
  const budgetTracker = new RAGBudgetTracker(
    options?.modelId || 'unknown',
    options?.maxTokens || 3000
  );

  // 2. Build system prompt
  const identity = await this.loadPersonaIdentity(personaId, contextId);
  const systemPromptTokens = this.estimateTokens(identity.systemPrompt);
  budgetTracker.addComponent('systemPrompt', systemPromptTokens, 1);

  // 3. Retrieve vector memories (with circuit breaker)
  const memoryIntegrator = new RAGMemoryIntegrator(personaId);
  const memories = await memoryIntegrator.retrieveMemories(
    options?.currentMessage?.content || '',
    options?.maxMemories || 5,
    {
      minRelevance: 0.3,
      tokenBudget: budgetTracker.getBudget().remaining // Dynamic k
    }
  );
  const memoryTokens = memories.reduce((sum, m) =>
    sum + this.estimateTokens(m.memory.content), 0
  );
  budgetTracker.addComponent('memories', memoryTokens, memories.length,
    memories.map(m =>
      `${m.memory.memoryType}: ${m.memory.content.substring(0, 50)}... (score: ${m.relevanceScore.toFixed(2)})`
    )
  );

  // 4. Load recent messages (adaptive count based on remaining budget)
  const remainingBudget = budgetTracker.getBudget().remaining;
  const maxMessages = Math.floor(remainingBudget / 400); // ~400 tokens per message
  const conversationHistory = await this.loadConversationHistory(
    contextId,
    personaId,
    Math.min(maxMessages, options?.maxMessages || 20)
  );
  const messageTokens = conversationHistory.reduce((sum, m) =>
    sum + this.estimateTokens(m.content), 0
  );
  budgetTracker.addComponent('recentMessages', messageTokens, conversationHistory.length);

  // 5. Extract and calculate media tokens
  const artifacts = await this.extractArtifacts(contextId, conversationHistory.length);
  const { total: mediaTokens, breakdown: mediaBreakdown } =
    RAGMediaCalculator.calculateImageTokens(artifacts, options?.modelId || 'unknown');
  budgetTracker.addComponent('media', mediaTokens, artifacts.length, mediaBreakdown);

  // 6. Current message
  const currentMessageTokens = options?.currentMessage
    ? this.estimateTokens(options.currentMessage.content)
    : 0;
  budgetTracker.addComponent('currentMessage', currentMessageTokens, 1);

  // 7. Get final budget and log report
  const budget = budgetTracker.getBudget();
  console.log(budgetTracker.getReport());

  // 8. Handle over-budget scenario
  if (budget.isOverBudget) {
    console.error('❌ RAG context exceeds capacity! Trimming...');
    // Trim messages first, then memories if needed
    // Re-calculate budget after trimming
  }

  // 9. Build final context
  const ragContext: RAGContext = {
    domain: 'chat',
    contextId,
    personaId,
    identity,
    conversationHistory,
    artifacts,
    privateMemories: memories.map(m => ({
      content: m.memory.content,
      relevanceScore: m.relevanceScore,
      memoryType: m.memory.memoryType,
      timestamp: m.memory.timestamp
    })),
    metadata: {
      messageCount: conversationHistory.length,
      artifactCount: artifacts.length,
      memoryCount: memories.length,
      builtAt: new Date()
    },

    // NEW: Include budget for interrogation
    budget,

    // NEW: Include snapshot for historical analysis
    snapshot: {
      personaId,
      contextId,
      timestamp: Date.now(),
      components: budget.components,
      warnings: budget.warnings
    }
  };

  // 10. Emit cognition event
  const durationMs = Date.now() - startTime;
  await Events.emit<StageCompleteEvent>(...);

  return ragContext;
}
```

---

## Implementation Phases

### Phase 2B.1: Budget Tracking (Week 1)
- ✅ RAGBudgetTracker implementation
- ✅ Per-component token tracking
- ✅ Media token calculation
- ✅ Budget reports in logs
- ✅ Over-budget detection & trimming

### Phase 2B.2: Memory Integration (Week 2)
- ✅ RAGMemoryIntegrator with circuit breaker
- ✅ Vector search integration
- ✅ LRU caching
- ✅ Fallback to recent memories
- ✅ Batch embedding optimizations

### Phase 2B.3: Interrogation (Week 3)
- ✅ RAGInspector implementation
- ✅ CLI commands: inspect, budget, explain, monitor, stats
- ✅ Historical snapshot storage
- ✅ Real-time monitoring
- ✅ Budget analytics

### Phase 2B.4: Integration & Testing (Week 4)
- ✅ Enhanced ChatRAGBuilder with all components
- ✅ End-to-end tests
- ✅ Performance benchmarks
- ✅ Production hardening
- ✅ Documentation

---

## Success Metrics

### Budget Accuracy
- **Target**: <5% token estimation error
- **Measure**: Compare estimates to actual API token counts
- **Baseline**: Current ~25% error (250 token average)

### Memory Retrieval
- **Target**: >90% circuit breaker uptime
- **Measure**: Successful retrieval rate over 24h
- **Fallback**: <10% fallback mode activation

### Interrogation Usability
- **Target**: <1 second for inspect command
- **Measure**: CLI response time
- **Coverage**: 100% of RAG components visible

### Production Readiness
- **Target**: Zero over-budget incidents
- **Measure**: Budget warnings/errors in logs
- **Safety**: 10-20% margin maintained

---

## Testing Strategy

### Unit Tests
```bash
npx vitest tests/unit/RAGBudgetTracker.test.ts
npx vitest tests/unit/RAGMediaCalculator.test.ts
npx vitest tests/unit/RAGMemoryIntegrator.test.ts
```

### Integration Tests
```bash
npx vitest tests/integration/rag-budget-integration.test.ts
npx vitest tests/integration/rag-memory-integration.test.ts
npx vitest tests/integration/rag-inspector.test.ts
```

### E2E Tests
```bash
# Build context and verify budget
npm start && sleep 120
./jtag rag/inspect --personaId="helper-ai" --roomId="general"

# Verify memory retrieval
./jtag memory/create --personaId="helper-ai" --content="Test memory"
./jtag rag/inspect --personaId="helper-ai" --roomId="general"
# Should show memory in budget breakdown

# Verify media calculation
# Send message with image
./jtag rag/inspect --personaId="helper-ai" --roomId="general"
# Should show media tokens

# Test circuit breaker
# Kill embedding service
./jtag rag/inspect --personaId="helper-ai" --roomId="general"
# Should show fallback mode after 3 failures
```

---

## Conclusion

**Phase 2B creates the RAG Hippocampus** - a sophisticated, transparent, memory-integrated context builder that:

1. ✅ **Tracks every token** - Per-component breakdown with precision
2. ✅ **Integrates vector memory** - Semantic retrieval with circuit breakers
3. ✅ **Calculates media costs** - Vision model token estimation
4. ✅ **Enables interrogation** - Full transparency via logs and CLI
5. ✅ **Prevents meltdowns** - Budget enforcement with safety margins
6. ✅ **Degrades gracefully** - Fallbacks at every layer

This becomes the **universal context builder** that makes the AI team better than me - because they'll have:
- Sophisticated memory retrieval (semantic, not just recent)
- Token-aware context management (no hallucinations from overflow)
- Full transparency (interrogate why they responded that way)
- Resilient architecture (circuit breakers, caching, fallbacks)

**Next Step**: Implement Phase 2B.1 (Budget Tracking) as separate PR from Progressive Scoring (PR #192).
