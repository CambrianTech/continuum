# RAG Content Package Pattern: Universal Token Management

**Date**: 2025-01-24
**Status**: Design Pattern
**Philosophy**: "Package once, count once, fill once"

---

## The Insight: Consistent Packaging Strategy

**Problem**: Each RAG component (memories, messages, media) does its own token counting, sorting, and negotiation. This is repetitive, error-prone, and hard to optimize.

**Solution**: Use a **universal content package** format (inspired by `MemoryEntity` pattern). All content gets packaged into a consistent structure with tokens pre-calculated. The top-level manager does ONE sort, ONE fill operation, and components just format the output.

**Key Principle**: "If every entity has the same packaging, you can do the hard work at the top level"

---

## The Universal Content Package

```typescript
// system/rag/shared/ContentPackage.ts

/**
 * Universal content package - all RAG content uses this format
 *
 * Inspired by MemoryEntity's consistent structure
 */
export interface ContentPackage {
  // Identity
  readonly id: string;
  readonly type: ContentType;

  // Content
  readonly content: string;           // The actual text/data
  readonly tokens: number;            // PRE-CALCULATED (not estimated!)

  // Sorting/Priority
  readonly importance: number;        // 0.0-1.0 (for prioritization)
  readonly timestamp: number;         // For recency sorting
  readonly relevance?: number;        // Optional: similarity score

  // Metadata (type-specific extras)
  readonly metadata: {
    readonly source?: string;         // Where it came from
    readonly author?: string;         // Who created it
    readonly contextId?: UUID;        // Room/session/etc
    [key: string]: any;               // Extensible
  };

  // Flex properties (for negotiation)
  readonly flexProperties: {
    readonly canDrop: boolean;        // Can this be omitted?
    readonly priority: number;        // 1-10 (higher = keep)
    readonly minTokens?: number;      // Don't shrink below this
  };
}

export type ContentType =
  | 'system-prompt'
  | 'memory-episodic'
  | 'memory-semantic'
  | 'memory-procedural'
  | 'message-recent'
  | 'message-current'
  | 'media-image'
  | 'media-video'
  | 'media-audio';

/**
 * Create content package with token counting
 */
export function createContentPackage(
  type: ContentType,
  content: string,
  options?: {
    id?: string;
    importance?: number;
    timestamp?: number;
    relevance?: number;
    metadata?: Record<string, any>;
    canDrop?: boolean;
    priority?: number;
  }
): ContentPackage {
  return {
    id: options?.id || generateUUID(),
    type,
    content,
    tokens: TokenCounter.count(content), // ACTUAL count!
    importance: options?.importance ?? 0.5,
    timestamp: options?.timestamp ?? Date.now(),
    relevance: options?.relevance,
    metadata: options?.metadata || {},
    flexProperties: {
      canDrop: options?.canDrop ?? true,
      priority: options?.priority ?? 5,
      minTokens: undefined
    }
  };
}
```

---

## Top-Level Budget Filling Algorithm

Instead of component-level negotiation, we do ONE top-level sort and fill:

