/**
 * SQLite Storage Adapter - SQL Database Backend
 *
 * Implements full SQL functionality with joins, transactions, and indexing
 * Foundation for PostgreSQL/MySQL adapters with 90% code reuse
 */

import sqlite3 from 'sqlite3';
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
import { DATABASE_PATHS } from '../../../system/data/config/DatabaseConfig';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { SqliteQueryBuilder } from './SqliteQueryBuilder';
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
import { SqliteRawExecutor } from './SqliteRawExecutor';
import { SqliteTransactionManager } from './SqliteTransactionManager';
import { SqliteSchemaManager } from './managers/SqliteSchemaManager';
import { SqliteQueryExecutor } from './managers/SqliteQueryExecutor';
import { SqliteWriteManager } from './managers/SqliteWriteManager';
import { SqliteVectorSearchManager } from './managers/SqliteVectorSearchManager';
import { ENTITY_REGISTRY, registerEntity, getRegisteredEntity, type EntityConstructor } from './EntityRegistry';

/**
 * SQLite Configuration Options
 */
interface SqliteOptions {
  filename?: string;          // Database file path
  mode?: number;             // SQLite open mode
  foreignKeys?: boolean;     // Enable foreign key constraints
  wal?: boolean;            // Write-Ahead Logging
  synchronous?: 'OFF' | 'NORMAL' | 'FULL';
  journalMode?: 'DELETE' | 'WAL' | 'MEMORY';
  cacheSize?: number;       // Page cache size
  timeout?: number;         // Busy timeout in ms
}

// Re-export entity registry functions for backwards compatibility
export { registerEntity, getRegisteredEntity, type EntityConstructor };

/**
 * SQLite Storage Adapter with Proper Relational Schema
 */
export class SqliteStorageAdapter extends SqlStorageAdapterBase implements VectorSearchAdapter {
  private db: sqlite3.Database | null = null;
  private config: StorageAdapterConfig | null = null;
  private isInitialized: boolean = false;

  // Extracted utility classes (Phase 1 refactoring)
  private executor!: SqliteRawExecutor;
  private transactionManager!: SqliteTransactionManager;

  // Extracted manager classes (Phase 0 refactoring)
  private schemaManager!: SqliteSchemaManager;
  private queryExecutor!: SqliteQueryExecutor;
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
   * Initialize SQLite database with configuration
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    console.log('🚀 SQLite: Starting initialization...');

    if (this.isInitialized && this.db) {
      console.log('✅ SQLite: Already initialized, skipping');
      return;
    }

    this.config = config;
    const options = config.options as SqliteOptions || {};

    // Use explicit filename from options, or fall back to default database path
    // This allows multi-database support (training DBs, etc.) while maintaining backward compatibility
    const dbPath = options.filename || DATABASE_PATHS.SQLITE;
    console.log(`📍 SQLite: Using database path: ${dbPath}`);

    // Ensure directory exists with proper permissions
    const dbDir = path.dirname(dbPath);
    // CRITICAL: Save and set umask to ensure permissions stick
    const oldUmask = process.umask(0o000);
    console.log(`🔒 SQLite: Saved umask ${oldUmask.toString(8)}, set to 0o000 for permission control`);

    try {
      console.log(`📁 SQLite: Ensuring directory exists: ${dbDir}`);
      await fs.mkdir(dbDir, { recursive: true, mode: 0o755 });

      // Set directory permissions for SQLite write operations
      console.log('🔒 SQLite: Setting directory permissions to 0o755...');
      await fs.chmod(dbDir, 0o755);
      console.log('✅ SQLite: Directory permissions set successfully');

      // Clear extended attributes on directory (macOS)
      if (process.platform === 'darwin') {
        try {
          console.log('🍎 SQLite: Clearing directory extended attributes...');
          await execAsync(`xattr -c "${dbDir}"`);
          console.log('✅ SQLite: Directory extended attributes cleared');
        } catch (error) {
          console.warn('⚠️ SQLite: Could not clear directory xattr (non-fatal):', error);
        }
      }

      // Check if database file exists before connection
      let dbFileExists = false;
      try {
        const stats = await fs.stat(dbPath);
        console.log(`📊 SQLite: Existing database found - Size: ${stats.size} bytes, Mode: ${stats.mode.toString(8)}`);
        dbFileExists = true;
      } catch (error) {
        console.log('📄 SQLite: No existing database file, will create new');
      }

      // CRITICAL FIX: Create empty file BEFORE opening connection
      // This allows us to set permissions/clear xattr before SQLite touches it
      if (!dbFileExists) {
        console.log('📝 SQLite: Creating empty database file...');
        await fs.writeFile(dbPath, '', { mode: 0o666 });
        console.log('✅ SQLite: Empty file created with mode 0o666');
      }

      console.log('🔒 SQLite: Setting file permissions to 0o666...');
      await fs.chmod(dbPath, 0o666);
      console.log('✅ SQLite: File permissions set successfully');

      // Clear extended attributes on macOS BEFORE opening connection (prevents SQLITE_READONLY errors)
      if (process.platform === 'darwin') {
        try {
          console.log('🍎 SQLite: Clearing macOS extended attributes...');
          await execAsync(`xattr -c "${dbPath}"`);
          console.log('✅ SQLite: Extended attributes cleared');
        } catch (error) {
          // This is non-fatal, just log it
          console.warn('⚠️ SQLite: Could not clear extended attributes (non-fatal):', error);
        }
      }
    } finally {
      // Restore original umask
      process.umask(oldUmask);
      console.log(`🔒 SQLite: Restored umask to ${oldUmask.toString(8)}`);
    }

