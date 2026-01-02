/**
 * Hippocampus - Episodic Memory System with Two-Tier Architecture
 *
 * Implements biological memory consolidation pattern:
 * - STM (Short-Term Memory): In-memory ring buffer for recent experiences
 * - LTM (Long-Term Memory): Persistent SQLite database for important memories
 *
 * Key Features:
 * - Non-blocking memory operations (queued via PersonaLogger pattern)
 * - Importance-based consolidation (STM → LTM)
 * - Dedicated per-persona database (isolated storage)
 * - Fast retrieval with indexes
 *
 * See HIPPOCAMPUS-MEMORY-DESIGN.md for full architecture
 */

import { PersonaContinuousSubprocess } from '../../PersonaSubprocess';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { PersonaUser } from '../../../PersonaUser';
import { Commands } from '../../../../../core/shared/Commands';
import { SystemPaths } from '../../../../../core/config/SystemPaths';
import type { DataOpenParams, DataOpenResult } from '../../../../../../commands/data/open/shared/DataOpenTypes';
import type { DataListParams, DataListResult } from '../../../../../../commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '../../../../../../commands/data/create/shared/DataCreateTypes';
import type { DbHandle } from '../../../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { generateUUID } from '../../../../../core/types/CrossPlatformUUID';
import { ISOString } from '../../../../../data/domains/CoreTypes';
import type {
  MemoryEntity,
  RecallParams,
  MemoryStats,
  MemoryType
} from '../../MemoryTypes';
import { MemoryType as MemoryTypeEnum } from '../../MemoryTypes';
import { AdaptiveConsolidationThreshold } from './AdaptiveConsolidationThreshold';
import { MemoryConsolidationAdapter } from './adapters/MemoryConsolidationAdapter';
import { SemanticCompressionAdapter } from './adapters/SemanticCompressionAdapter';
import { RawMemoryAdapter } from './adapters/RawMemoryAdapter';
import type { WorkingMemoryEntry } from '../../cognition/memory/InMemoryCognitionStorage';
import { DataDaemon } from '../../../../../../daemons/data-daemon/shared/DataDaemon';
import type { VectorSearchOptions, VectorSearchResponse } from '../../../../../../daemons/data-daemon/shared/VectorSearchTypes';
import { BackpressureService } from '../../../../../core/services/BackpressureService';

/**
 * Snapshot of persona state at tick time
 * Used for logging and consolidation decisions
 */
interface PersonaStateSnapshot {
  readonly inboxSize: number;
  readonly energy: number;
  readonly attention: number;
}

/**
 * Memory consolidation metrics
 */
interface ConsolidationMetrics {
  readonly tickCount: number;
  readonly lastConsolidation: Date | null;
  readonly consolidationCount: number;
  readonly stmEvictions: number;
}

/**
 * Hippocampus - Two-tier episodic memory system
 *
 * Phase 1 (CURRENT): STM + LTM with basic consolidation
 * - Short-term memory (ring buffer, in-memory)
 * - Long-term memory (SQLite, persistent)
 * - Importance-based consolidation
 * - Simple recall API
 *
 * Phase 2 (FUTURE): Advanced features
 * - Semantic search with vector embeddings
 * - Memory clustering
 * - Forgetting curve
 */
export class Hippocampus extends PersonaContinuousSubprocess {
  // Long-Term Memory (LTM) - Database handle
  private memoryDbHandle: DbHandle | null = null;

  // Adaptive consolidation threshold (sigmoid-based, activity-responsive)
  private adaptiveThreshold: AdaptiveConsolidationThreshold;

  // Memory consolidation adapter (pluggable strategy for consolidation)
  private consolidationAdapter: MemoryConsolidationAdapter;

  // Metrics
  private metrics: ConsolidationMetrics;
  private initializePromise: Promise<void> | null = null;

  constructor(persona: PersonaUser, adapter?: MemoryConsolidationAdapter) {
    super(persona, {
      priority: 'low', // Low priority - don't interfere with response times
      name: 'Hippocampus'
    });

    this.metrics = {
      tickCount: 0,
      lastConsolidation: null,
      consolidationCount: 0,
      stmEvictions: 0
    };

    // Initialize adaptive threshold (sigmoid-based, activity-responsive)
    this.adaptiveThreshold = new AdaptiveConsolidationThreshold();

    // Initialize consolidation adapter (default: semantic compression)
    // Pass persona directly - adapter uses persona.generateText() for synthesis (same code path as chat)
    const hippocampusLogger = (message: string, ...args: any[]) => {
      this.persona.logger.enqueueLog('hippocampus.log', message);
    };
    this.consolidationAdapter = adapter || new SemanticCompressionAdapter(
      persona,
      { maxThoughtsPerGroup: 10, logger: hippocampusLogger }
    );

    this.log(`Initialized with ${this.consolidationAdapter.getName()} adapter`);

    // Initialize database asynchronously (non-blocking)
    this.initializePromise = this.initializeDatabase();
  }