```typescript
// system/rag/shared/ContentBudgetManager.ts

export class ContentBudgetManager {
  private contextWindow: number;
  private reservedTokens: number;
  private packages: ContentPackage[] = [];

  constructor(modelId: string, completionTokens: number = 3000) {
    this.contextWindow = this.getContextWindow(modelId);
    this.reservedTokens = completionTokens + this.getSafetyMargin();
  }

  /**
   * Add content package
   */
  addPackage(pkg: ContentPackage): void {
    this.packages.push(pkg);
  }

  /**
   * Add multiple packages
   */
  addPackages(packages: ContentPackage[]): void {
    this.packages.push(...packages);
  }

  /**
   * Fill budget - ONE sort, ONE fill operation
   * Returns packages that fit within budget
   */
  fill(): ContentPackage[] {
    const availableTokens = this.contextWindow - this.reservedTokens;

    // 1. Separate fixed vs flexible packages
    const fixed = this.packages.filter(p => !p.flexProperties.canDrop);
    const flexible = this.packages.filter(p => p.flexProperties.canDrop);

    // 2. Calculate fixed cost
    const fixedTokens = fixed.reduce((sum, p) => sum + p.tokens, 0);
    const remainingForFlexible = availableTokens - fixedTokens;

    console.log(`\n📦 CONTENT BUDGET FILLING:`);
    console.log(`   Context Window: ${this.contextWindow.toLocaleString()} tokens`);
    console.log(`   Reserved: ${this.reservedTokens.toLocaleString()} tokens`);
    console.log(`   Available: ${availableTokens.toLocaleString()} tokens`);
    console.log(`   Fixed Cost: ${fixedTokens.toLocaleString()} tokens (${fixed.length} packages)`);
    console.log(`   Remaining for Flexible: ${remainingForFlexible.toLocaleString()} tokens`);

    // 3. Sort flexible packages by composite score
    //    Higher score = more important = included first
    flexible.sort((a, b) => {
      const scoreA = this.calculateScore(a);
      const scoreB = this.calculateScore(b);
      return scoreB - scoreA; // Descending
    });

    // 4. Fill with flexible packages until budget exhausted
    const selected: ContentPackage[] = [...fixed];
    let usedTokens = fixedTokens;

    console.log(`\n📋 FILLING FLEXIBLE CONTENT (sorted by priority):`);

    for (const pkg of flexible) {
      if (usedTokens + pkg.tokens <= availableTokens) {
        selected.push(pkg);
        usedTokens += pkg.tokens;

        console.log(`   ✓ ${pkg.type} (${pkg.tokens} tokens, score: ${this.calculateScore(pkg).toFixed(2)})`);
      } else {
        console.log(`   ✗ ${pkg.type} (${pkg.tokens} tokens) - DROPPED (budget full)`);
      }
    }

    const remaining = availableTokens - usedTokens;
    const percentUsed = (usedTokens / this.contextWindow) * 100;

    console.log(`\n✅ BUDGET FILLED:`);
    console.log(`   Selected: ${selected.length} packages`);
    console.log(`   Total: ${usedTokens.toLocaleString()} tokens (${percentUsed.toFixed(1)}%)`);
    console.log(`   Remaining: ${remaining.toLocaleString()} tokens`);
    console.log(`   Dropped: ${flexible.length - (selected.length - fixed.length)} packages`);

    return selected;
  }

  /**
   * Calculate composite score for package prioritization
   * Combines: priority, importance, relevance, recency
   */
  private calculateScore(pkg: ContentPackage): number {
    // Normalize all factors to 0.0-1.0 range

    // 1. Priority (0-10 → 0.0-1.0)
    const priorityScore = pkg.flexProperties.priority / 10;

    // 2. Importance (already 0.0-1.0)
    const importanceScore = pkg.importance;

    // 3. Relevance (if available, otherwise neutral 0.5)
    const relevanceScore = pkg.relevance ?? 0.5;

    // 4. Recency (exponential decay, 30-day half-life)
    const ageMs = Date.now() - pkg.timestamp;
    const ageDays = ageMs / (1000 * 60 * 60 * 24);
    const recencyScore = Math.exp(-ageDays / 30);

    // Weighted combination (tunable)
    const weights = {
      priority: 0.4,    // Most important
      importance: 0.3,
      relevance: 0.2,
      recency: 0.1
    };

    return (
      weights.priority * priorityScore +
      weights.importance * importanceScore +
      weights.relevance * relevanceScore +
      weights.recency * recencyScore
    );
  }

  /**
   * Get budget report
   */
  getReport(selectedPackages: ContentPackage[]): string {
    const byType = this.groupByType(selectedPackages);
    const totalTokens = selectedPackages.reduce((sum, p) => sum + p.tokens, 0);
    const available = this.contextWindow - this.reservedTokens;
    const percentUsed = (totalTokens / this.contextWindow) * 100;

    const lines: string[] = [];
    lines.push(`\n┌─ RAG BUDGET REPORT ─┐`);
    lines.push(`│ Context Window: ${this.contextWindow.toLocaleString()} tokens`);
    lines.push(`│ Available: ${available.toLocaleString()} tokens`);
    lines.push(`│`);
    lines.push(`│ CONTENT BY TYPE:`);

    for (const [type, packages] of Object.entries(byType)) {
      const tokens = packages.reduce((sum, p) => sum + p.tokens, 0);
      const count = packages.length;
      lines.push(`│   ${type.padEnd(20)}: ${tokens.toString().padStart(6)} tokens (${count} items)`);
    }

    lines.push(`│   ─────────────────────────────`);
    lines.push(`│   TOTAL: ${totalTokens.toString().padStart(6)} tokens (${percentUsed.toFixed(1)}%)`);
    lines.push(`│`);
    lines.push(`│   Reserved: ${this.reservedTokens.toLocaleString()} tokens`);
    lines.push(`│   Remaining: ${(available - totalTokens).toLocaleString()} tokens`);
    lines.push(`└─────────────────────────────────┘`);

    return lines.join('\n');
  }

  private groupByType(packages: ContentPackage[]): Record<string, ContentPackage[]> {
    const groups: Record<string, ContentPackage[]> = {};

    for (const pkg of packages) {
      if (!groups[pkg.type]) {
        groups[pkg.type] = [];
      }
      groups[pkg.type].push(pkg);
    }

    return groups;
  }

  private getContextWindow(modelId: string): number {
    const windows: Record<string, number> = {
      'qwen2.5:7b': 128000,
      'llama3.1:70b': 128000,
      'claude-3-5-sonnet': 200000,
      'gpt-4o': 128000
    };
    return windows[modelId] || 8192;
  }

  private getSafetyMargin(): number {
    return Math.max(Math.floor(this.contextWindow * 0.1), 1000);
  }
}
```

