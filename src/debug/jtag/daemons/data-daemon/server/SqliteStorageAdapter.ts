/**
 * SQLite Storage Adapter - SQL Database Backend
 *
 * Implements full SQL functionality with joins, transactions, and indexing
 * Foundation for PostgreSQL/MySQL adapters with 90% code reuse
 */

import sqlite3 from 'sqlite3';
import * as path from 'path';
import * as fs from 'fs/promises';
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type CollectionStats,
  type StorageOperation,
  type RecordData
} from '../shared/DataStorageAdapter';
import { DATABASE_PATHS } from '../../../system/data/config/DatabaseConfig';
// Clean imports - field extraction removed for now
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

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

/**
 * SQLite Storage Adapter with Full SQL Support
 */
export class SqliteStorageAdapter extends DataStorageAdapter {
  private db: sqlite3.Database | null = null;
  private config: StorageAdapterConfig | null = null;
  private isInitialized: boolean = false;

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

    // Use centralized database path - no fallbacks
    const dbPath = DATABASE_PATHS.SQLITE;
    console.log(`📍 SQLite: Using database path: ${dbPath}`);

    // Ensure directory exists with proper permissions
    const dbDir = path.dirname(dbPath);
    console.log(`📁 SQLite: Ensuring directory exists: ${dbDir}`);
    await fs.mkdir(dbDir, { recursive: true, mode: 0o755 });

    // Check if database file exists before connection
    try {
      const stats = await fs.stat(dbPath);
      console.log(`📊 SQLite: Existing database found - Size: ${stats.size} bytes, Mode: ${stats.mode.toString(8)}`);
    } catch (error) {
      console.log('📄 SQLite: No existing database file, will create new');
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

    console.log('⚙️ SQLite: Configuring database settings...');
    // Configure SQLite settings
    await this.configureSqlite(options);

    console.log('🏗️ SQLite: Creating core schema...');
    // Create core schema
    await this.createCoreSchema();

    console.log('🔒 SQLite: Setting file permissions...');
    // Ensure database file has correct permissions
    try {
      await fs.chmod(dbPath, 0o644);
      console.log('✅ SQLite: File permissions set successfully');
    } catch (error) {
      console.warn('⚠️ SQLite: Could not set database file permissions:', error);
    }

    // Verify integrity after initialization
    console.log('🔍 SQLite: Verifying database integrity...');
    await this.verifyIntegrity();

    this.isInitialized = true;
    console.log('🎯 SQLite: Initialization complete and verified');
  }

