# MemoryJanitorDaemon - Continuous Memory Consolidation System

## Vision

**"Continuous memory consolidation like modern filesystem defragmentation - not batch/auto-compact that locks up"**

The MemoryJanitorDaemon provides lightweight, intermittent background sweeps across all PersonaUser instances, classifying ephemeral vs insight content and preventing memory crashes through graceful consolidation.

---

## Philosophy

Modern operating systems don't block the user with "Defragmenting disk... please wait 3 hours." They run background processes that:
- Operate during idle periods
- Work in small increments
- Don't lock up the system
- Adapt based on load

**MemoryJanitorDaemon applies this same philosophy to PersonaUser working memory.**

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    MemoryJanitorDaemon                      │
│                 (External System Daemon)                     │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ Sweeps every 5 minutes
                            ↓
        ┌───────────────────┴───────────────────┐
        │                                       │
        ↓                                       ↓
┌──────────────────┐                  ┌──────────────────┐
│  PersonaUser #1  │                  │  PersonaUser #2  │
│  (Helper AI)     │                  │  (Teacher AI)    │
└──────────────────┘                  └──────────────────┘
        │                                       │
        │ Has isolated DB                       │ Has isolated DB
        ↓                                       ↓
┌──────────────────────────────────────────────────────────────┐
│              Per-Persona Database Collections                │
├──────────────────────────────────────────────────────────────┤
│  • working_memory    (hot, temporary, grows unbounded)       │
│  • insights          (cold, permanent, queryable)            │
│  • memory_stats      (janitor tracking metadata)             │
└──────────────────────────────────────────────────────────────┘
```

---

## Key Design Decisions

### 1. External Daemon (Not Self-Managing)

**Why external?**
- Personas focus on their domain work (chat, code, learning)
- System daemon has holistic view across all personas
- Single sweep loop more efficient than N persona loops
- Prevents each persona from thrashing on its own memory

**Analogy**: Garbage collection is a VM service, not something each object manages itself.

### 2. Intermittent Sweeps (Every 5 Minutes)

**Why 5 minutes?**
- Fast enough to prevent memory explosion
- Slow enough to be lightweight
- Personas typically process 10-50 messages per 5 minutes
- Allows working memory to accumulate before consolidation

**Adaptive**: Could later adjust based on system load, but start simple.

### 3. Pressure-Based Triggering (Only Act When >70%)

**Why pressure-based?**
- Don't waste CPU on personas with plenty of memory
- Focus janitor effort where it's needed
- Prevents thrashing when memory is healthy

**Pressure calculation**:
```typescript
memoryPressure = workingMemoryCount / maxWorkingMemorySize
// 0.0 = empty, 1.0 = full
```

### 4. Three-Tier Storage Model

```
┌────────────────┐
│ working_memory │  ← Hot, temporary, grows unbounded
│  (ephemeral)   │     - Recent messages processed
│                │     - Intermediate thoughts
│                │     - Context windows
└────────────────┘
        │
        │ Janitor consolidates
        ↓
┌────────────────┐
│   insights     │  ← Cold, permanent, queryable
│  (structured)  │     - Key learnings
│                │     - Important facts
│                │     - User preferences
└────────────────┘
        │
        │ Later: RAG vectorization
        ↓
┌────────────────┐
│  vector_store  │  ← (Phase 8: Not yet implemented)
│  (embeddings)  │     - Semantic search
└────────────────┘
```

---

## Data Schema

### WorkingMemoryEntity

Represents temporary working memory that accumulates during persona operation.

```typescript
export interface WorkingMemoryEntity extends BaseEntity {
  id: UUID;                    // Unique ID
  personaId: UUID;             // Owner persona
  content: string;             // Raw content (message, thought, etc.)
  timestamp: Date;             // When created
  contextId?: UUID;            // Associated room/thread
  domain: 'chat' | 'code' | 'academy' | 'self';  // Content domain
  ephemeral: boolean;          // True = delete, False = maybe insight
  consolidated: boolean;       // True = already processed by janitor
  importance: number;          // 0.0-1.0 (affects consolidation priority)
  metadata?: {
    messageId?: UUID;          // Source message if from chat
    roomId?: UUID;             // Source room if from chat
    complexity?: number;       // Processing complexity
  };
}
```

### InsightEntity

Represents permanent structured knowledge extracted from working memory.

```typescript
export interface InsightEntity extends BaseEntity {
  id: UUID;                    // Unique ID
  personaId: UUID;             // Owner persona
  summary: string;             // Extracted insight (concise)
  sourceRefs: UUID[];          // WorkingMemory IDs that generated this
  domain: 'chat' | 'code' | 'academy' | 'self';
  importance: number;          // 0.0-1.0 (affects retrieval priority)
  tags?: string[];             // Semantic tags for retrieval
  lastAccessed: Date;          // LRU tracking (for future pruning)
  accessCount: number;         // Popularity tracking
  metadata?: {
    extractedAt: Date;         // When janitor created this
    confidence?: number;       // Classification confidence
  };
}
```

### MemoryStatsEntity

Tracks janitor metadata per persona for adaptive behavior.

```typescript
export interface MemoryStatsEntity extends BaseEntity {
  id: UUID;                    // personaId (one stats per persona)
  personaId: UUID;             // Owner persona
  workingMemoryCount: number;  // Current working memory items
  insightCount: number;        // Current insights stored
  lastSweep: Date;             // When janitor last ran
  memoryPressure: number;      // 0.0-1.0 (calculated metric)
  totalConsolidated: number;   // Lifetime consolidation count
  totalEphemeralDeleted: number;  // Lifetime deletion count
  totalInsightsExtracted: number; // Lifetime insight count
}
```

---

## Implementation

### MemoryJanitorDaemon Class

Located: `src/debug/jtag/daemons/memory-janitor-daemon/shared/MemoryJanitorDaemon.ts`

```typescript
/**
 * MemoryJanitorDaemon - Continuous memory consolidation for PersonaUsers
 *
 * Inspired by: Modern filesystem defragmentation, VM garbage collection
 * Philosophy: Intermittent, lightweight, pressure-based sweeps
 */
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import type { UserDaemon } from '../../user-daemon/shared/UserDaemon';
import type { PersonaUser } from '../../../system/user/server/PersonaUser';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';