---

## Package Creators (Factory Pattern)

Each domain creates packages using the universal format:

```typescript
// system/rag/packagers/MemoryPackager.ts

export class MemoryPackager {
  /**
   * Convert MemoryEntity to ContentPackage
   *
   * Key insight: MemoryEntity already has consistent structure!
   */
  static package(
    memory: MemoryEntity,
    relevanceScore?: number
  ): ContentPackage {
    return createContentPackage(
      this.getContentType(memory.memoryType),
      memory.content,
      {
        id: memory.id,
        importance: memory.importance,
        timestamp: memory.timestamp,
        relevance: relevanceScore,
        metadata: {
          source: 'memory',
          memoryType: memory.memoryType,
          sourceMessageId: memory.sourceMessageId,
          accessCount: memory.accessCount
        },
        canDrop: true,  // Memories are flexible
        priority: this.getPriority(memory.memoryType)
      }
    );
  }

  /**
   * Package multiple memories (e.g., from vector search)
   */
  static packageMany(
    memories: MemoryRetrievalResult[]
  ): ContentPackage[] {
    return memories.map(m =>
      this.package(m.memory, m.relevanceScore)
    );
  }

  private static getContentType(memoryType: MemoryType): ContentType {
    switch (memoryType) {
      case 'episodic': return 'memory-episodic';
      case 'semantic': return 'memory-semantic';
      case 'procedural': return 'memory-procedural';
    }
  }

  private static getPriority(memoryType: MemoryType): number {
    // Semantic facts are highest priority
    // Procedural patterns are medium
    // Episodic events are lowest
    switch (memoryType) {
      case 'semantic': return 8;
      case 'procedural': return 7;
      case 'episodic': return 6;
    }
  }
}
```