  /**
   * Initialize long-term memory database
   * Opens dedicated SQLite database for this persona
   */
  private async initializeDatabase(): Promise<void> {
    // Use SystemPaths.personas.longterm() as SINGLE SOURCE OF TRUTH for path
    const dbPath = SystemPaths.personas.longterm(this.persona.entity.uniqueId);

    try {
      this.log(`Opening LTM database: ${dbPath}`);

      const result = await Commands.execute<DataOpenParams, DataOpenResult>(DATA_COMMANDS.OPEN, {
        adapter: 'sqlite',
        config: {
          path: dbPath,
          mode: 'readwrite',
          wal: true,           // Write-Ahead Logging (fast writes)
          foreignKeys: true    // Referential integrity
        }
      });

      if (!result.success || !result.dbHandle) {
        throw new Error(result.error || 'Failed to open memory database');
      }

      this.memoryDbHandle = result.dbHandle;
      this.log(`LTM database opened: ${this.memoryDbHandle}`);
      this.log('LTM database initialized successfully');
    } catch (error) {
      const errorMsg = String(error);

      // EXFAT FIX: Detect corrupted database and recover by deleting and retrying
      if (errorMsg.includes('SQLITE_CORRUPT')) {
        this.log(`⚠️ Detected corrupted database at ${dbPath}, attempting recovery...`);

        try {
          // Delete corrupted database
          const fs = await import('fs/promises');
          await fs.unlink(dbPath);
          this.log(`✅ Deleted corrupted database: ${dbPath}`);

          // Retry opening (will create fresh database)
          this.log(`🔄 Retrying database initialization with fresh file...`);
          const result = await Commands.execute<DataOpenParams, DataOpenResult>(DATA_COMMANDS.OPEN, {
            adapter: 'sqlite',
            config: {
              path: dbPath,
              mode: 'readwrite',
              wal: true,
              foreignKeys: true
            }
          });

          if (!result.success || !result.dbHandle) {
            throw new Error(result.error || 'Failed to open memory database on retry');
          }

          this.memoryDbHandle = result.dbHandle;
          this.log(`LTM database opened (after recovery): ${this.memoryDbHandle}`);
          this.log('✅ LTM database recovered and initialized successfully');
          return;
        } catch (recoveryError) {
          this.log(`❌ Recovery failed: ${recoveryError}`);
        }
      }

      // If not corruption or recovery failed, log and continue in STM-only mode
      this.log(`❌ Failed to initialize LTM database: ${error}`);
      // Continue without LTM (STM-only mode)
    }
  }

  /**
   * Recall memories from LTM
   * Query consolidated long-term memories
   */
  public async recall(params: RecallParams = {}): Promise<MemoryEntity[]> {
    // Wait for database initialization
    if (this.initializePromise) {
      await this.initializePromise;
    }

    // Search LTM (database)
    if (!this.memoryDbHandle) {
      return []; // No LTM available
    }

    const results = await this.searchLTM(params);

    // Update access stats (TODO: batch update in LTM)
    for (const memory of results) {
      memory.accessCount++;
      memory.lastAccessedAt = ISOString(new Date().toISOString());
    }

    return results;
  }