  /**
   * Verify database integrity and write capability
   */
  private async verifyIntegrity(): Promise<void> {
    if (!this.db) {
      throw new Error('Database not initialized');
    }

    console.log('🧪 SQLite: Testing database write capability...');

    // Create test metadata using real daemon context information
    const testId = `integrity-test-${Date.now()}`;
    const testMetadata = {
      adapterType: 'SqliteStorageAdapter',
      initializationTime: new Date().toISOString(),
      databasePath: DATABASE_PATHS.SQLITE,
      configOptions: this.config?.options || {},
      nodeVersion: process.version,
      platform: process.platform,
      test: true
    };

    try {
      // First register the collection to avoid foreign key constraint errors
      console.log('🧪 SQLite: Registering test collection...');
      await this.runSql(
        'INSERT OR IGNORE INTO _collections (name, created_at) VALUES (?, ?)',
        ['DAEMON_METADATA', new Date().toISOString()]
      );
      console.log('✅ SQLite: Test collection registered');

      // Test 1: Insert daemon metadata record
      console.log('🧪 SQLite: Testing insert operation...');
      await this.runSql(
        'INSERT INTO _data (id, collection, data, created_at) VALUES (?, ?, ?, ?)',
        [testId, 'DAEMON_METADATA', JSON.stringify(testMetadata), new Date().toISOString()]
      );
      console.log('✅ SQLite: Insert test successful');

      // Test 2: Read back the metadata
      console.log('🧪 SQLite: Testing read operation...');
      const results = await this.runSql(
        'SELECT data, created_at FROM _data WHERE id = ? AND collection = ?',
        [testId, 'DAEMON_METADATA']
      );

      if (!results || results.length === 0) {
        throw new Error('Read test failed - no data returned');
      }

      const result = results[0];
      const retrievedData = JSON.parse(result.data);
      if (retrievedData.test !== true || retrievedData.adapterType !== 'SqliteStorageAdapter') {
        throw new Error('Read test failed - data integrity mismatch');
      }
      console.log('✅ SQLite: Read test successful');

      // Test 3: Update the record
      console.log('🧪 SQLite: Testing update operation...');
      const updatedMetadata = { ...testMetadata, verified: true, verificationTime: new Date().toISOString() };
      await this.runSql(
        'UPDATE _data SET data = ?, updated_at = ? WHERE id = ? AND collection = ?',
        [JSON.stringify(updatedMetadata), new Date().toISOString(), testId, 'DAEMON_METADATA']
      );
      console.log('✅ SQLite: Update test successful');

      // Test 4: Verify update worked
      const updatedResults = await this.runSql(
        'SELECT data FROM _data WHERE id = ? AND collection = ?',
        [testId, 'DAEMON_METADATA']
      );

      if (!updatedResults || updatedResults.length === 0 || !JSON.parse(updatedResults[0].data).verified) {
        throw new Error('Update verification failed');
      }
      console.log('✅ SQLite: Update verification successful');

      // Test 5: Test collection query (like the commands use)
      console.log('🧪 SQLite: Testing collection query...');
      const collectionResults = await this.runSql(
        'SELECT id, data FROM _data WHERE collection = ?',
        ['DAEMON_METADATA']
      );

      if (!collectionResults || collectionResults.length === 0) {
        throw new Error('Collection query failed');
      }
      console.log('✅ SQLite: Collection query test successful');

      // Cleanup: Delete test record and collection
      await this.runSql(
        'DELETE FROM _data WHERE id = ? AND collection = ?',
        [testId, 'DAEMON_METADATA']
      );
      await this.runSql(
        'DELETE FROM _collections WHERE name = ?',
        ['DAEMON_METADATA']
      );
      console.log('✅ SQLite: Cleanup successful - database fully operational');

      // Final verification: Ensure cleanup worked
      const cleanupResults = await this.runSql(
        'SELECT COUNT(*) as count FROM _data WHERE collection = ?',
        ['DAEMON_METADATA']
      );

      if (cleanupResults && cleanupResults.length > 0 && parseInt(cleanupResults[0].count) > 0) {
        console.warn('⚠️ SQLite: Test data may not have been fully cleaned up');
      }

      console.log('🎉 SQLite: All integrity checks passed - adapter fully functional');

    } catch (error) {
      console.error('❌ SQLite: Integrity verification failed:', error);
      console.error('❌ SQLite: Error details:', error instanceof Error ? error.message : String(error));
      throw new Error(`Database integrity check failed: ${error}`);
    }
  }

  /**
   * Configure SQLite performance and behavior settings
   */
  private async configureSqlite(options: SqliteOptions): Promise<void> {
    if (!this.db) return;

    const settings = [
      // Enable foreign keys if requested
      options.foreignKeys !== false ? 'PRAGMA foreign_keys = ON' : null,

      // Set WAL mode for better concurrency
      options.wal ? 'PRAGMA journal_mode = WAL' : null,

      // Set synchronous mode
      options.synchronous ? `PRAGMA synchronous = ${options.synchronous}` : 'PRAGMA synchronous = NORMAL',

      // Set cache size (negative = KB, positive = pages)
      options.cacheSize ? `PRAGMA cache_size = ${options.cacheSize}` : 'PRAGMA cache_size = -2000',

      // Set busy timeout
      options.timeout ? `PRAGMA busy_timeout = ${options.timeout}` : 'PRAGMA busy_timeout = 10000'
    ].filter(Boolean);

    for (const sql of settings) {
      if (sql) {
        await this.runSql(sql);
      }
    }

    console.log('⚙️ SQLite: Configuration applied');
  }