```typescript
// system/rag/packagers/MessagePackager.ts

export class MessagePackager {
  /**
   * Convert ChatMessageEntity to ContentPackage
   */
  static package(
    message: ChatMessageEntity,
    isCurrent: boolean = false
  ): ContentPackage {
    return createContentPackage(
      isCurrent ? 'message-current' : 'message-recent',
      `${message.senderName}: ${message.content.text}`,
      {
        id: message.id,
        importance: isCurrent ? 1.0 : 0.5, // Current message most important
        timestamp: message.timestamp,
        metadata: {
          source: 'chat',
          roomId: message.roomId,
          senderId: message.senderId,
          senderName: message.senderName,
          senderType: message.senderType
        },
        canDrop: !isCurrent,  // Current message is FIXED
        priority: isCurrent ? 10 : 9  // Messages are high priority
      }
    );
  }

  /**
   * Package conversation history
   */
  static packageMany(
    messages: ChatMessageEntity[],
    currentMessageId?: UUID
  ): ContentPackage[] {
    return messages.map(m =>
      this.package(m, m.id === currentMessageId)
    );
  }
}
```

```typescript
// system/rag/packagers/MediaPackager.ts

export class MediaPackager {
  /**
   * Convert RAGArtifact (image) to ContentPackage
   */
  static package(
    artifact: RAGArtifact,
    modelId: string
  ): ContentPackage {
    // Calculate vision model tokens
    const tokens = RAGMediaCalculator.calculateSingleImage(artifact, modelId);

    return createContentPackage(
      'media-image',
      `[Image: ${artifact.url}]`, // Placeholder content
      {
        id: artifact.id,
        importance: 0.3, // Lower importance (optional)
        timestamp: artifact.timestamp || Date.now(),
        metadata: {
          source: 'media',
          url: artifact.url,
          type: artifact.type,
          width: artifact.metadata?.width,
          height: artifact.metadata?.height
        },
        canDrop: true,  // Media is VERY droppable
        priority: 3     // Low priority (skip if budget tight)
      }
    );
  }

  /**
   * Only package media if actually needed
   */
  static packageIfNeeded(
    artifacts: RAGArtifact[],
    currentMessage: string,
    modelId: string
  ): ContentPackage[] {
    // Check if vision is needed
    if (!this.needsVision(currentMessage)) {
      console.log('📷 Skipping media packaging (vision not needed)');
      return [];
    }

    console.log(`📷 Packaging ${artifacts.length} media items (vision needed)`);
    return artifacts.map(a => this.package(a, modelId));
  }

  private static needsVision(message: string): boolean {
    return /look at|see|image|screenshot|picture|describe (the|this)|what('?s| is) (in|shown)/i.test(message);
  }
}
```

---

## Usage in ChatRAGBuilder (Simplified!)