    console.log(`🔗 SQLite: Opening database connection with READWRITE | CREATE mode`);

    // Create database connection with explicit write mode
    await new Promise<void>((resolve, reject) => {
      const mode = sqlite3.OPEN_READWRITE | sqlite3.OPEN_CREATE;
      console.log(`🔧 SQLite: Connection mode flags: ${mode}`);

      this.db = new sqlite3.Database(dbPath, mode, (err) => {
        if (err) {
          console.error('❌ SQLite: Failed to open database:', err);
          console.error('❌ SQLite: Error details:', err.message, (err as any).code || 'NO_CODE');
          reject(err);
        } else {
          console.log('✅ SQLite: Database connection established successfully');
          resolve();
        }
      });
    });

    // Ensure database is initialized before proceeding
    if (!this.db) {
      throw new Error('Database initialization failed - db is null');
    }

    // Initialize extracted utility classes (Phase 1 refactoring)
    console.log('🔧 SQLite: Initializing utility classes...');
    this.executor = new SqliteRawExecutor(this.db);
    this.transactionManager = new SqliteTransactionManager(this.executor);
    console.log('✅ SQLite: Utility classes initialized');

    // Initialize schema manager (Phase 0 refactoring)
    console.log('🔧 SQLite: Initializing schema manager...');
    this.schemaManager = new SqliteSchemaManager(
      this.db,
      this.executor,
      this.generateCreateTableSql.bind(this),
      this.generateCreateIndexSql.bind(this),
      this.mapFieldTypeToSql.bind(this)
    );
    console.log('✅ SQLite: Schema manager initialized');

    // Initialize query executor (Phase 0 refactoring)
    console.log('🔧 SQLite: Initializing query executor...');
    this.queryExecutor = new SqliteQueryExecutor(this.executor);
    console.log('✅ SQLite: Query executor initialized');

    // Initialize write manager (Phase 0 refactoring)
    console.log('🔧 SQLite: Initializing write manager...');
    this.writeManager = new SqliteWriteManager(this.executor);
    console.log('✅ SQLite: Write manager initialized');

    console.log('⚙️ SQLite: Configuring database settings...');
    // Configure SQLite settings
    await this.schemaManager.configureSqlite(options);

    // EXFAT FIX: Re-apply permissions after SQLite opens and potentially modifies the file
    // This handles cases where filesystem doesn't properly support Unix permissions
    try {
      console.log('🔧 SQLite: Re-applying file permissions after connection (exFAT workaround)...');
      await fs.chmod(dbPath, 0o666);
      console.log('✅ SQLite: Post-connection permissions applied');
    } catch (error) {
      console.warn('⚠️ SQLite: Could not re-apply permissions (non-fatal):', error);
    }

    console.log('📋 SQLite: Initializing entity registry...');
    // Import and register all known entities (server-side only)
    const { initializeEntityRegistry } = await import('./EntityRegistry');
    initializeEntityRegistry();

    console.log('✅ SQLite: Entity registry initialized (tables will be created lazily on first use)');

    // Verify integrity after initialization
    console.log('🔍 SQLite: Verifying database integrity...');
    await this.schemaManager.verifyIntegrity();