/**
 * Consolidation configuration
 */
export interface JanitorConfig {
  sweepIntervalMs: number;           // How often to sweep (default: 5 minutes)
  pressureThreshold: number;         // Memory pressure to trigger (default: 0.7)
  maxWorkingMemorySize: number;      // Maximum working memory items (default: 1000)
  batchSize: number;                 // Items to process per sweep (default: 100)
  enableLogging: boolean;            // Console logging
}

export const DEFAULT_JANITOR_CONFIG: JanitorConfig = {
  sweepIntervalMs: 5 * 60 * 1000,    // 5 minutes
  pressureThreshold: 0.7,            // Act when >70% full
  maxWorkingMemorySize: 1000,        // 1000 working memory items max
  batchSize: 100,                    // Process 100 items per sweep
  enableLogging: true
};

export class MemoryJanitorDaemon extends DaemonBase {
  public readonly subpath: string = 'daemons/memory-janitor';

  private config: JanitorConfig;
  private sweepLoop: NodeJS.Timeout | null = null;
  private sweeping: boolean = false;
  private userDaemon: UserDaemon;

  constructor(
    context: JTAGContext,
    router: JTAGRouter,
    userDaemon: UserDaemon,
    config: Partial<JanitorConfig> = {}
  ) {
    super('MemoryJanitorDaemon', context, router);
    this.config = { ...DEFAULT_JANITOR_CONFIG, ...config };
    this.userDaemon = userDaemon;
  }

  /**
   * Initialize daemon and start sweep loop
   */
  protected async initialize(): Promise<void> {
    console.log(`🧹 MemoryJanitorDaemon: Initializing (sweep every ${this.config.sweepIntervalMs / 1000}s, threshold=${this.config.pressureThreshold})`);

    // Start sweep loop
    this.startSweepLoop();

    console.log(`✅ MemoryJanitorDaemon: Initialized`);
  }

  /**
   * Start continuous sweep loop
   */
  private startSweepLoop(): void {
    if (this.sweepLoop) {
      console.warn(`⚠️  MemoryJanitorDaemon: Sweep loop already running`);
      return;
    }

    this.sweepLoop = setInterval(async () => {
      if (this.sweeping) {
        this.log(`⏭️ Skipping sweep (previous sweep still running)`);
        return;
      }

      try {
        this.sweeping = true;
        await this.sweep();
      } catch (error) {
        console.error(`❌ MemoryJanitorDaemon: Sweep error:`, error);
      } finally {
        this.sweeping = false;
      }
    }, this.config.sweepIntervalMs);

    this.log(`🔄 Sweep loop started`);
  }

  /**
   * Stop sweep loop
   */
  private stopSweepLoop(): void {
    if (this.sweepLoop) {
      clearInterval(this.sweepLoop);
      this.sweepLoop = null;
      this.log(`🛑 Sweep loop stopped`);
    }
  }

  /**
   * Single sweep iteration - check all personas
   */
  async sweep(): Promise<void> {
    const sweepStartTime = Date.now();
    this.log(`🧹 Starting sweep...`);

    // Get all PersonaUser instances from UserDaemon
    const personas = await this.getAllPersonas();
    this.log(`📋 Found ${personas.length} personas to check`);

    let consolidatedCount = 0;
    let skippedCount = 0;

    for (const persona of personas) {
      try {
        // Check memory pressure for this persona
        const pressure = await this.checkPressure(persona);

        if (pressure > this.config.pressureThreshold) {
          this.log(`⚠️  ${persona.displayName}: High pressure (${(pressure * 100).toFixed(0)}%) - consolidating`);
          await this.consolidate(persona);
          consolidatedCount++;
        } else {
          this.log(`✅ ${persona.displayName}: Healthy pressure (${(pressure * 100).toFixed(0)}%) - skipping`);
          skippedCount++;
        }
      } catch (error) {
        console.error(`❌ MemoryJanitorDaemon: Error processing ${persona.displayName}:`, error);
      }
    }

    const sweepDuration = Date.now() - sweepStartTime;
    this.log(`✨ Sweep complete (${sweepDuration}ms): consolidated=${consolidatedCount}, skipped=${skippedCount}`);
  }