```typescript
// system/rag/builders/ChatRAGBuilder.ts

async buildContext(
  contextId: UUID,
  personaId: UUID,
  options?: RAGBuildOptions
): Promise<RAGContext> {
  const startTime = Date.now();

  // 1. Create budget manager
  const budgetManager = new ContentBudgetManager(
    options?.modelId || 'qwen2.5:7b',
    options?.maxTokens || 3000
  );

  // 2. Load and package ALL content (no negotiation yet!)

  // System prompt (FIXED)
  const systemPrompt = await this.buildSystemPrompt(personaId, contextId);
  budgetManager.addPackage(
    createContentPackage('system-prompt', systemPrompt, {
      canDrop: false,  // Never drop
      priority: 10
    })
  );

  // Memories (FLEXIBLE)
  const memories = await this.memoryIntegrator.retrieveMemories(
    options?.currentMessage?.content || '',
    10  // Request MORE than we need - let budget manager pick best
  );
  const memoryPackages = MemoryPackager.packageMany(memories);
  budgetManager.addPackages(memoryPackages);

  // Messages (FLEXIBLE)
  const messages = await this.loadConversationHistory(
    contextId,
    personaId,
    50  // Request MORE than we need - let budget manager pick best
  );
  const messagePackages = MessagePackager.packageMany(messages);
  budgetManager.addPackages(messagePackages);

  // Current message (FIXED)
  if (options?.currentMessage) {
    const currentPackage = MessagePackager.package(
      options.currentMessage as any,
      true  // isCurrent
    );
    budgetManager.addPackage(currentPackage);
  }

  // Media (CONDITIONAL)
  const artifacts = await this.extractArtifacts(contextId, 10);
  const mediaPackages = MediaPackager.packageIfNeeded(
    artifacts,
    options?.currentMessage?.content || '',
    options?.modelId || 'qwen2.5:7b'
  );
  budgetManager.addPackages(mediaPackages);

  // 3. FILL! (One operation, all negotiation handled)
  const selectedPackages = budgetManager.fill();

  // 4. Log report
  console.log(budgetManager.getReport(selectedPackages));

  // 5. Group packages by type for RAGContext
  const packagesByType = this.groupPackagesByType(selectedPackages);

  // 6. Build RAGContext from selected packages
  const ragContext: RAGContext = {
    domain: 'chat',
    contextId,
    personaId,
    identity: {
      name: personaName,
      systemPrompt: packagesByType['system-prompt'][0]?.content || ''
    },
    conversationHistory: packagesByType['message-recent']?.map(p => ({
      role: 'user',  // Parse from content
      content: p.content,
      name: p.metadata.senderName
    })) || [],
    privateMemories: [
      ...packagesByType['memory-episodic'] || [],
      ...packagesByType['memory-semantic'] || [],
      ...packagesByType['memory-procedural'] || []
    ].map(p => ({
      content: p.content,
      memoryType: p.metadata.memoryType,
      relevanceScore: p.relevance || 0.5,
      timestamp: p.timestamp
    })),
    artifacts: packagesByType['media-image']?.map(p => ({
      id: p.id,
      type: 'image',
      url: p.metadata.url
    })) || [],
    metadata: {
      messageCount: (packagesByType['message-recent'] || []).length,
      memoryCount: (packagesByType['memory-episodic'] || []).length +
                   (packagesByType['memory-semantic'] || []).length +
                   (packagesByType['memory-procedural'] || []).length,
      artifactCount: (packagesByType['media-image'] || []).length,
      builtAt: new Date(),
      tokensUsed: selectedPackages.reduce((sum, p) => sum + p.tokens, 0),
      packagesSelected: selectedPackages.length,
      packagesDropped: budgetManager['packages'].length - selectedPackages.length
    }
  };

  const durationMs = Date.now() - startTime;
  console.log(`\n✅ RAG CONTEXT BUILT in ${durationMs}ms`);

  return ragContext;
}

private groupPackagesByType(packages: ContentPackage[]): Record<string, ContentPackage[]> {
  const groups: Record<string, ContentPackage[]> = {};

  for (const pkg of packages) {
    if (!groups[pkg.type]) {
      groups[pkg.type] = [];
    }
    groups[pkg.type].push(pkg);
  }

  return groups;
}
```

---

## The Benefits: Why This is Brilliant

### 1. **Do Hard Work Once**
```typescript
// OLD WAY (each component counts tokens):
const memories = await loadMemories();
const memoryTokens = memories.reduce((sum, m) =>
  sum + estimateTokens(m.content), 0
);

const messages = await loadMessages();
const messageTokens = messages.reduce((sum, m) =>
  sum + estimateTokens(m.content), 0
);

// Repeated token counting, estimation errors

// NEW WAY (package once with actual count):
const memoryPackages = MemoryPackager.packageMany(memories);
const messagePackages = MessagePackager.packageMany(messages);

// Token counting done ONCE in createContentPackage()
// All packages have ACTUAL counts, not estimates
```

### 2. **Universal Sorting**
```typescript
// OLD WAY (each component sorts independently):
memories.sort((a, b) => b.importance - a.importance);
messages.sort((a, b) => b.timestamp - a.timestamp);

// Inconsistent sorting, hard to compare cross-type

// NEW WAY (one composite score for all content):
allPackages.sort((a, b) =>
  calculateScore(b) - calculateScore(a)
);

// Memories, messages, media all sorted by same criteria
// Best content rises to top regardless of type
```