    // Initialize vector search manager (Phase 0 refactoring)
    console.log('🔍 SQLite: Initializing vector search manager...');
    this.vectorSearchManager = new SqliteVectorSearchManager(
      this.executor,
      this  // DataStorageAdapter for CRUD operations
    );
    console.log('✅ SQLite: Vector search manager initialized');

    this.isInitialized = true;
    console.log('🎯 SQLite: Initialization complete and verified');
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
   * Supports nested calls by only creating transaction if not already in one
   * Delegated to SqliteTransactionManager (Phase 1 refactoring)
   */
  private async withTransaction<T>(operation: () => Promise<T>): Promise<T> {
    return this.transactionManager.withTransaction(operation);
  }

  /**
   * Create a record with proper relational schema (always use entity-specific tables)
   * Delegates to SqliteWriteManager
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    return this.writeManager.create<T>(record.collection, record.data, record.id);
  }

  /**
   * Read a single record by ID - uses entity-specific tables
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    return this.queryExecutor.read<T>(collection, id);
  }


  /**
   * Query records with complex filters - uses entity-specific tables
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    return this.queryExecutor.query<T>(query);
  }


  // Removed relational query methods - cross-cutting concerns

  /**
   * Update an existing record - delegates to SqliteWriteManager
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
      console.error(`❌ SQLite: Update failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }


  /**
   * Delete a record - delegates to SqliteWriteManager
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    return this.writeManager.delete(collection, id);
  }

  /**
   * Batch create records - delegates to SqliteWriteManager
   */
  async batchCreate<T extends RecordData>(
    collection: string,
    records: T[]
  ): Promise<StorageResult<DataRecord<T>[]>> {
    return this.writeManager.batchCreate<T>(collection, records);
  }

  /**
   * Batch update records - delegates to SqliteWriteManager
   */
  async batchUpdate<T extends RecordData>(
    collection: string,
    updates: Array<{ id: UUID; data: Partial<T>; version?: number }>
  ): Promise<StorageResult<DataRecord<T>[]>> {
    return this.writeManager.batchUpdate<T>(collection, updates);
  }

  /**
   * Batch delete records - delegates to SqliteWriteManager
   */
  async batchDelete(
    collection: string,
    ids: UUID[]
  ): Promise<StorageResult<boolean>> {
    return this.writeManager.batchDelete(collection, ids);
  }