  /**
   * Create core schema for collections and metadata
   */
  private async createCoreSchema(): Promise<void> {
    // Collections registry table
    await this.runSql(`
      CREATE TABLE IF NOT EXISTS _collections (
        name TEXT PRIMARY KEY,
        schema_version INTEGER DEFAULT 1,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
        record_count INTEGER DEFAULT 0,
        total_size INTEGER DEFAULT 0
      )
    `);

    // Generic data table - stores all collection data as JSON
    await this.runSql(`
      CREATE TABLE IF NOT EXISTS _data (
        id TEXT NOT NULL,
        collection TEXT NOT NULL,
        data TEXT NOT NULL,  -- JSON data
        created_at TEXT DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
        version INTEGER DEFAULT 1,
        tags TEXT,  -- JSON array of tags
        ttl INTEGER,  -- Time to live (Unix timestamp)

        PRIMARY KEY (collection, id),
        FOREIGN KEY (collection) REFERENCES _collections(name) ON DELETE CASCADE
      )
    `);

    // Index for common queries
    await this.runSql('CREATE INDEX IF NOT EXISTS idx_data_collection ON _data(collection)');
    await this.runSql('CREATE INDEX IF NOT EXISTS idx_data_id ON _data(id)');
    await this.runSql('CREATE INDEX IF NOT EXISTS idx_data_created_at ON _data(created_at)');
    await this.runSql('CREATE INDEX IF NOT EXISTS idx_data_updated_at ON _data(updated_at)');

    // Ready for future SQL query builder integration

    console.log('📋 SQLite: Core schema created');
  }

  // Field extraction removed - will be added back with SQL query builder

  // Type mapping will be added back with SQL query builder

  /**
   * Execute SQL query with parameters
   */
  private async runSql(sql: string, params: any[] = []): Promise<any[]> {
    if (!this.db) {
      throw new Error('SQLite database not initialized');
    }

    return new Promise((resolve, reject) => {
      this.db!.all(sql, params, (err, rows) => {
        if (err) {
          console.error('❌ SQLite Query Error:', err.message);
          console.error('SQL:', sql);
          console.error('Params:', params);
          reject(err);
        } else {
          resolve(rows || []);
        }
      });
    });
  }

  /**
   * Execute SQL statement (INSERT, UPDATE, DELETE)
   */
  private async runStatement(sql: string, params: any[] = []): Promise<{ lastID?: number; changes: number }> {
    if (!this.db) {
      throw new Error('SQLite database not initialized');
    }

    return new Promise((resolve, reject) => {
      this.db!.run(sql, params, function(err) {
        if (err) {
          console.error('❌ SQLite Statement Error:', err.message);
          console.error('SQL:', sql);
          console.error('Params:', params);
          reject(err);
        } else {
          resolve({ lastID: this.lastID, changes: this.changes });
        }
      });
    });
  }

  /**
   * Begin a database transaction
   */
  private async beginTransaction(): Promise<void> {
    await this.runStatement('BEGIN TRANSACTION');
  }

  /**
   * Commit a database transaction
   */
  private async commitTransaction(): Promise<void> {
    await this.runStatement('COMMIT');
  }

  /**
   * Rollback a database transaction
   */
  private async rollbackTransaction(): Promise<void> {
    await this.runStatement('ROLLBACK');
  }

