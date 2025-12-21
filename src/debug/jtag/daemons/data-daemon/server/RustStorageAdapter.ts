/**
 * Rust Storage Adapter - Drop-in Replacement for SqliteStorageAdapter
 *
 * Identical to SqliteStorageAdapter but uses Rust worker for SQL execution.
 * All decorator logic, schema generation, and query building remains in TypeScript.
 * Only the raw SQL execution is delegated to Rust.
 *
 * Phase 1: Isolated testing (manual worker start, test file only)
 * Phase 2+: Integration into production (future)
 */

import * as path from 'path';
import * as fs from 'fs/promises';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type CollectionStats,
  type StorageOperation,
  type RecordData,
  type QueryExplanation
} from '../shared/DataStorageAdapter';
import { SqlStorageAdapterBase, type SqlDialect, type SqlValue } from './SqlStorageAdapterBase';
import { getDatabasePath } from '../../../system/config/ServerConfig';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { getFieldMetadata, hasFieldMetadata, type FieldMetadata, type FieldType } from '../../../system/data/decorators/FieldDecorators';
import {
  type VectorSearchAdapter,
  type VectorSearchOptions,
  type VectorSearchResponse,
  type GenerateEmbeddingRequest,
  type GenerateEmbeddingResponse,
  type IndexVectorRequest,
  type BackfillVectorsRequest,
  type BackfillVectorsProgress,
  type VectorIndexStats,
  type VectorSearchCapabilities
} from '../shared/VectorSearchTypes';
import { SqlNamingConverter } from '../shared/SqlNamingConverter';
import { RustSqliteExecutor } from './RustSqliteExecutor';
import { SqliteTransactionManager } from './SqliteTransactionManager';
import { SqliteSchemaManager } from './managers/SqliteSchemaManager';
import { SqliteQueryExecutor } from './managers/SqliteQueryExecutor';
import { SqliteWriteManager } from './managers/SqliteWriteManager';
import { SqliteVectorSearchManager } from './managers/SqliteVectorSearchManager';
import { ENTITY_REGISTRY, registerEntity, getRegisteredEntity, type EntityConstructor } from './EntityRegistry';
import { Logger } from '../../../system/core/logging/Logger';

const log = Logger.create('RustStorageAdapter', 'sql');

/**
 * Rust SQLite Configuration Options
 */
interface RustSqliteOptions {
  filename?: string;          // Database file path
  socketPath?: string;        // Unix socket path to Rust worker
  dbHandle?: string;          // Database handle name (for multi-db support)
  foreignKeys?: boolean;      // Enable foreign key constraints
  synchronous?: 'OFF' | 'NORMAL' | 'FULL';
  journalMode?: 'DELETE' | 'WAL' | 'MEMORY';
  cacheSize?: number;         // Page cache size
  timeout?: number;           // Busy timeout in ms
}

// Re-export entity registry functions for backwards compatibility
export { registerEntity, getRegisteredEntity, type EntityConstructor };

/**
 * Rust Storage Adapter - Uses Rust worker for SQL execution
 *
 * Architecture:
 *   1. TypeScript generates SQL from decorators (unchanged)
 *   2. RustSqliteExecutor sends SQL to Rust worker via Unix socket
 *   3. Rust worker executes SQL via rusqlite connection pool
 *   4. Results flow back through RustSqliteExecutor to TypeScript
 */
export class RustStorageAdapter extends SqlStorageAdapterBase implements VectorSearchAdapter {
  private config: StorageAdapterConfig | null = null;
  private isInitialized: boolean = false;

  // Rust-backed executor (ORM bridge to Rust worker)
  private executor!: RustSqliteExecutor;
  private transactionManager!: SqliteTransactionManager;

  // Manager classes
  // NOTE: queryExecutor NOT used - we use ORM pattern via executor.queryRecords()
  private schemaManager!: SqliteSchemaManager;
  private writeManager!: SqliteWriteManager;
  private vectorSearchManager!: SqliteVectorSearchManager;

