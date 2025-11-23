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
  private readonly CONSOLIDATION_THRESHOLD = 0.6; // Importance threshold for LTM

  // Metrics
  private metrics: ConsolidationMetrics;
  private initializePromise: Promise<void> | null = null;

  constructor(persona: PersonaUser) {
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
   * Create database schema for memories
   *
   * CRITICAL FIX: SqliteStorageAdapter creates ALL entity tables from ENTITY_REGISTRY
   * for every database opened. This pollutes persona memory databases with chat_messages,
   * users, rooms, etc. We fix this by:
   * 1. Opening database directly with better-sqlite3
   * 2. Dropping all unwanted tables
   * 3. Creating ONLY the memories table
   */
  private async ensureSchema(): Promise<void> {
    if (!this.memoryDbHandle) return;

    try {
      this.log('Creating LTM schema (memories table only)...');

      // Import better-sqlite3 directly for schema management
      const Database = require('better-sqlite3');
      const personaDirName = this.getPersonaDirName();
      const dbPath = `.continuum/personas/${personaDirName}/memory/longterm.db`;

      const db = new Database(dbPath);

      // Get list of all tables (excluding SQLite internal tables)
      const tables = db.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
      ).all() as Array<{ name: string }>;

      // Drop all tables except memories
      for (const table of tables) {
        if (table.name !== 'memories') {
          this.log(`Dropping unwanted table: ${table.name}`);
          db.exec(`DROP TABLE IF EXISTS ${table.name}`);
        }
      }

      // Create memories table with proper schema
      db.exec(`
        CREATE TABLE IF NOT EXISTS memories (
          id TEXT PRIMARY KEY,
          createdAt TEXT NOT NULL,
          updatedAt TEXT NOT NULL,
          version INTEGER NOT NULL DEFAULT 0,
          personaId TEXT NOT NULL,
          sessionId TEXT NOT NULL,
          type TEXT NOT NULL,
          content TEXT NOT NULL,
          context TEXT NOT NULL,
          timestamp TEXT NOT NULL,
          consolidatedAt TEXT,
          lastAccessedAt TEXT,
          importance REAL NOT NULL,
          accessCount INTEGER NOT NULL DEFAULT 0,
          relatedTo TEXT NOT NULL,
          tags TEXT NOT NULL,
          source TEXT NOT NULL,
          embedding TEXT
        )
      `);

      // Create indexes for common queries
      db.exec(`
        CREATE INDEX IF NOT EXISTS idx_memories_persona ON memories(personaId);
        CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(sessionId);
        CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(type);
        CREATE INDEX IF NOT EXISTS idx_memories_timestamp ON memories(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance DESC);
      `);

      db.close();

      this.log('✅ LTM schema ready (memories table only)');
    } catch (error) {
      this.log(`ERROR: Failed to create schema: ${error}`);
      console.error(`❌ [Hippocampus] Failed to create schema:`, error);
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
   */
  private async snoopAndConsolidate(): Promise<void> {
    if (!this.memoryDbHandle || !this.persona.sessionId) {
      return; // Can't consolidate without LTM or session
    }

    try {
      // Recall important thoughts from working memory
      const thoughts = await this.persona.workingMemory.recall({
        minImportance: this.CONSOLIDATION_THRESHOLD, // 0.6
        limit: 50,
        includePrivate: true // Consolidate all thoughts
      });

      if (thoughts.length === 0) {
        return;
      }

      // Convert WorkingMemoryEntry → MemoryEntity and write to LTM
      const consolidatedIds: string[] = [];
      for (const thought of thoughts) {
        const memory: MemoryEntity = {
          id: generateUUID(),
          createdAt: ISOString(new Date().toISOString()),
          updatedAt: ISOString(new Date().toISOString()),
          version: 0,
          personaId: this.persona.id,
          sessionId: this.persona.sessionId,
          type: this.mapThoughtTypeToMemoryType(thought.thoughtType),
          content: thought.thoughtContent,
          context: {
            domain: thought.domain,
            contextId: thought.contextId,
            thoughtType: thought.thoughtType,
            shareable: thought.shareable
          },
          timestamp: ISOString(new Date(thought.createdAt).toISOString()),
          consolidatedAt: ISOString(new Date().toISOString()),
          importance: thought.importance,
          accessCount: 0,
          relatedTo: [],
          tags: thought.domain ? [thought.domain] : [],
          source: 'working-memory'
        };

        // Write to LTM
        await Commands.execute<DataCreateParams, DataCreateResult<any>>('data/create', {
          dbHandle: this.memoryDbHandle,
          collection: 'memories',
          data: memory
        } as any);

        consolidatedIds.push(thought.id);
      }

      // Remove consolidated thoughts from working memory
      await this.persona.workingMemory.clearBatch(consolidatedIds as any);

      this.log(`Consolidated ${consolidatedIds.length} thoughts to LTM`);

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