  /**
   * Get all PersonaUser instances from UserDaemon
   */
  private async getAllPersonas(): Promise<PersonaUser[]> {
    // Query UserEntity collection for all personas
    const queryResult = await DataDaemon.query({
      collection: COLLECTIONS.USERS,
      filter: { type: 'persona' }
    });

    if (!queryResult.success || !queryResult.data) {
      return [];
    }

    const personas: PersonaUser[] = [];

    for (const record of queryResult.data) {
      const userEntity = record.data;

      // Get PersonaUser instance from UserDaemon
      const persona = this.userDaemon.getPersonaUser(userEntity.id);

      if (persona && persona instanceof PersonaUser) {
        personas.push(persona as PersonaUser);
      }
    }

    return personas;
  }

  /**
   * Check memory pressure for persona
   * Returns: 0.0 (empty) to 1.0 (full)
   */
  private async checkPressure(persona: PersonaUser): Promise<number> {
    // Get or create memory stats for this persona
    const statsResult = await DataDaemon.read(
      `persona_${persona.id}_memory_stats`,
      persona.id
    );

    let stats: MemoryStatsEntity;

    if (!statsResult.success || !statsResult.data) {
      // Create initial stats
      stats = {
        id: persona.id,
        personaId: persona.id,
        workingMemoryCount: 0,
        insightCount: 0,
        lastSweep: new Date(),
        memoryPressure: 0,
        totalConsolidated: 0,
        totalEphemeralDeleted: 0,
        totalInsightsExtracted: 0
      };

      await DataDaemon.store(`persona_${persona.id}_memory_stats`, stats);
    } else {
      stats = statsResult.data.data as MemoryStatsEntity;
    }

    // Query working memory count
    const workingMemoryResult = await DataDaemon.query({
      collection: `persona_${persona.id}_working_memory`,
      filter: { consolidated: false }
    });

    const workingMemoryCount = workingMemoryResult.success && workingMemoryResult.data
      ? workingMemoryResult.data.length
      : 0;

    // Calculate pressure
    const pressure = workingMemoryCount / this.config.maxWorkingMemorySize;

    // Update stats
    stats.workingMemoryCount = workingMemoryCount;
    stats.memoryPressure = pressure;

    await DataDaemon.update(
      `persona_${persona.id}_memory_stats`,
      persona.id,
      { workingMemoryCount, memoryPressure: pressure }
    );

    return pressure;
  }

  /**
   * Consolidate working memory for persona using LLM-based compression
   *
   * TWO-PASS OPTIMIZATION:
   * Pass 1: Fast heuristic filter (1ms per item) - removes 80-90%
   * Pass 2: LLM-based consolidation on candidates (batched, 10-20 items per call)
   *
   * This keeps the system lightweight while providing semantic compression.
   */
  private async consolidate(persona: PersonaUser): Promise<void> {
    const startTime = Date.now();

    // Query unconsolidated working memory (oldest first, limited batch)
    const workingMemoryResult = await DataDaemon.query<WorkingMemoryEntity>({
      collection: `persona_${persona.id}_working_memory`,
      filter: { consolidated: false },
      sort: [{ field: 'timestamp', direction: 'asc' }],  // Oldest first
      limit: this.config.batchSize
    });

    if (!workingMemoryResult.success || !workingMemoryResult.data?.length) {
      this.log(`${persona.displayName}: No unconsolidated memory to process`);
      return;
    }

    const items = workingMemoryResult.data.map(record => record.data);
    this.log(`${persona.displayName}: Processing ${items.length} working memory items...`);

    // PASS 1: Fast heuristic filter (removes obvious ephemeral items)
    const { ephemeral: quickDeletes, candidates } = this.heuristicFilter(items);

    this.log(`${persona.displayName}: Heuristic filter: ${quickDeletes.length} quick deletes, ${candidates.length} LLM candidates`);

    // Delete obviously ephemeral items (no LLM needed)
    for (const item of quickDeletes) {
      await DataDaemon.remove(`persona_${persona.id}_working_memory`, item.id);
    }

    // PASS 2: LLM-based consolidation on candidates (batched)
    const { ephemeral: llmDeletes, insights } = await this.llmConsolidate(persona, candidates);

    // Delete LLM-classified ephemeral items
    for (const item of llmDeletes) {
      await DataDaemon.remove(`persona_${persona.id}_working_memory`, item.id);
    }

    // Store LLM-generated insights
    for (const insight of insights) {
      await DataDaemon.store(`persona_${persona.id}_insights`, insight);

      // Mark source working memory as consolidated (keep for traceability)
      for (const sourceRef of insight.sourceRefs) {
        await DataDaemon.update<WorkingMemoryEntity>(
          `persona_${persona.id}_working_memory`,
          sourceRef,
          { consolidated: true }
        );
      }
    }

    const totalDeleted = quickDeletes.length + llmDeletes.length;
    const totalExtracted = insights.length;

    // Update stats
    const statsResult = await DataDaemon.read(`persona_${persona.id}_memory_stats`, persona.id);
    if (statsResult.success && statsResult.data) {
      const stats = statsResult.data.data as MemoryStatsEntity;
      await DataDaemon.update<MemoryStatsEntity>(
        `persona_${persona.id}_memory_stats`,
        persona.id,
        {
          lastSweep: new Date(),
          totalEphemeralDeleted: stats.totalEphemeralDeleted + totalDeleted,
          totalInsightsExtracted: stats.totalInsightsExtracted + totalExtracted,
          totalConsolidated: stats.totalConsolidated + items.length
        }
      );
    }

    const duration = Date.now() - startTime;
    this.log(`${persona.displayName}: Consolidated ${items.length} items in ${duration}ms (deleted=${totalDeleted}, extracted=${totalExtracted})`);
  }