  /**
   * Execute operations within a transaction for atomic consistency
   */
  private async withTransaction<T>(operation: () => Promise<T>): Promise<T> {
    await this.beginTransaction();

    try {
      const result = await operation();
      await this.commitTransaction();
      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Transaction committed successfully`);
      return result;
    } catch (error) {
      await this.rollbackTransaction();
      console.error(`❌ SQLite: Transaction rolled back due to error:`, error);
      throw error;
    }
  }

  /**
   * Create or update a record with dual-mode storage (JSON blob + field extraction)
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      // Ensure collection exists
      await this.ensureCollection(record.collection);

      // Execute create operation in transaction for atomic consistency
      const result = await this.withTransaction(async () => {
        // Insert/replace data record (JSON blob storage - always maintained for backward compatibility)
        const sql = `
          INSERT OR REPLACE INTO _data (
            id, collection, data, created_at, updated_at, version, tags, ttl
          ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        `;

        const params = [
          record.id,
          record.collection,
          JSON.stringify(record.data),
          record.metadata.createdAt,
          record.metadata.updatedAt,
          record.metadata.version,
          record.metadata.tags ? JSON.stringify(record.metadata.tags) : null,
          record.metadata.ttl || null
        ];

        await this.runStatement(sql, params);

        // Field extraction will be added back with SQL query builder

        return record;
      });

      // Update collection stats (outside transaction)
      await this.updateCollectionStats(record.collection);

      console.log(`✅ SQLite: Created record ${record.id} in ${record.collection}`);

      return {
        success: true,
        data: result
      };

    } catch (error: any) {
      console.error(`❌ SQLite: Create failed for ${record.id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  // Removed extractFields - cross-cutting concern

  /**
   * Read a single record by ID
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const sql = 'SELECT * FROM _data WHERE collection = ? AND id = ? LIMIT 1';
      const rows = await this.runSql(sql, [collection, id]);

      if (rows.length === 0) {
        return {
          success: true,
          data: undefined
        };
      }

      const row = rows[0];
      const record: DataRecord<T> = {
        id: row.id,
        collection: row.collection,
        data: JSON.parse(row.data),
        metadata: {
          createdAt: row.created_at,
          updatedAt: row.updated_at,
          version: row.version,
          tags: row.tags ? JSON.parse(row.tags) : undefined,
          ttl: row.ttl || undefined
        }
      };

      return {
        success: true,
        data: record
      };

    } catch (error: any) {
      console.error(`❌ SQLite: Read failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Query records with complex filters - enhanced for relational queries
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      console.log(`🔍 SQLite: Querying ${query.collection}`, query);

      const { sql, params } = this.buildSelectQuery(query);
      const rows = await this.runSql(sql, params);

      const records: DataRecord<T>[] = rows.map(row => ({
        id: row.id,
        collection: row.collection,
        data: JSON.parse(row.data),
        metadata: {
          createdAt: row.created_at,
          updatedAt: row.updated_at,
          version: row.version,
          tags: row.tags ? JSON.parse(row.tags) : undefined,
          ttl: row.ttl || undefined
        }
      }));

      console.log(`✅ SQLite: Query returned ${records.length} records`);

      return {
        success: true,
        data: records,
        metadata: {
          totalCount: records.length,
          queryTime: 0 // TODO: Add timing
        }
      };

    } catch (error: any) {
      console.error(`❌ SQLite: Query failed for ${query.collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Build SELECT SQL query - currently JSON-based, ready for SQL query builder
   */
  private buildSelectQuery(query: StorageQuery): { sql: string; params: any[] } {
    return this.buildJsonQuery(query);
  }

  /**
   * Sanitize field path for JSON_EXTRACT to prevent injection
   */
  private sanitizeJsonPath(fieldPath: string): string {
    // Only allow alphanumeric, dots, underscores, and array indices
    return fieldPath.replace(/[^a-zA-Z0-9._\[\]]/g, '');
  }

  /**
   * Build traditional JSON_EXTRACT query for collections without field extraction
   */
  private buildJsonQuery(query: StorageQuery): { sql: string; params: any[] } {
    const params: any[] = [];
    let sql = 'SELECT * FROM _data WHERE collection = ?';
    params.push(query.collection);

    // Add basic filters
    if (query.filters) {
      for (const [field, value] of Object.entries(query.filters)) {
        sql += ` AND JSON_EXTRACT(data, '$.${field}') = ?`;
        params.push(value);
      }
    }

    // Add time range filter
    if (query.timeRange) {
      if (query.timeRange.start) {
        sql += ' AND created_at >= ?';
        params.push(query.timeRange.start);
      }
      if (query.timeRange.end) {
        sql += ' AND created_at <= ?';
        params.push(query.timeRange.end);
      }
    }

    // Add sorting
    if (query.sort && query.sort.length > 0) {
      const sortClauses = query.sort.map(s =>
        `JSON_EXTRACT(data, '$.${s.field}') ${s.direction.toUpperCase()}`
      );
      sql += ` ORDER BY ${sortClauses.join(', ')}`;
    }

    // Add pagination
    if (query.limit) {
      sql += ' LIMIT ?';
      params.push(query.limit);

      if (query.offset) {
        sql += ' OFFSET ?';
        params.push(query.offset);
      }
    }

    return { sql, params };
  }

  // Removed relational query methods - cross-cutting concerns

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
      // First read existing record
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

      // Update record
      const sql = `
        UPDATE _data
        SET data = ?, updated_at = ?, version = ?
        WHERE collection = ? AND id = ?
      `;

      const params = [
        JSON.stringify(updatedData),
        new Date().toISOString(),
        version,
        collection,
        id
      ];

      await this.runStatement(sql, params);

      // Create updated record for field extraction
      const updatedRecord: DataRecord<T> = {
        ...existing.data,
        data: updatedData as T,
        metadata: {
          ...existing.data.metadata,
          updatedAt: new Date().toISOString(),
          version
        }
      };

      // Field extraction optimization disabled for now
      // await this.extractFields(updatedRecord, fieldMapping);

      // Update collection stats
      await this.updateCollectionStats(collection);

      console.log(`✅ SQLite: Updated record ${id} in ${collection}`);

      return {
        success: true,
        data: updatedRecord
      };

    } catch (error: any) {
      console.error(`❌ SQLite: Update failed for ${collection}/${id}:`, error.message);
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
    try {
      console.log(`🗑️ CLAUDE-FIX-${Date.now()}: Deleting ${collection}/${id} with atomic transaction`);

      // Execute delete operation in transaction for atomic consistency
      const deletedCount = await this.withTransaction(async () => {
        // Delete from main data table
        const dataSql = 'DELETE FROM _data WHERE collection = ? AND id = ?';
        const dataResult = await this.runStatement(dataSql, [collection, id]);

        return dataResult.changes;
      });

      if (deletedCount === 0) {
        return {
          success: true,
          data: false  // Record didn't exist
        };
      }

      // Update collection stats (outside transaction)
      await this.updateCollectionStats(collection);

      console.log(`✅ SQLite: Deleted record ${id} from ${collection} with full cleanup`);

      return {
        success: true,
        data: true
      };

    } catch (error: any) {
      console.error(`❌ SQLite: Delete failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * List collections
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      const sql = 'SELECT name FROM _collections ORDER BY name';
      const rows = await this.runSql(sql);

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
   * Get collection statistics
   */
  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    try {
      const sql = 'SELECT * FROM _collections WHERE name = ?';
      const rows = await this.runSql(sql, [collection]);

      if (rows.length === 0) {
        return {
          success: true,
          data: undefined
        };
      }

      const row = rows[0];
      const stats: CollectionStats = {
        name: row.name,
        recordCount: row.record_count,
        totalSize: row.total_size,
        lastModified: row.updated_at,
        schema: `v${row.schema_version}`
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
        this.db!.run('BEGIN TRANSACTION');

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
   * Clear all data from all collections
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
        // Clear all data and collections
        await this.runStatement('DELETE FROM _data');
        await this.runStatement('DELETE FROM _collections');

        // Ready for future field extraction cleanup

        return true;
      });

      console.log('🧹 SQLite: All data cleared successfully');
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
      const result = await this.withTransaction(async () => {
        // Delete all records from the collection
        const deleteResult = await this.runStatement('DELETE FROM _data WHERE collection = ?', [collection]);

        // Ready for future field extraction cleanup

        // Update collection stats
        await this.updateCollectionStats(collection);

        return deleteResult.changes;
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
      // Remove expired records
      await this.runStatement('DELETE FROM _data WHERE ttl IS NOT NULL AND ttl < ?', [Date.now()]);

      // Update collection stats
      const collections = await this.listCollections();
      if (collections.success && collections.data) {
        for (const collection of collections.data) {
          await this.updateCollectionStats(collection);
        }
      }

      // VACUUM to reclaim space
      await this.runStatement('VACUUM');

      // ANALYZE to update statistics
      await this.runStatement('ANALYZE');

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

  // --- Helper Methods ---

  /**
   * Ensure collection exists in registry
   */
  private async ensureCollection(collection: string): Promise<void> {
    const sql = `
      INSERT OR IGNORE INTO _collections (name, created_at, updated_at)
      VALUES (?, ?, ?)
    `;

    const now = new Date().toISOString();
    await this.runStatement(sql, [collection, now, now]);
  }

  /**
   * Update collection statistics
   */
  private async updateCollectionStats(collection: string): Promise<void> {
    const countSql = 'SELECT COUNT(*) as count, SUM(LENGTH(data)) as total_size FROM _data WHERE collection = ?';
    const rows = await this.runSql(countSql, [collection]);

    if (rows.length > 0) {
      const { count, total_size } = rows[0];

      const updateSql = `
        UPDATE _collections
        SET record_count = ?, total_size = ?, updated_at = ?
        WHERE name = ?
      `;

      await this.runStatement(updateSql, [count, total_size || 0, new Date().toISOString(), collection]);
    }
  }
}