  /**
   * Semantic recall - query memories by meaning, not just filters
   *
   * Uses vector similarity search to find semantically relevant memories.
   * This is the key capability for "thinking about what you know" vs "filtering what you stored".
   *
   * @param queryText - Natural language query describing what you're looking for
   * @param params - Additional filter constraints (types, importance, etc.)
   * @returns Memories ranked by semantic relevance
   */
  public async semanticRecall(queryText: string, params: RecallParams = {}): Promise<MemoryEntity[]> {
    // Wait for database initialization
    if (this.initializePromise) {
      await this.initializePromise;
    }

    if (!this.memoryDbHandle) {
      this.log('WARN: semanticRecall called without LTM database - falling back to filter-based recall');
      return this.recall(params);
    }

    if (!queryText || queryText.trim().length === 0) {
      this.log('WARN: semanticRecall called with empty query - falling back to filter-based recall');
      return this.recall(params);
    }

    try {
      // Build metadata filter for pre-filtering (reduces search space)
      // Note: No personaId filter - each persona has their own database file
      // This fixes orphaned memories when personas get new UUIDs on reseed
      const filter: Record<string, any> = {};

      if (params.types) {
        filter.type = { $in: params.types };
      }

      if (params.minImportance !== undefined) {
        filter.importance = { $gte: params.minImportance };
      }

      if (params.since) {
        filter.timestamp = { $gte: params.since };
      }

      // Perform vector similarity search via DataDaemon
      // Map RecallParams hybridMode to VectorSearchOptions hybridMode
      // 'filter' in our API maps to 'keyword' in vector search (filter-based = keyword-based)
      const vectorHybridMode = params.hybridMode === 'filter' ? 'keyword' :
                               params.hybridMode === 'hybrid' ? 'hybrid' : 'semantic';

      const searchOptions: VectorSearchOptions = {
        collection: 'memories',
        dbHandle: this.memoryDbHandle || undefined,  // Use per-persona database
        queryText,
        k: params.limit || 10,
        similarityThreshold: params.semanticThreshold || 0.6,
        filter,
        hybridMode: vectorHybridMode
      };

      // Use Commands.execute to go through VectorSearchServerCommand which handles dbHandle
      const result = await Commands.execute<any, any>('data/vector-search', searchOptions);

      if (!result.success || !result.results) {
        this.log(`WARN: Vector search failed: ${result.error} - falling back to filter-based recall`);
        return this.recall(params);
      }

      // Cast results back to MemoryEntity (type-safe at runtime, worked around generic constraint)
      const memories = result.results.map((r: { data: unknown }) => r.data as unknown as MemoryEntity);

      // Update access stats for retrieved memories
      for (const memory of memories) {
        memory.accessCount++;
        memory.lastAccessedAt = ISOString(new Date().toISOString());
      }

      this.log(`🔍 Semantic recall: "${queryText.slice(0, 50)}..." → ${memories.length} results ` +
               `(threshold=${searchOptions.similarityThreshold}, mode=${searchOptions.hybridMode})`);

      return memories;
    } catch (error) {
      this.log(`ERROR: Semantic recall failed: ${error} - falling back to filter-based recall`);
      return this.recall(params);
    }
  }

  /**
   * Search long-term memory (database)
   */
  private async searchLTM(params: RecallParams): Promise<MemoryEntity[]> {
    if (!this.memoryDbHandle) {
      return [];
    }

    try {
      // Build filter
      // Note: No personaId filter - each persona has their own database file
      // This fixes orphaned memories when personas get new UUIDs on reseed
      const filter: Record<string, any> = {};

      if (params.types) {
        filter.type = { $in: params.types };
      }

      if (params.minImportance !== undefined) {
        filter.importance = { $gte: params.minImportance };
      }

      if (params.since) {
        filter.timestamp = { $gte: params.since };
      }

      // Query LTM
      const result = (await Commands.execute(DATA_COMMANDS.LIST, {
        dbHandle: this.memoryDbHandle,
        collection: 'memories',
        filter,
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: params.limit || 100
      } as any)) as any;

      if (!result.success || !result.items) {
        return [];
      }

      // Return items as mutable array - database stores ISO strings which match our interface
      return [...result.items] as MemoryEntity[];
    } catch (error) {
      this.log(`ERROR: LTM search failed: ${error}`);
      return [];
    }
  }


  /**
   * Continuous tick - snoop WorkingMemory and consolidate to LTM
   */
  protected async tick(): Promise<void> {
    this.metrics = {
      ...this.metrics,
      tickCount: this.metrics.tickCount + 1
    };

    // BACKPRESSURE: Skip consolidation entirely when system is under high load
    // Consolidation involves LLM calls (expensive) - wait until load drops
    if (BackpressureService.isHighLoad()) {
      // Log only occasionally to avoid spam (every 30 ticks = ~30 seconds at 1s tick)
      if (this.metrics.tickCount % 30 === 0) {
        this.log(`🚦 Backpressure: Deferring consolidation (load=${BackpressureService.getLoad().toFixed(2)})`);
      }
      return;
    }

    // Snoop on PersonaUser's working memory (thoughts during cognition)
    if (this.persona.prefrontal?.workingMemory) {
      await this.snoopAndConsolidate();
    }

    // Log status every 60 ticks (~60 seconds)
    if (this.metrics.tickCount % 60 === 0) {
      const stats = await this.getStats();
      this.log(`Memory stats: WorkingMemory=${stats.stmSize}/${stats.stmMaxSize}, ` +
               `LTM=${stats.ltmCount}, consolidated=${stats.consolidationCount}`);
    }
  }