  /**
   * SqlStorageAdapterBase abstract method implementations
   */
  protected getSqlDialect(): SqlDialect {
    return 'sqlite';
  }

  protected async executeRawSql(sql: string, params?: SqlValue[]): Promise<Record<string, unknown>[]> {
    return this.executor.runSql(sql, params || []);
  }

  protected async executeRawStatement(sql: string, params?: SqlValue[]): Promise<{ lastID?: number; changes: number }> {
    return this.executor.runStatement(sql, params || []);
  }

  /**
   * Initialize Rust-backed SQLite adapter
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    if (this.isInitialized) {
      log.debug('Already initialized, skipping');
      return;
    }

    log.info('Starting Rust-backed initialization...');

    this.config = config;
    const options = config.options as RustSqliteOptions || {};

    // Use explicit filename from options, or fall back to default database path
    const dbPath = options.filename || getDatabasePath();
    log.info(`Using database path: ${dbPath}`);

    // Socket path to Rust worker (matches workers-config.json)
    const socketPath = options.socketPath || '/tmp/jtag-data-daemon-worker.sock';
    log.info(`Using Rust worker socket: ${socketPath}`);

    // Ensure directory exists with proper permissions
    const dbDir = path.dirname(dbPath);

    // Skip permission changes for system directories (/tmp, /var, etc.)
    const isSystemDir = ['/tmp', '/var', '/usr', '/etc'].some(sysDir =>
      dbDir === sysDir || dbDir.startsWith(sysDir + '/')
    );

    const oldUmask = process.umask(0o000);
    log.debug(`Saved umask ${oldUmask.toString(8)}, set to 0o000 for permission control`);

    try {
      log.debug(`Ensuring directory exists: ${dbDir}`);
      await fs.mkdir(dbDir, { recursive: true, mode: 0o755 });

      if (!isSystemDir) {
        log.debug('Setting directory permissions to 0o755');
        await fs.chmod(dbDir, 0o755);
        log.debug('Directory permissions set successfully');
      } else {
        log.debug('Skipping chmod on system directory');
      }

      // Clear extended attributes on directory (macOS)
      if (process.platform === 'darwin' && !isSystemDir) {
        try {
          log.debug('Clearing directory extended attributes');
          await execAsync(`xattr -c "${dbDir}"`);
          log.debug('Directory extended attributes cleared');
        } catch (error) {
          log.debug('Could not clear directory xattr (non-fatal):', error);
        }
      }

      // Check if database file exists before connection
      let dbFileExists = false;
      try {
        const stats = await fs.stat(dbPath);
        log.debug(`Existing database found - Size: ${stats.size} bytes, Mode: ${stats.mode.toString(8)}`);
        dbFileExists = true;
      } catch (error) {
        log.debug('No existing database file, will create new');
      }

      // Create empty file BEFORE opening connection
      if (!dbFileExists) {
        log.debug('Creating empty database file');
        await fs.writeFile(dbPath, '', { mode: 0o666 });
        log.debug('Empty file created with mode 0o666');
      }

      if (!isSystemDir) {
        log.debug('Setting file permissions to 0o666');
        await fs.chmod(dbPath, 0o666);
        log.debug('File permissions set successfully');
      } else {
        log.debug('Skipping chmod on system directory file');
      }

      // Clear extended attributes on macOS BEFORE opening connection
      if (process.platform === 'darwin' && !isSystemDir) {
        try {
          log.debug('Clearing macOS extended attributes');
          await execAsync(`xattr -c "${dbPath}"`);
          log.debug('Extended attributes cleared');
        } catch (error) {
          log.debug('Could not clear extended attributes (non-fatal):', error);
        }
      }
    } finally {
      // Restore original umask
      process.umask(oldUmask);
      log.debug(`Restored umask to ${oldUmask.toString(8)}`);
    }

    log.info('Creating Rust executor connection to worker');

    // Initialize Rust-backed executor (replaces SqliteRawExecutor)
    this.executor = new RustSqliteExecutor(
      dbPath,
      options.dbHandle,
      socketPath
    );

    // Test connection to Rust worker
    try {
      await this.executor.runSql('SELECT 1 as test', []);
      log.debug('Rust worker connection verified');
    } catch (error) {
      log.error('Failed to connect to Rust worker:', error);
      throw new Error(`Rust worker not available at ${socketPath}. Start it with: ./workers/data/target/release/data-worker ${socketPath}`);
    }

    // Initialize transaction manager (uses executor)
    log.debug('Initializing transaction manager');
    this.transactionManager = new SqliteTransactionManager(this.executor);
    log.debug('Transaction manager initialized');

    // Initialize schema manager
    log.debug('Initializing schema manager');
    this.schemaManager = new SqliteSchemaManager(
      null,  // No direct DB handle for Rust adapter
      this.executor,
      this.generateCreateTableSql.bind(this),
      this.generateCreateIndexSql.bind(this),
      this.mapFieldTypeToSql.bind(this)
    );
    log.debug('Schema manager initialized');

    // NOTE: No SqliteQueryExecutor - we use ORM pattern via executor.queryRecords()
    // This is the CORRECT abstraction: send StorageQuery, not raw SQL

    // Initialize write manager
    log.debug('Initializing write manager');
    this.writeManager = new SqliteWriteManager(this.executor);
    log.debug('Write manager initialized');

    log.debug('Configuring database settings (via Rust worker)');
    // Configure SQLite settings
    await this.schemaManager.configureSqlite(options);

    log.debug('Initializing entity registry');
    // Import and register all known entities (server-side only)
    const { initializeEntityRegistry } = await import('./EntityRegistry');
    initializeEntityRegistry();

    log.debug('Entity registry initialized (tables created lazily on first use)');

    // Verify integrity after initialization
    log.debug('Verifying database integrity');
    await this.schemaManager.verifyIntegrity();

    // Initialize vector search manager
    log.debug('Initializing vector search manager');
    this.vectorSearchManager = new SqliteVectorSearchManager(
      this.executor,
      this  // DataStorageAdapter for CRUD operations
    );
    log.debug('Vector search manager initialized');

    this.isInitialized = true;
    log.info('Rust-backed initialization complete');
  }

  /**
   * Ensure schema exists for collection (orchestrated by DataDaemon)
   * Delegates to SqliteSchemaManager
   */
  async ensureSchema(collectionName: string, _schema?: unknown): Promise<StorageResult<boolean>> {
    return this.schemaManager.ensureSchema(collectionName, _schema);
  }

