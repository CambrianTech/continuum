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
import type { PersonaUser } from '../../../PersonaUser';
import { Commands } from '../../../../../core/shared/Commands';
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
    // Use persona's own modelConfig for synthesizing their own memories (single source of truth)
    this.consolidationAdapter = adapter || new SemanticCompressionAdapter({
      modelConfig: persona.modelConfig,  // Pass entire config - same model/provider as inference
      maxThoughtsPerGroup: 10
    });

    this.log(`Initialized with ${this.consolidationAdapter.getName()} adapter (model: ${persona.modelConfig.model}, provider: ${persona.modelConfig.provider})`);

    // Initialize database asynchronously (non-blocking)
    this.initializePromise = this.initializeDatabase();
  }

  /**
   * Initialize long-term memory database
   * Opens dedicated SQLite database for this persona
   */
  private async initializeDatabase(): Promise<void> {
    const personaDirName = this.getPersonaDirName();
    const dbPath = `.continuum/personas/${personaDirName}/memory/longterm.db`;

    try {
      this.log(`Opening LTM database: ${dbPath}`);

      const result = await Commands.execute<DataOpenParams, DataOpenResult>('data/open', {
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

      // Ensure schema exists
      await this.ensureSchema();

      this.log('LTM database initialized successfully');
    } catch (error) {
      const errorMsg = String(error);

      // EXFAT FIX: Detect corrupted database and recover by deleting and retrying
      if (errorMsg.includes('SQLITE_CORRUPT')) {
        this.log(`⚠️ Detected corrupted database, attempting recovery...`);
        console.warn(`⚠️ [Hippocampus] Corrupted database detected at ${dbPath}, deleting and recreating...`);

        try {
          // Delete corrupted database
          const fs = await import('fs/promises');
          await fs.unlink(dbPath);
          this.log(`✅ Deleted corrupted database: ${dbPath}`);

          // Retry opening (will create fresh database)
          this.log(`🔄 Retrying database initialization with fresh file...`);
          const result = await Commands.execute<DataOpenParams, DataOpenResult>('data/open', {
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

          await this.ensureSchema();
          this.log('✅ LTM database recovered and initialized successfully');
          return;
        } catch (recoveryError) {
          this.log(`ERROR: Recovery failed: ${recoveryError}`);
          console.error(`❌ [Hippocampus] Recovery failed:`, recoveryError);
        }
      }

      // If not corruption or recovery failed, log and continue in STM-only mode
      this.log(`ERROR: Failed to initialize LTM database: ${error}`);
      console.error(`❌ [Hippocampus] Failed to initialize database:`, error);
      // Continue without LTM (STM-only mode)
    }
  }

  /**
   * Ensure schema exists (adapter handles table creation)
   *
   * Note: No direct table creation. The adapter (SqliteStorageAdapter) creates
   * simple entity table with: id, data (JSON), created_at, updated_at, version
   * All field names are snake_case - adapter responsibility, not ours.
   */
  private async ensureSchema(): Promise<void> {
    if (!this.memoryDbHandle) return;

    try {
      this.log('Ensuring LTM schema...');

      // Trigger table creation - adapter will handle it automatically
      // Simple entity table: id, data (JSON), created_at, updated_at, version
      await Commands.execute('data/list', {
        dbHandle: this.memoryDbHandle,
        collection: 'memories',
        limit: 1
      } as any);

      this.log('✅ LTM schema ready (adapter created table with snake_case columns)');
    } catch (error) {
      this.log(`ERROR: Failed to ensure schema: ${error}`);
      console.error(`❌ [Hippocampus] Failed to ensure schema:`, error);
    }
  }

  /**
   * Get persona directory name (helper-ai-154ee833)
   */
  private getPersonaDirName(): string {
    const displayName = this.persona.entity.displayName
      .toLowerCase()
      .replace(/\s+/g, '-')
      .replace(/[^a-z0-9-]/g, '');

    const shortId = this.persona.id.substring(0, 8);
    return `${displayName}-${shortId}`;
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
   * Search long-term memory (database)
   */
  private async searchLTM(params: RecallParams): Promise<MemoryEntity[]> {
    if (!this.memoryDbHandle) {
      return [];
    }

    try {
      // Build filter
      const filter: Record<string, any> = {
        personaId: this.persona.id
      };

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
      const result = (await Commands.execute('data/list', {
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

    // Snoop on PersonaUser's working memory (thoughts during cognition)
    if (this.persona.workingMemory) {
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
      const thoughts = await this.persona.workingMemory.recall({
        minImportance: currentThreshold,  // ← ADAPTIVE!
        limit: 50,
        includePrivate: true
      });

      if (thoughts.length === 0) {
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
          const result = await Commands.execute<DataCreateParams, DataCreateResult<any>>('data/create', {
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
              const matchingThought = thoughts.find(t =>
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
          this.log(`ERROR: Failed to store memory ${memory.id}: ${error}`);
          console.error(`❌ [Hippocampus] Insert failed for memory ${memory.id}:`, error);
        }
      }

      // Remove ONLY successfully consolidated thoughts from working memory
      if (consolidatedIds.length > 0) {
        await this.persona.workingMemory.clearBatch(consolidatedIds as any);
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
      this.log(`ERROR: Consolidation failed: ${error}`);
      console.error(`❌ [Hippocampus] Consolidation failed:`, error);
    }
  }

  /**
   * Calculate recent conversation activity (messages per minute)
   * Estimates from working memory size as proxy
   */
  private async calculateActivityLevel(): Promise<number> {
    try {
      // Get working memory capacity usage
      const capacity = await this.persona.workingMemory.getCapacity('chat');

      // Rough heuristic: working memory size indicates recent activity
      // If working memory is 40/100, estimate ~4 msg/min over last 10 min
      const estimatedRate = capacity.used / 10.0;

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
    // Get working memory size (source of consolidation)
    const workingMemorySize = this.persona.workingMemory
      ? (await this.persona.workingMemory.getCapacity('chat')).used  // TODO: aggregate all domains
      : 0;

    return {
      stmSize: workingMemorySize, // WorkingMemory acts as STM
      stmMaxSize: 100, // WorkingMemory max capacity
      ltmCount: 0, // TODO: Query LTM count from database
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