  /**
   * Snoop on WorkingMemory and consolidate important thoughts to LTM
   * Uses adaptive threshold (sigmoid + exponential decay)
   */
  private async snoopAndConsolidate(): Promise<void> {
    if (!this.memoryDbHandle || !this.persona.sessionId) {
      return; // Can't consolidate without LTM or session
    }

    try {
      // 1. Calculate recent activity level (messages/minute)
      const messagesPerMinute = await this.calculateActivityLevel();

      // 2. Update adaptive threshold (sigmoid + time decay)
      this.adaptiveThreshold.updateThreshold(messagesPerMinute);
      const currentThreshold = this.adaptiveThreshold.getThreshold();

      // 3. Recall thoughts above adaptive threshold
      const thoughts = await this.persona.prefrontal?.workingMemory?.recall({
        minImportance: currentThreshold,  // ← ADAPTIVE!
        limit: 50,
        includePrivate: true
      });

      if (!thoughts || thoughts.length === 0) {
        return;
      }

      // Log consolidation attempt
      const stats = this.adaptiveThreshold.getStats();
      this.log(`🧠 Consolidating: threshold=${currentThreshold.toFixed(2)}, ` +
               `activity=${messagesPerMinute.toFixed(1)} msg/min, ` +
               `decay=${stats.decayMultiplier.toFixed(2)}, ` +
               `timeSince=${stats.minutesSinceConsolidation.toFixed(1)}min, ` +
               `candidates=${thoughts.length}`);

      // Use consolidation adapter to transform thoughts → memories
      const consolidationResult = await this.consolidationAdapter.consolidate(thoughts, {
        personaId: this.persona.id,
        personaName: this.persona.entity.displayName,
        sessionId: this.persona.sessionId,
        timestamp: new Date()
      });

      const memories = consolidationResult.memories;
      const fallbackCount = memories.filter(m => m.source === 'working-memory-fallback').length;
      const synthesisCount = consolidationResult.metadata?.synthesisCount ?? 0;

      this.log(`📝 Adapter produced ${memories.length} memories (synthesis=${this.consolidationAdapter.doesSynthesis()}, ` +
               `successful=${synthesisCount}, fallback=${fallbackCount})`);

      // Log synthesis failures explicitly for visibility
      if (fallbackCount > 0) {
        this.log(`⚠️ Synthesis fallback: ${fallbackCount}/${memories.length} memories used fallback (LLM synthesis failed)`);

        // Log specific error details if available
        if (consolidationResult.metadata?.errors) {
          const errors = consolidationResult.metadata.errors as Array<{ domain: string; error: string }>;
          errors.forEach(err => {
            this.log(`   ❌ Domain [${err.domain}]: ${err.error}`);
          });
        }
      }

      // Write memories to LTM database
      // Try each insert individually so one failure doesn't kill the whole batch
      const consolidatedIds: string[] = [];
      let failedCount = 0;

      for (const memory of memories) {
        try {
          const result = await Commands.execute<DataCreateParams, DataCreateResult<any>>(DATA_COMMANDS.CREATE, {
            dbHandle: this.memoryDbHandle,
            collection: 'memories',
            data: memory
          } as any);

          if (result.success) {
            // Track original thought IDs for removal from working memory
            // For raw adapter: 1 memory = 1 thought
            // For synthesis adapter: 1 memory = N thoughts (track via context)
            if (memory.context?.synthesizedFrom) {
              consolidatedIds.push(...(memory.context.synthesizedFrom as string[]));
            } else {
              // Raw adapter: find matching thought by timestamp/content
              const matchingThought = thoughts.find((t: WorkingMemoryEntry) =>
                t.thoughtContent === memory.content ||
                Math.abs(new Date(t.createdAt).getTime() - new Date(memory.timestamp).getTime()) < 1000
              );
              if (matchingThought) {
                consolidatedIds.push(matchingThought.id);
              }
            }
          } else {
            failedCount++;
            this.log(`ERROR: Failed to store memory ${memory.id}: ${result.error}`);
          }
        } catch (error) {
          failedCount++;
          this.log(`❌ Failed to store memory ${memory.id}: ${error}`);
        }
      }

      // Remove ONLY successfully consolidated thoughts from working memory
      if (consolidatedIds.length > 0) {
        await this.persona.prefrontal?.workingMemory.clearBatch(consolidatedIds as any);
        this.log(`✅ Consolidated ${consolidatedIds.length} thoughts to LTM${failedCount > 0 ? ` (${failedCount} failed)` : ''}`);

        // Reset time decay timer (successful consolidation)
        this.adaptiveThreshold.recordConsolidation();
      } else if (failedCount > 0) {
        this.log(`ERROR: All ${failedCount} consolidation attempts failed`);
      }

      // Update metrics
      this.metrics = {
        ...this.metrics,
        lastConsolidation: new Date(),
        consolidationCount: this.metrics.consolidationCount + consolidatedIds.length
      };
    } catch (error) {
      this.log(`❌ Consolidation failed: ${error}`);
    }
  }