  /**
   * Execute operations within a transaction for atomic consistency
   * Delegated to SqliteTransactionManager
   */
  private async withTransaction<T>(operation: () => Promise<T>): Promise<T> {
    return this.transactionManager.withTransaction(operation);
  }

  /**
   * Create a record with proper relational schema
   * Delegates to SqliteWriteManager
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    await this.ensureSchema(record.collection);
    return this.writeManager.create<T>(record.collection, record.data, record.id);
  }

  /**
   * Read a single record by ID
   *
   * ORM PATTERN: Sends read-record message to Rust worker.
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    await this.ensureSchema(collection);

    try {
      // Send ORM read to Rust worker (NOT raw SQL!)
      const record = await this.executor.readRecord(collection, id);

      if (!record) {
        return {
          success: false,
          error: `Record not found: ${collection}/${id}`
        };
      }

      // Wrap in DataRecord format
      const dataRecord: DataRecord<T> = {
        id: record.id,
        collection,
        data: record.data as T,
        metadata: {
          createdAt: record.created_at,
          updatedAt: record.updated_at,
          version: record.version
        }
      };

      return {
        success: true,
        data: dataRecord
      };
    } catch (error: any) {
      log.error(`ORM read failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Query records with complex filters
   *
   * ORM PATTERN: Sends high-level StorageQuery to Rust worker.
   * Rust worker translates filter operators to SQL WHERE clauses.
   * Clean separation: TypeScript = application logic, Rust = storage logic.
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    await this.ensureSchema(query.collection);

    try {
      // Send ORM query to Rust worker (NOT raw SQL!)
      const records = await this.executor.queryRecords(query);

      // Rust worker returns raw data - wrap in DataRecord format
      const dataRecords: DataRecord<T>[] = records.map(record => ({
        id: record.id,
        collection: query.collection,
        data: record.data as T,
        metadata: {
          createdAt: record.created_at,
          updatedAt: record.updated_at,
          version: record.version
        }
      }));

      return {
        success: true,
        data: dataRecords
      };
    } catch (error: any) {
      log.error(`ORM query failed for ${query.collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Count records matching query without fetching data
   *
   * ORM PATTERN: Sends high-level StorageQuery to Rust worker.
   * Much more efficient than fetching all records - translates to COUNT(*) SQL.
   */
  async count(query: StorageQuery): Promise<StorageResult<number>> {
    await this.ensureSchema(query.collection);

    try {
      // Send ORM count to Rust worker (NOT raw SQL!)
      const count = await this.executor.countRecords(query);

      return {
        success: true,
        data: count
      };
    } catch (error: any) {
      log.error(`ORM count failed for ${query.collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Update an existing record
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      // First read existing record to get current version
      const existing = await this.read<T>(collection, id);
      if (!existing.success || !existing.data) {
        return {
          success: false,
          error: 'Record not found'
        };
      }

      // Merge data
      const updatedData = { ...existing.data.data, ...data };
      const version = incrementVersion ? existing.data.metadata.version + 1 : existing.data.metadata.version;

      // Delegate to write manager
      return this.writeManager.update<T>(collection, id, updatedData as T, version);

    } catch (error: any) {
      log.error(`Update failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Delete a record
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    await this.ensureSchema(collection);
    return this.writeManager.delete(collection, id);
  }

  /**
   * Batch create records
   */
  async batchCreate<T extends RecordData>(
    collection: string,
    records: T[]
  ): Promise<StorageResult<DataRecord<T>[]>> {
    await this.ensureSchema(collection);
    return this.writeManager.batchCreate<T>(collection, records);
  }

  /**
   * Batch update records
   */
  async batchUpdate<T extends RecordData>(
    collection: string,
    updates: Array<{ id: UUID; data: Partial<T>; version?: number }>
  ): Promise<StorageResult<DataRecord<T>[]>> {
    return this.writeManager.batchUpdate<T>(collection, updates);
  }

  /**
   * Batch delete records
   */
  async batchDelete(
    collection: string,
    ids: UUID[]
  ): Promise<StorageResult<boolean>> {
    return this.writeManager.batchDelete(collection, ids);
  }

  /**
   * List collections (entity tables from sqlite_master)
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      const sql = `
        SELECT name FROM sqlite_master
        WHERE type='table'
        AND name NOT LIKE 'sqlite_%'
        AND name NOT IN ('system_info', '_data', '_collections')
        ORDER BY name
      `;
      const rows = await this.executor.runSql(sql);

      const collections = rows.map(row => row.name);

      return {
        success: true,
        data: collections
      };

    } catch (error: any) {
      log.error('List collections failed:', error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Get collection statistics
   */
  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    try {
      const tableName = SqlNamingConverter.toTableName(collection);

      // Count records directly from entity table
      const countSql = `SELECT COUNT(*) as count FROM ${tableName}`;
      const countRows = await this.executor.runSql(countSql);
      const recordCount = countRows[0]?.count || 0;

      // Get table info
      const infoSql = `SELECT sql FROM sqlite_master WHERE type='table' AND name = ?`;
      const infoRows = await this.executor.runSql(infoSql, [tableName]);

      if (infoRows.length === 0) {
        return {
          success: true,
          data: undefined
        };
      }

      const stats: CollectionStats = {
        name: collection,
        recordCount: recordCount,
        totalSize: 0,
        lastModified: new Date().toISOString(),
        schema: 'v1'
      };

      return {
        success: true,
        data: stats
      };

    } catch (error: any) {
      log.error(`Get stats failed for ${collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Batch operations with transaction support
   */
  async batch<T extends RecordData = RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    const results: any[] = [];
    let hasError = false;
    let errorMessage = '';

    const processOperations = async () => {
      try {
        for (const op of operations) {
          switch (op.type) {
            case 'create':
              if (op.data && op.collection) {
                const record: DataRecord<T> = {
                  id: op.id || `batch_${Date.now()}_${Math.random()}`,
                  collection: op.collection,
                  data: op.data as T,
                  metadata: {
                    createdAt: new Date().toISOString(),
                    updatedAt: new Date().toISOString(),
                    version: 1
                  }
                };
                const result = await this.create(record);
                results.push(result);
              }
              break;

            case 'read':
              if (op.collection && op.id) {
                const result = await this.read(op.collection, op.id);
                results.push(result);
              }
              break;

            case 'update':
              if (op.collection && op.id && op.data) {
                const result = await this.update(op.collection, op.id, op.data as Partial<T>);
                results.push(result);
              }
              break;

            case 'delete':
              if (op.collection && op.id) {
                const result = await this.delete(op.collection, op.id);
                results.push(result);
              }
              break;
          }
        }
      } catch (error: any) {
        hasError = true;
        errorMessage = error.message;
      }
    };

    await this.withTransaction(processOperations);

    if (hasError) {
      return {
        success: false,
        error: errorMessage,
        data: results
      };
    } else {
      return {
        success: true,
        data: results
      };
    }
  }

  /**
   * Clear all data from all collections
   */
  async clear(): Promise<StorageResult<boolean>> {
    if (!this.isInitialized) {
      return {
        success: false,
        error: 'Adapter not initialized'
      };
    }

    try {
      const result = await this.withTransaction(async () => {
        // Get all entity tables
        const tables = await this.executor.runSql(`
          SELECT name FROM sqlite_master
          WHERE type='table'
          AND name NOT LIKE 'sqlite_%'
          AND name NOT IN ('system_info', '_data', '_collections')
        `);

        // Delete from each entity table
        for (const table of tables) {
          await this.executor.runStatement(`DELETE FROM ${table.name}`);
        }

        return true;
      });

      log.info('All entity data cleared');
      return {
        success: true,
        data: result
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      log.error('Error clearing data:', errorMessage);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Clear all records from a specific collection
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    if (!this.isInitialized) {
      return {
        success: false,
        error: 'Adapter not initialized'
      };
    }

    try {
      const tableName: string = SqlNamingConverter.toTableName(collection);

      // Validate table name to prevent SQL injection
      if (!/^[a-z0-9_]+$/i.test(tableName)) {
        throw new Error(`Invalid table name: ${tableName}`);
      }

      const result: number = await this.withTransaction(async (): Promise<number> => {
        const deleteResult = await this.executor.runStatement(`DELETE FROM ${tableName}`, []);
        return deleteResult.changes ?? 0;
      });

      log.info(`Truncated collection '${collection}' - ${result} records removed`);
      return {
        success: true,
        data: result > 0
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      log.error(`Error truncating collection '${collection}':`, errorMessage);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Cleanup and optimization
   */
  async cleanup(): Promise<void> {
    try {
      // ANALYZE to update statistics
      await this.executor.runStatement('ANALYZE');

      log.info('Cleanup completed');

    } catch (error) {
      log.error('Cleanup failed:', error);
    }
  }

  /**
   * Close connection (no-op for Rust worker - connection managed by worker)
   */
  async close(): Promise<void> {
    log.info('Closing RustStorageAdapter (Rust worker manages connection pool)');
    this.isInitialized = false;
  }

  /**
   * Clear all entity data from the database (preserving structure)
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    if (!this.isInitialized) {
      throw new Error('RustStorageAdapter not initialized');
    }

    log.info('Starting complete database clear (preserving structure)');

    const tablesCleared: string[] = [];
    let totalRecordsDeleted = 0;

    try {
      await this.withTransaction(async () => {
        // Get list of all tables to clear
        const tables = await this.executor.runSql(`
          SELECT name FROM sqlite_master
          WHERE type='table'
          AND name NOT LIKE 'sqlite_%'
        `);

        for (const table of tables) {
          const tableName = table.name;

          // Count records before deletion
          const countRows = await this.executor.runSql(`SELECT COUNT(*) as count FROM \`${tableName}\``);
          const recordCount = countRows[0]?.count || 0;

          if (recordCount > 0) {
            // Delete all records from this table
            await this.executor.runStatement(`DELETE FROM \`${tableName}\``);

            tablesCleared.push(tableName);
            totalRecordsDeleted += recordCount;

            log.debug(`Cleared ${recordCount} records from table '${tableName}'`);
          } else {
            log.debug(`Table '${tableName}' was already empty`);
          }
        }

        // Reset SQLite sequence counters
        const sequenceTables = await this.executor.runSql(`
          SELECT name FROM sqlite_sequence
        `);

        for (const seqTable of sequenceTables) {
          await this.executor.runStatement(`UPDATE sqlite_sequence SET seq = 0 WHERE name = ?`, [seqTable.name]);
        }
      });