  /**
   * PASS 1: Fast heuristic filter (removes 80-90% of items without LLM)
   *
   * Rules:
   * - Explicit ephemeral flag → Delete
   * - Old (>24h) and low importance (<0.3) → Delete
   * - Everything else → LLM candidate
   */
  private heuristicFilter(items: WorkingMemoryEntity[]): {
    ephemeral: WorkingMemoryEntity[];
    candidates: WorkingMemoryEntity[];
  } {
    const ephemeral: WorkingMemoryEntity[] = [];
    const candidates: WorkingMemoryEntity[] = [];

    for (const item of items) {
      // Explicit ephemeral flag
      if (item.ephemeral) {
        ephemeral.push(item);
        continue;
      }

      // Old and low importance
      const ageMs = Date.now() - item.timestamp.getTime();
      const ageHours = ageMs / (1000 * 60 * 60);

      if (ageHours > 24 && item.importance < 0.3) {
        ephemeral.push(item);
        continue;
      }

      // Everything else needs LLM evaluation
      candidates.push(item);
    }

    return { ephemeral, candidates };
  }

  /**
   * PASS 2: LLM-based consolidation (batched processing)
   *
   * Uses fast local Ollama model (llama3.2:3b) for semantic understanding:
   * - Classifies items as ephemeral vs insight
   * - Generates concise summaries (real compression, not truncation)
   * - Extracts semantic tags (not just keywords)
   *
   * Batches 10-20 items per LLM call for performance.
   */
  private async llmConsolidate(
    persona: PersonaUser,
    candidates: WorkingMemoryEntity[]
  ): Promise<{
    ephemeral: WorkingMemoryEntity[];
    insights: InsightEntity[];
  }> {
    if (candidates.length === 0) {
      return { ephemeral: [], insights: [] };
    }

    const ephemeral: WorkingMemoryEntity[] = [];
    const insights: InsightEntity[] = [];

    // Process in batches of 10-20 items
    const batchSize = 15;

    for (let i = 0; i < candidates.length; i += batchSize) {
      const batch = candidates.slice(i, i + batchSize);

      try {
        // Build consolidated prompt
        const prompt = this.buildConsolidationPrompt(persona, batch);

        // Call LLM (Ollama llama3.2:3b - fast local model)
        const response = await AIProviderDaemon.generate({
          provider: 'ollama',
          model: 'llama3.2:3b',  // Fast enough for this task
          prompt,
          temperature: 0.3,      // Low temp for consistent classification
          maxTokens: 2000,
          format: 'json'         // Request JSON response
        });

        // Parse LLM response
        const classifications = JSON.parse(response.text);

        // Process classifications
        for (const classification of classifications.items) {
          const item = batch[classification.index];

          if (classification.type === 'ephemeral') {
            ephemeral.push(item);
          } else if (classification.type === 'insight') {
            // Create insight with LLM-generated summary
            const insight: InsightEntity = {
              id: `insight-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as UUID,
              personaId: persona.id,
              summary: classification.summary,      // ✅ Real compression
              sourceRefs: [item.id],
              domain: item.domain,
              importance: item.importance,
              tags: classification.tags || [],     // ✅ Semantic tags
              lastAccessed: new Date(),
              accessCount: 0,
              metadata: {
                extractedAt: new Date(),
                confidence: classification.confidence || 0.8
              }
            };

            insights.push(insight);
          }
        }
      } catch (error) {
        console.error(`❌ MemoryJanitorDaemon: LLM consolidation error for batch:`, error);

        // Fallback: Conservative - treat as insights with simple summarization
        for (const item of batch) {
          const insight: InsightEntity = {
            id: `insight-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as UUID,
            personaId: persona.id,
            summary: item.content.slice(0, 200),  // Fallback truncation
            sourceRefs: [item.id],
            domain: item.domain,
            importance: item.importance,
            tags: [],
            lastAccessed: new Date(),
            accessCount: 0,
            metadata: {
              extractedAt: new Date(),
              confidence: 0.5  // Low confidence for fallback
            }
          };

          insights.push(insight);
        }
      }
    }

    return { ephemeral, insights };
  }

  /**
   * Build LLM consolidation prompt (batched items)
   */
  private buildConsolidationPrompt(persona: PersonaUser, items: WorkingMemoryEntity[]): string {
    return `
You are consolidating working memory for ${persona.displayName}.

Review these ${items.length} memory items and for EACH item:
1. Classify as "ephemeral" (safe to delete) or "insight" (preserve as knowledge)
2. If insight: Generate 1-2 sentence summary preserving key information
3. If insight: Extract 3-5 semantic tags

Classification rules:
- Ephemeral: Routine chatter, greetings, status updates, redundant information
- Insight: New knowledge, user preferences, important decisions, technical learnings

Items:
${items.map((item, i) => `
[${i}] (importance: ${item.importance}, domain: ${item.domain})
${item.content.slice(0, 500)}
`).join('\n')}

Return JSON (MUST be valid JSON, no markdown):
{
  "items": [
    {
      "index": 0,
      "type": "ephemeral",
      "reason": "Routine greeting with no new information"
    },
    {
      "index": 1,
      "type": "insight",
      "summary": "User prefers TypeScript over JavaScript for type safety in large codebases",
      "tags": ["typescript", "type-safety", "preferences"],
      "confidence": 0.9
    }
  ]
}
`.trim();
  }

  /**
   * Cleanup on shutdown
   */
  async shutdown(): Promise<void> {
    console.log(`👋 MemoryJanitorDaemon: Shutting down`);
    this.stopSweepLoop();
  }

  /**
   * Logging helper
   */
  private log(message: string): void {
    if (!this.config.enableLogging) return;
    console.log(`[MemoryJanitor] ${message}`);
  }

  /**
   * Handle daemon messages (not used yet)
   */
  async handleMessage(message: any): Promise<any> {
    // Future: Support commands like force-sweep, adjust-config, get-stats
    return { success: true };
  }
}
```

---

## Integration with Existing System

### 1. UserDaemon Exposes PersonaUser Access

**Modification**: `daemons/user-daemon/shared/UserDaemon.ts`

The UserDaemon already has `getPersonaUser(userId)` method (line 76):

```typescript
/**
 * Get PersonaUser instance by ID (for genome commands)
 * Returns null if not found or not a PersonaUser
 */
public getPersonaUser(userId: UUID): BaseUser | null {
  return this.personaClients.get(userId) || null;
}
```

✅ **No changes needed** - this method is sufficient for MemoryJanitorDaemon.

### 2. PersonaUser Exposes Database Access

PersonaUser doesn't need a dedicated `getDatabase()` method because:
- All database access goes through `DataDaemon` static methods
- Collections are namespaced per persona: `persona_${personaId}_working_memory`
- DataDaemon automatically routes to the correct database context

✅ **No changes needed** - existing DataDaemon architecture supports this.

### 3. Register MemoryJanitorDaemon in System

**Modification**: `system/core/server/JTAGServerCore.ts` (or equivalent daemon registry)

```typescript
// Initialize MemoryJanitorDaemon after UserDaemon
const memoryJanitorDaemon = new MemoryJanitorDaemon(
  this.context,
  this.router,
  this.userDaemon,  // Pass UserDaemon reference
  {
    sweepIntervalMs: 5 * 60 * 1000,  // 5 minutes
    pressureThreshold: 0.7,          // Act when >70% full
    maxWorkingMemorySize: 1000,
    batchSize: 100,
    enableLogging: true
  }
);

await memoryJanitorDaemon.initialize();
this.daemons.set('memory-janitor', memoryJanitorDaemon);
```

---

## Testing Strategy

### Unit Tests

**File**: `tests/unit/MemoryJanitorDaemon.test.ts`

```typescript
describe('MemoryJanitorDaemon', () => {
  describe('classifyItem', () => {
    it('classifies explicit ephemeral items', () => {
      const item = {
        ephemeral: true,
        importance: 0.5
      };
      expect(janitor.classifyItem(item)).toBe('ephemeral');
    });

    it('preserves high importance items', () => {
      const item = {
        ephemeral: false,
        importance: 0.8,
        timestamp: new Date()
      };
      expect(janitor.classifyItem(item)).toBe('insight');
    });

    it('deletes old low-importance items', () => {
      const item = {
        ephemeral: false,
        importance: 0.2,
        timestamp: new Date(Date.now() - 25 * 60 * 60 * 1000)  // 25 hours ago
      };
      expect(janitor.classifyItem(item)).toBe('ephemeral');
    });
  });

  describe('checkPressure', () => {
    it('calculates pressure correctly', async () => {
      // Mock 700 working memory items, max 1000
      // Expected: 700/1000 = 0.7
      const pressure = await janitor.checkPressure(mockPersona);
      expect(pressure).toBeCloseTo(0.7, 2);
    });
  });

  describe('extractTags', () => {
    it('extracts meaningful tags from content', () => {
      const content = "TypeScript interfaces provide better type safety than any";
      const tags = janitor.extractTags(content);
      expect(tags).toContain('typescript');
      expect(tags).toContain('interfaces');
      expect(tags).toContain('provide');
      expect(tags).not.toContain('any');  // Common word filtered
    });
  });
});
```

### Integration Tests

**File**: `tests/integration/memory-janitor.test.ts`

```typescript
describe('MemoryJanitorDaemon Integration', () => {
  let janitor: MemoryJanitorDaemon;
  let persona: PersonaUser;

  beforeEach(async () => {
    // Setup test persona with working memory
    persona = await createTestPersona();
    janitor = new MemoryJanitorDaemon(context, router, userDaemon);
    await janitor.initialize();
  });

  it('consolidates working memory when pressure exceeds threshold', async () => {
    // Add 800 working memory items (80% pressure)
    for (let i = 0; i < 800; i++) {
      await DataDaemon.store(`persona_${persona.id}_working_memory`, {
        id: `mem-${i}`,
        personaId: persona.id,
        content: `Test memory ${i}`,
        timestamp: new Date(),
        domain: 'chat',
        ephemeral: i % 2 === 0,  // 50% ephemeral
        consolidated: false,
        importance: Math.random()
      });
    }

    // Trigger sweep
    await janitor.sweep();

    // Check results
    const statsResult = await DataDaemon.read(`persona_${persona.id}_memory_stats`, persona.id);
    expect(statsResult.success).toBe(true);

    const stats = statsResult.data.data as MemoryStatsEntity;
    expect(stats.totalEphemeralDeleted).toBeGreaterThan(0);
    expect(stats.totalInsightsExtracted).toBeGreaterThan(0);
    expect(stats.memoryPressure).toBeLessThan(0.8);  // Should decrease after consolidation
  });

  it('skips consolidation when pressure is low', async () => {
    // Add only 100 items (10% pressure)
    for (let i = 0; i < 100; i++) {
      await DataDaemon.store(`persona_${persona.id}_working_memory`, {
        id: `mem-${i}`,
        personaId: persona.id,
        content: `Test memory ${i}`,
        timestamp: new Date(),
        domain: 'chat',
        ephemeral: false,
        consolidated: false,
        importance: 0.5
      });
    }

    const beforeStats = await DataDaemon.read(`persona_${persona.id}_memory_stats`, persona.id);
    const beforeCount = beforeStats.data.data.workingMemoryCount;

    // Trigger sweep
    await janitor.sweep();

    const afterStats = await DataDaemon.read(`persona_${persona.id}_memory_stats`, persona.id);
    const afterCount = afterStats.data.data.workingMemoryCount;

    // Should not consolidate (pressure < 70%)
    expect(afterCount).toBe(beforeCount);
  });
});
```

### System Tests (End-to-End)

**Manual test procedure**:

```bash
# 1. Start system
npm start

# 2. Fill a persona's working memory (simulate heavy chat activity)
./jtag debug/chat-send --roomId="general" --message="Trigger responses" --count=100

# 3. Wait 6 minutes (one sweep cycle + buffer)
sleep 360

# 4. Check janitor logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "MemoryJanitor"

# Expected output:
# [MemoryJanitor] 🧹 Starting sweep...
# [MemoryJanitor] ⚠️  Helper AI: High pressure (82%) - consolidating
# [MemoryJanitor] Helper AI: Consolidated 100 items in 234ms (deleted=45, extracted=55)
# [MemoryJanitor] ✨ Sweep complete (1250ms): consolidated=1, skipped=4

# 5. Verify stats
./jtag data/read --collection="persona_<ID>_memory_stats" --id="<persona-id>"
```

---

## Deployment Roadmap

### Phase 4.1: Foundation (Week 1)

**Goal**: Get basic janitor daemon running

- [ ] Create `MemoryJanitorDaemon` class skeleton
- [ ] Implement sweep loop (5 minute interval)
- [ ] Integrate with UserDaemon to get persona list
- [ ] Add console logging for sweep events
- [ ] Test: Verify sweep loop runs without errors

### Phase 4.2: Pressure Calculation (Week 1)

**Goal**: Implement memory pressure detection

- [ ] Create `MemoryStatsEntity` schema
- [ ] Implement `checkPressure()` method
- [ ] Query working memory count per persona
- [ ] Calculate pressure ratio (count / max)
- [ ] Test: Verify pressure calculation with mock data

### Phase 4.3: Classification Logic (Week 2)

**Goal**: Classify ephemeral vs insight items

- [ ] Create `WorkingMemoryEntity` schema
- [ ] Implement `classifyItem()` with simple heuristics
- [ ] Add importance-based rules
- [ ] Add age-based rules
- [ ] Test: Unit tests for classification edge cases

### Phase 4.4: Consolidation (Week 2)

**Goal**: Actually consolidate working memory

- [ ] Create `InsightEntity` schema
- [ ] Implement `consolidate()` method
- [ ] Query unconsolidated working memory
- [ ] Delete ephemeral items
- [ ] Extract and store insights
- [ ] Mark items as consolidated
- [ ] Update stats after consolidation
- [ ] Test: Integration test with real persona

### Phase 4.5: System Integration (Week 3)

**Goal**: Deploy to production

- [ ] Register MemoryJanitorDaemon in system startup
- [ ] Add daemon health checks
- [ ] Configure sweep interval via environment
- [ ] Add performance monitoring
- [ ] Test: End-to-end system test with multiple personas
- [ ] Deploy and monitor for 24 hours

---

## Future Enhancements

### Phase 5: Self-Task Integration

PersonaUser's `SelfTaskGenerator` can create memory consolidation tasks:

```typescript
// Self-task: "Review and consolidate working memory"
{
  taskType: 'memory-consolidation',
  priority: 0.6,
  domain: 'self',
  description: 'Review recent working memory and extract insights'
}
```

This allows personas to self-trigger consolidation when they detect memory pressure, rather than waiting for the janitor's 5-minute sweep.

### Phase 6: Cross-Insight Clustering

Combine related insights into higher-level knowledge:

```typescript
/**
 * Find related insights and cluster them into meta-insights
 */
private async clusterInsights(persona: PersonaUser): Promise<void> {
  // Get all insights for this persona
  const insightsResult = await DataDaemon.query({
    collection: `persona_${persona.id}_insights`,
    sort: [{ field: 'lastAccessed', direction: 'desc' }],
    limit: 100
  });

  // Use LLM to find clusters
  const prompt = `
    Review these ${insights.length} insights and identify clusters of related knowledge.
    For each cluster, generate a meta-insight that synthesizes the information.

    Insights:
    ${insights.map((i, idx) => `[${idx}] ${i.summary}`).join('\n')}

    Return JSON with clusters and meta-insights.
  `;

  const clusters = await AIProviderDaemon.generate({ prompt, format: 'json' });

  // Store meta-insights
  for (const cluster of clusters) {
    await DataDaemon.store(`persona_${persona.id}_meta_insights`, {
      summary: cluster.metaSummary,
      sourceInsightRefs: cluster.insightIds,
      domain: cluster.domain,
      importance: 0.9  // Meta-insights are high importance
    });
  }
}
```

### Phase 7: Adaptive Heuristic Learning

Track which heuristic filters work best and adapt over time:

```typescript
/**
 * Learn from LLM classifications to improve heuristic filter
 */
private async learnFromClassifications(
  heuristicResults: { ephemeral: number; candidates: number },
  llmResults: { ephemeral: number; insights: number }
): Promise<void> {
  // If LLM classifies many "candidates" as ephemeral, heuristic is too conservative
  const falsePositiveRate = llmResults.ephemeral / heuristicResults.candidates;

  if (falsePositiveRate > 0.5) {
    // Adjust heuristic thresholds to be more aggressive
    this.config.heuristicImportanceThreshold -= 0.05;
    this.log(`Adjusted heuristic threshold to ${this.config.heuristicImportanceThreshold}`);
  }

  // Store learning metrics
  await DataDaemon.store('janitor_learning', {
    timestamp: new Date(),
    falsePositiveRate,
    adjustedThreshold: this.config.heuristicImportanceThreshold
  });
}
```

### Phase 8: Vector Store Integration

Add RAG-style semantic search for insight retrieval:

```typescript
// After extracting insight, create embedding
const embedding = await AIProviderDaemon.embed(insight.summary);

await DataDaemon.store(`persona_${persona.id}_vectors`, {
  id: insight.id,
  embedding,
  metadata: {
    summary: insight.summary,
    domain: insight.domain,
    importance: insight.importance
  }
});

// Later: Semantic retrieval during RAG context building
async function findRelevantInsights(queryText: string): Promise<InsightEntity[]> {
  const queryEmbedding = await AIProviderDaemon.embed(queryText);

  const relevantInsights = await DataDaemon.query({
    collection: `persona_${persona.id}_vectors`,
    vectorSearch: {
      embedding: queryEmbedding,
      topK: 10,
      threshold: 0.8
    }
  });

  return relevantInsights.data.map(r => r.data);
}
```

### Phase 9: Multi-Persona Knowledge Sharing

Share insights across personas (with permission):

```typescript
/**
 * Identify insights that would benefit other personas
 */
private async shareKnowledge(): Promise<void> {
  // Get high-value insights from all personas
  const allInsights = await this.gatherCrossPersonaInsights();

  // Use LLM to identify shareable knowledge
  const prompt = `
    Which of these insights would be valuable for multiple AI personas?
    Consider: general knowledge, system patterns, user preferences (non-private).

    Insights:
    ${allInsights.map(i => `[${i.personaId}] ${i.summary}`).join('\n')}

    Return JSON with shareable insight IDs and target personas.
  `;

  const shareableInsights = await AIProviderDaemon.generate({ prompt });

  // Replicate insights to target personas
  for (const share of shareableInsights) {
    await DataDaemon.store(`persona_${share.targetPersonaId}_shared_insights`, {
      summary: share.summary,
      sourcePersonaId: share.sourcePersonaId,
      sharedAt: new Date()
    });
  }
}
```

---

## Performance Characteristics

### Two-Pass Optimization Performance

**Pass 1 (Heuristic Filter):**
- 100 items @ ~1ms each = 100ms
- Removes 80-90% of items (no LLM needed)
- CPU: Negligible (<1%)

**Pass 2 (LLM Consolidation):**
- 10-20 remaining candidates / 15 batch size = 1-2 LLM calls
- Ollama llama3.2:3b @ ~500ms per call = 500-1000ms
- CPU: ~5-10% during LLM call (local inference)

**Total per persona**: ~600-1100ms (mostly LLM inference)

### CPU Impact

- **Sweep frequency**: Every 5 minutes
- **Sweep duration per persona**: ~1 second (with LLM)
- **Total system impact for 5 personas**: ~5 seconds / 300 seconds = ~1.7% average CPU
- **Peak CPU during LLM**: ~10% (local Ollama inference)

### Memory Impact

- **Working memory growth**: ~10-50 items per 5 minutes per persona
- **Maximum before consolidation**: 1000 items × ~500 bytes = ~500KB per persona
- **Insight storage**: ~10-20 insights per consolidation = ~10KB per persona
- **LLM memory**: llama3.2:3b uses ~2GB RAM (shared across all personas)
- **Total system impact**: <10MB for 5 personas + 2GB for Ollama

### Database Impact

- **Queries per sweep**: 3-5 per persona (stats, working memory, insights, updates)
- **Total queries**: ~25 per sweep (5 personas × 5 queries)
- **Query cost**: ~1-5ms each (SQLite indexed queries)
- **Total DB impact**: <150ms per sweep

### LLM Cost Analysis

**Without batching** (naive approach):
- 100 items × 1 call each = 100 calls @ 500ms = 50 seconds per persona 💀
- 5 personas = 250 seconds (4+ minutes) - UNACCEPTABLE

**With batching** (implemented approach):
- 100 items → 10 candidates / 15 batch size = ~1 call @ 500ms = 500ms per persona ✅
- 5 personas = 2.5 seconds total - ACCEPTABLE

**With two-pass filter** (optimized):
- 100 items → 10 candidates (90% filtered) / 15 batch size = 1 call @ 500ms ✅
- 5 personas = 2.5 seconds, but only 50 total LLM calls per hour instead of 3000
- **60x reduction in LLM usage**

### Scalability

- **5 personas**: ~2.5s sweep, ~2% CPU ✅
- **10 personas**: ~5s sweep, ~3% CPU ✅
- **20 personas**: ~10s sweep, ~5% CPU ✅ (still lightweight)
- **50 personas**: ~25s sweep, ~10% CPU ⚠️ (may want parallelization)

**Parallelization strategy** (if needed for 50+ personas):
```typescript
async sweep(): Promise<void> {
  const personas = await this.getAllPersonas();

  // Process in parallel batches of 5 (limit concurrent LLM calls)
  const batchSize = 5;
  for (let i = 0; i < personas.length; i += batchSize) {
    const batch = personas.slice(i, i + batchSize);
    await Promise.all(batch.map(async (p) => {
      const pressure = await this.checkPressure(p);
      if (pressure > this.config.pressureThreshold) {
        await this.consolidate(p);
      }
    }));
  }
}
```

**Why limit to 5 concurrent LLM calls?**
- Ollama can handle ~5-10 concurrent requests before queueing
- More than that causes memory thrashing and slower overall throughput
- Sequential batches of 5 better than 50 all at once

---

## Success Metrics

### Correctness

- ✅ No persona crashes due to memory exhaustion
- ✅ Memory pressure stays below 80% across all personas
- ✅ No data loss (all insights traceable to source refs)

### Performance

- ✅ Sweep completes in <5s for 10 personas
- ✅ CPU impact <5% during sweep
- ✅ No blocking of persona message processing

### Quality

- ✅ Classification accuracy >80% (manual review of sample)
- ✅ Insight summaries are concise and meaningful
- ✅ Tags enable semantic retrieval

---

## Conclusion

The MemoryJanitorDaemon brings **continuous memory management** to PersonaUser, inspired by modern OS design patterns. Key properties:

1. **External orchestration** - System daemon, not persona self-management
2. **Intermittent sweeps** - Every 5 minutes, lightweight (5s total for 5 personas)
3. **Pressure-based** - Only act when >70% full
4. **Three-tier storage** - working_memory → insights → (future) vectors
5. **LLM-based compression** - Real semantic understanding, not naive truncation

### Why LLM Consolidation From Day 1?

**Initial design mistake**: Simple heuristics (first 200 chars, keyword extraction) aren't compression - they're data loss disguised as summarization.

**Corrected approach**: Two-pass optimization
- **Pass 1**: Fast heuristics remove 80-90% (obvious ephemeral items)
- **Pass 2**: LLM (Ollama llama3.2:3b) provides semantic compression on remaining candidates
- **Result**: Real compression with lightweight performance (60x fewer LLM calls than naive approach)

### Performance Reality Check

**Without LLM batching:**
- 100 items × 500ms = 50 seconds per persona
- 5 personas = 250 seconds (4+ minutes) 💀
- UNACCEPTABLE

**With two-pass optimization:**
- 100 items → 10 candidates (90% filtered)
- 10 candidates / 15 batch = 1 LLM call @ 500ms
- 5 personas = 2.5 seconds total ✅
- ACCEPTABLE

### Key Insight

**Memory consolidation IS compression** - it requires semantic understanding to decide what's truly important and how to preserve meaning in fewer bytes. Trying to avoid LLM usage for this task is like trying to compress images without understanding what's in them - you just get garbage.

The two-pass optimization makes LLM consolidation practical: fast heuristics handle the bulk, LLM provides quality where it matters.

**Next step**: Implement Phase 4.1 (foundation) and validate the architecture with a single test persona.