  /**
   * Calculate recent activity level (messages per minute equivalent)
   * Aggregates working memory usage across all domains
   */
  private async calculateActivityLevel(): Promise<number> {
    try {
      // Aggregate activity across all domains
      const domains = ['chat', 'game', 'ui', 'browsing', 'code'];
      let totalUsed = 0;
      let domainsChecked = 0;

      for (const domain of domains) {
        try {
          const capacity = await this.persona.prefrontal?.workingMemory?.getCapacity(domain);
          if (capacity) {
            totalUsed += capacity.used;
            domainsChecked++;
          }
        } catch {
          // Domain not configured - skip
        }
      }

      if (domainsChecked === 0) {
        return 1.0;  // Default to moderate activity if no domains available
      }

      // Rough heuristic: working memory size indicates recent activity
      // If working memory is 40/100, estimate ~4 msg/min over last 10 min
      const estimatedRate = totalUsed / 10.0;

      return Math.max(0.1, estimatedRate);  // Minimum 0.1 to avoid division by zero
    } catch (error) {
      this.log(`WARN: Could not calculate activity level: ${error}`);
      return 1.0;  // Default to moderate activity
    }
  }

  /**
   * Map WorkingMemory thoughtType to MemoryType enum
   */
  private mapThoughtTypeToMemoryType(thoughtType: string): MemoryType {
    // Map common thought types to memory types
    if (thoughtType.includes('decision')) return MemoryTypeEnum.DECISION;
    if (thoughtType.includes('observation')) return MemoryTypeEnum.OBSERVATION;
    if (thoughtType.includes('task')) return MemoryTypeEnum.TASK;
    if (thoughtType.includes('tool')) return MemoryTypeEnum.TOOL_USE;
    if (thoughtType.includes('error')) return MemoryTypeEnum.ERROR;
    if (thoughtType.includes('insight')) return MemoryTypeEnum.INSIGHT;
    return MemoryTypeEnum.OBSERVATION; // Default
  }

  /**
   * Get memory system statistics
   */
  public async getStats(): Promise<MemoryStats> {
    // Aggregate working memory size across all domains
    let workingMemorySize = 0;
    const domains = ['chat', 'game', 'ui', 'browsing', 'code'];
    for (const domain of domains) {
      try {
        const capacity = await this.persona.prefrontal?.workingMemory?.getCapacity(domain);
        if (capacity) {
          workingMemorySize += capacity.used;
        }
      } catch {
        // Domain not configured
      }
    }

    // Query actual LTM count from database
    // Note: No personaId filter needed - each persona has their own database file
    // This also fixes the orphaned memories bug when personas get new UUIDs on reseed
    let ltmCount = 0;
    if (this.memoryDbHandle) {
      try {
        const result = await Commands.execute(DATA_COMMANDS.LIST, {
          dbHandle: this.memoryDbHandle,
          collection: 'memories',
          limit: 0  // Just get count
        } as any) as any;
        ltmCount = result.totalCount || result.items?.length || 0;
      } catch (error) {
        this.log(`WARN: Could not get LTM count: ${error}`);
      }
    }

    return {
      stmSize: workingMemorySize, // WorkingMemory acts as STM
      stmMaxSize: 100, // WorkingMemory max capacity
      ltmCount,
      consolidationCount: this.metrics.consolidationCount,
      lastConsolidation: this.metrics.lastConsolidation || undefined
    };
  }

  /**
   * Get current consolidation metrics
   */
  public getMetrics(): Readonly<ConsolidationMetrics> {
    return this.metrics;
  }
}