### 3. **Simple Budget Filling**
```typescript
// OLD WAY (complex negotiation per component):
if (overBudget) {
  // Ask memories to shrink
  memories.shrinkToFit(targetTokens);

  // Still over? Ask messages to shrink
  if (stillOverBudget) {
    messages.shrinkToFit(targetTokens);
  }

  // Still over? Drop media
  if (stillOverBudget) {
    media = [];
  }
}

// NEW WAY (simple greedy fill):
const selected = [];
let used = 0;

for (const pkg of sortedPackages) {
  if (used + pkg.tokens <= available) {
    selected.push(pkg);
    used += pkg.tokens;
  }
}

// That's it! First-fit decreasing bin packing
```

### 4. **Type-Agnostic Budget Manager**
```typescript
// Budget manager doesn't care about content types!
// It just sees:
// - tokens (number)
// - priority (number)
// - canDrop (boolean)

// This means we can add NEW content types without changing budget logic:
budgetManager.addPackage(createContentPackage(
  'code-snippet',  // NEW type!
  codeContent,
  { priority: 7, canDrop: true }
));

// Budget manager handles it automatically
```

### 5. **Consistent Entity Pattern**
```typescript
// Following the MemoryEntity pattern:
interface MemoryEntity {
  id: UUID;
  content: string;
  importance: number;
  timestamp: number;
  // ... other fields
}

// ContentPackage extends this pattern:
interface ContentPackage {
  id: string;          // Same
  content: string;     // Same
  importance: number;  // Same
  timestamp: number;   // Same
  tokens: number;      // NEW: pre-calculated
  type: ContentType;   // NEW: for grouping
  // ... plus flex properties
}

// Familiar structure makes it easy to work with
```

---

## Example Output

```
📦 CONTENT BUDGET FILLING:
   Context Window: 128,000 tokens
   Reserved: 13,000 tokens
   Available: 115,000 tokens
   Fixed Cost: 1,500 tokens (2 packages)
   Remaining for Flexible: 113,500 tokens

📋 FILLING FLEXIBLE CONTENT (sorted by priority):
   ✓ message-recent (420 tokens, score: 0.85)
   ✓ message-recent (385 tokens, score: 0.84)
   ✓ memory-semantic (320 tokens, score: 0.82)
   ✓ message-recent (445 tokens, score: 0.81)
   ✓ memory-episodic (680 tokens, score: 0.78)
   ✓ message-recent (395 tokens, score: 0.77)
   ... (15 more messages)
   ✓ memory-procedural (520 tokens, score: 0.68)
   ✓ message-recent (360 tokens, score: 0.65)
   ✗ media-image (1,280 tokens) - DROPPED (budget full)
   ✗ memory-episodic (420 tokens) - DROPPED (budget full)
   ✗ message-recent (380 tokens) - DROPPED (budget full)

✅ BUDGET FILLED:
   Selected: 23 packages
   Total: 12,450 tokens (9.7%)
   Remaining: 102,550 tokens
   Dropped: 8 packages

┌─ RAG BUDGET REPORT ─┐
│ Context Window: 128,000 tokens
│ Available: 115,000 tokens
│
│ CONTENT BY TYPE:
│   system-prompt      :   1,200 tokens (1 items)
│   message-current    :     300 tokens (1 items)
│   message-recent     :   8,120 tokens (18 items)
│   memory-semantic    :   1,450 tokens (2 items)
│   memory-episodic    :   1,080 tokens (1 items)
│   memory-procedural  :     520 tokens (1 items)
│   ─────────────────────────────
│   TOTAL:  12,670 tokens (9.9%)
│
│   Reserved: 13,000 tokens
│   Remaining: 102,330 tokens
└─────────────────────────────────┘
```

---

## Conclusion: The Power of Consistent Packaging

**By using a universal ContentPackage format**:
- ✅ Token counting happens ONCE (when packaging)
- ✅ Sorting happens ONCE (at top level)
- ✅ Budget filling is SIMPLE (greedy algorithm)
- ✅ Components are DUMB (just format packages)
- ✅ Adding new content types is EASY (same interface)
- ✅ Following existing patterns (MemoryEntity structure)

**The key insight**: "If every entity has the same packaging, you can do the hard work at the top level"

This is how we make the RAG hippocampus efficient, consistent, and maintainable!