      log.info(`Database clearing complete - ${tablesCleared.length} tables processed, ${totalRecordsDeleted} records deleted`);

      return {
        success: true,
        data: {
          tablesCleared,
          recordsDeleted: totalRecordsDeleted
        }
      };

    } catch (error) {
      log.error('Database clear failed:', error);
      throw new Error(`Database clear failed: ${error}`);
    }
  }

  /**
   * Explain query execution (dry-run)
   *
   * ORM PATTERN: Shows the high-level query structure sent to Rust worker.
   * Rust worker translates this to SQL internally.
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    // In ORM pattern, we don't generate SQL in TypeScript
    // Return explanation of the ORM query structure
    const executionPlan = [
      `ORM Query Pattern (Rust-translated):`,
      `  Collection: ${query.collection}`,
      `  Filter: ${query.filter ? JSON.stringify(query.filter) : 'none'}`,
      `  Sort: ${query.sort ? JSON.stringify(query.sort) : 'none'}`,
      `  Limit: ${query.limit || 'none'}`,
      `  Cursor: ${query.cursor ? JSON.stringify(query.cursor) : 'none'}`,
      ``,
      `This query is sent to Rust worker as JSON.`,
      `Rust worker translates filter operators to SQL WHERE clauses.`,
      `SQL generation happens in Rust adapter (single source of truth).`
    ].join('\n');

    return {
      query,
      translatedQuery: 'ORM query (translated to SQL in Rust worker)',
      parameters: query.filter ? [query.filter] : [],
      estimatedRows: 0,
      executionPlan,
      adapterType: 'RustStorageAdapter (ORM)',
      timestamp: new Date().toISOString()
    };
  }

  // ============================================================================
  // VECTOR SEARCH ADAPTER INTERFACE - Delegate to SqliteVectorSearchManager
  // ============================================================================

  async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    return this.vectorSearchManager.vectorSearch<T>(options);
  }

  async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    return this.vectorSearchManager.generateEmbedding(request);
  }

  async indexVector(request: IndexVectorRequest): Promise<StorageResult<boolean>> {
    return this.vectorSearchManager.indexVector(request);
  }

  async backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    return this.vectorSearchManager.backfillVectors(request, onProgress);
  }

  async getVectorIndexStats(collection: string): Promise<StorageResult<VectorIndexStats>> {
    return this.vectorSearchManager.getVectorIndexStats(collection);
  }

  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities> {
    return this.vectorSearchManager.getVectorSearchCapabilities();
  }
}