  /**
   * List collections (entity tables from sqlite_master, not old _collections table)
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      // List all non-system tables (entity tables)
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
      console.error('❌ SQLite: List collections failed:', error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Get collection statistics (from entity table directly, not old _collections table)
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
      console.error(`❌ SQLite: Get stats failed for ${collection}:`, error.message);
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
    if (!this.db) {
      return {
        success: false,
        error: 'Database not initialized'
      };
    }

    return new Promise((resolve) => {
      this.db!.serialize(() => {
        // Only begin transaction if not already in one
        if (!this.transactionManager.isInTransaction()) {
          // FIXME(Phase2): This manual transaction management should be replaced with withTransaction()
          this.db!.run('BEGIN TRANSACTION');
        }

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

          // Commit or rollback
          // FIXME(Phase2): Manual transaction management - should use withTransaction()
          if (hasError) {
            this.db!.run('ROLLBACK', (err) => {
              resolve({
                success: false,
                error: errorMessage,
                data: results
              });
            });
          } else {
            this.db!.run('COMMIT', (err) => {
              if (err) {
                resolve({
                  success: false,
                  error: err.message,
                  data: results
                });
              } else {
                resolve({
                  success: true,
                  data: results
                });
              }
            });
          }
        };

        processOperations();
      });
    });
  }

  /**
   * Clear all data from all collections (entity tables)
   */
  async clear(): Promise<StorageResult<boolean>> {
    if (!this.isInitialized || !this.db) {
      return {
        success: false,
        error: 'Database not initialized'
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

      console.log('🧹 SQLite: All entity data cleared successfully');
      return {
        success: true,
        data: result
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('❌ SQLite: Error clearing data:', errorMessage);
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
    if (!this.isInitialized || !this.db) {
      return {
        success: false,
        error: 'Database not initialized'
      };
    }

    try {
      const tableName: string = SqlNamingConverter.toTableName(collection);

      // Validate table name to prevent SQL injection (must be alphanumeric + underscores)
      if (!/^[a-z0-9_]+$/i.test(tableName)) {
        throw new Error(`Invalid table name: ${tableName}`);
      }

      const result: number = await this.withTransaction(async (): Promise<number> => {
        // Use DELETE with table name (cannot parameterize table names in SQL)
        // Table name validated above to prevent injection
        const deleteResult = await this.executor.runStatement(`DELETE FROM ${tableName}`, []);

        return deleteResult.changes ?? 0;
      });

      console.log(`🧹 SQLite: Truncated collection '${collection}' - ${result} records removed`);
      return {
        success: true,
        data: result > 0
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ SQLite: Error truncating collection '${collection}':`, errorMessage);
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
    if (!this.db) return;

    try {
      // VACUUM to reclaim space
      await this.executor.runStatement('VACUUM');

      // ANALYZE to update statistics
      await this.executor.runStatement('ANALYZE');

      console.log('✅ SQLite: Cleanup completed');

    } catch (error) {
      console.error('❌ SQLite: Cleanup failed:', error);
    }
  }

  /**
   * Close database connection
   */
  async close(): Promise<void> {
    if (!this.db) return;

    return new Promise<void>((resolve, reject) => {
      this.db!.close((err) => {
        if (err) {
          console.error('❌ SQLite: Failed to close database:', err);
          reject(err);
        } else {
          console.log('✅ SQLite: Database connection closed');
          this.db = null;
          this.isInitialized = false;
          resolve();
        }
      });
    });
  }

  /**
   * Clear all entity data from the database (preserving structure)
   *
   * This method:
   * - Deletes all records from entity-specific tables
   * - Resets SQLite sequence counters
   * - Preserves database schema and table structure
   * - Uses transactions for consistency
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    if (!this.isInitialized || !this.db) {
      throw new Error('SqliteStorageAdapter not initialized');
    }

    console.log('🧹 SQLite: Starting complete database clear (preserving structure)...');

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

            console.log(`✅ SQLite: Cleared ${recordCount} records from table '${tableName}'`);
          } else {
            console.log(`📋 SQLite: Table '${tableName}' was already empty`);
          }
        }

        // No collection statistics to reset (entity tables only)

        // Reset SQLite sequence counters for tables that use them
        const sequenceTables = await this.executor.runSql(`
          SELECT name FROM sqlite_sequence
        `);

        for (const seqTable of sequenceTables) {
          await this.executor.runStatement(`UPDATE sqlite_sequence SET seq = 0 WHERE name = ?`, [seqTable.name]);
        }
      });

      console.log(`🎉 SQLite: Database clearing complete!`);
      console.log(`   📊 Tables processed: ${tablesCleared.length}`);
      console.log(`   🗑️ Records deleted: ${totalRecordsDeleted}`);
      console.log(`   🏗️ Database structure preserved`);

      return {
        success: true,
        data: {
          tablesCleared,
          recordsDeleted: totalRecordsDeleted
        }
      };

    } catch (error) {
      console.error('❌ SQLite: Database clear failed:', error);
      throw new Error(`Database clear failed: ${error}`);
    }
  }

  /**
   * Explain query execution (dry-run) - shows what SQL would be generated
   * Uses the same query builder as actual execution for true-to-life results
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    return this.queryExecutor.explainQuery(query);
  }

  // ============================================================================
  // VECTOR SEARCH ADAPTER INTERFACE - Delegate to SqliteVectorSearchManager
  // ============================================================================

  /**
   * Perform vector similarity search
   * Delegates to SqliteVectorSearchManager
   */
  async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    return this.vectorSearchManager.vectorSearch<T>(options);
  }

  /**
   * Generate embedding for text
   * Delegates to SqliteVectorSearchManager
   */
  async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    return this.vectorSearchManager.generateEmbedding(request);
  }

  /**
   * Index vector for a record
   * Delegates to SqliteVectorSearchManager
   */
  async indexVector(request: IndexVectorRequest): Promise<StorageResult<boolean>> {
    return this.vectorSearchManager.indexVector(request);
  }

  /**
   * Backfill embeddings for existing records
   * Delegates to SqliteVectorSearchManager
   */
  async backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    return this.vectorSearchManager.backfillVectors(request, onProgress);
  }

  /**
   * Get vector index statistics
   * Delegates to SqliteVectorSearchManager
   */
  async getVectorIndexStats(collection: string): Promise<StorageResult<VectorIndexStats>> {
    return this.vectorSearchManager.getVectorIndexStats(collection);
  }

  /**
   * Get vector search capabilities
   * Delegates to SqliteVectorSearchManager
   */
  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities> {
    return this.vectorSearchManager.getVectorSearchCapabilities();
  }

}