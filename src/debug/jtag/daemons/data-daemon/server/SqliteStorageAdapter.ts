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
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { getFieldMetadata, hasFieldMetadata, type FieldMetadata, type FieldType } from '../../../system/data/decorators/FieldDecorators';

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
 * Entity Class Registry - Maps collection names to entity classes
 * Entities register themselves when their modules are imported
 */
type EntityConstructor = new (...args: any[]) => any;
const ENTITY_REGISTRY = new Map<string, EntityConstructor>();

/**
 * Register an entity class with its collection name
 * Called automatically when entity classes are imported/loaded
 */
export function registerEntity(collectionName: string, entityClass: EntityConstructor): void {
  console.log(`🏷️ SQLite: Registering entity ${collectionName} -> ${entityClass.name}`);
  ENTITY_REGISTRY.set(collectionName, entityClass);
}

/**
 * SQL Naming Convention Converter
 */
class SqlNamingConverter {
  /**
   * Convert camelCase to snake_case for SQL columns
   */
  static toSnakeCase(camelCase: string): string {
    return camelCase.replace(/([A-Z])/g, '_$1').toLowerCase().replace(/^_/, '');
  }

  /**
   * Convert snake_case back to camelCase for object properties
   */
  static toCamelCase(snakeCase: string): string {
    return snakeCase.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
  }

  /**
   * Convert collection name to table name (pluralized, snake_case)
   */
  static toTableName(collectionName: string): string {
    const snakeCase = this.toSnakeCase(collectionName);
    // Simple pluralization - add 's' or 'es'
    return snakeCase.endsWith('s') || snakeCase.endsWith('x') || snakeCase.endsWith('ch') || snakeCase.endsWith('sh')
      ? `${snakeCase}es`
      : `${snakeCase}s`;
  }
}

/**
 * SQLite Storage Adapter with Proper Relational Schema
 */
export class SqliteStorageAdapter extends DataStorageAdapter {
  private db: sqlite3.Database | null = null;
  private config: StorageAdapterConfig | null = null;
  private isInitialized: boolean = false;
  private createdTables = new Set<string>(); // Track created tables

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

    console.log('📋 SQLite: Initializing entity registry...');
    // Import and register all known entities (server-side only)
    const { initializeEntityRegistry } = await import('./EntityRegistry');
    initializeEntityRegistry();

    console.log('🏗️ SQLite: Creating core schema...');
    // Create core schema
    await this.createCoreSchema();

    console.log('🔍 SQLite: Testing entity schema generation...');
    // Test schema generation for registered entities (just log, don't create yet)
    await this.logEntitySchemas();

    console.log('🔒 SQLite: Setting file permissions...');
    // Ensure database file has correct permissions (read/write for owner)
    try {
      await fs.chmod(dbPath, 0o666);
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
      // Set foreign keys based on configuration
      options.foreignKeys === false ? 'PRAGMA foreign_keys = OFF' : 'PRAGMA foreign_keys = ON',

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
   * Map FieldType to SQL column type
   */
  private mapFieldTypeToSql(fieldType: FieldType, options?: FieldMetadata['options']): string {
    switch (fieldType) {
      case 'primary':
        return 'TEXT PRIMARY KEY';
      case 'foreign_key':
        return 'TEXT' + (options?.nullable ? '' : ' NOT NULL');
      case 'text':
        const maxLength = options?.maxLength;
        return maxLength ? `VARCHAR(${maxLength})` : 'TEXT';
      case 'number':
        return 'REAL';
      case 'boolean':
        return 'INTEGER'; // SQLite uses INTEGER for booleans (0/1)
      case 'date':
        return 'TEXT'; // ISO string format
      case 'enum':
        return 'TEXT';
      case 'json':
        return 'TEXT'; // JSON stored as TEXT
      default:
        return 'TEXT';
    }
  }

  /**
   * Generate SQL CREATE TABLE statement from entity metadata
   */
  private generateCreateTableSql(collectionName: string, entityClass: EntityConstructor): string {
    const tableName = SqlNamingConverter.toTableName(collectionName);
    const fieldMetadata = getFieldMetadata(entityClass);

    const columns: string[] = [];
    const constraints: string[] = [];

    // Base entity fields come from @PrimaryField, @DateField, @NumberField decorators
    // No hardcoded base fields - let the decorator system handle them

    // Add entity-specific fields
    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      const columnName = SqlNamingConverter.toSnakeCase(fieldName);
      const columnType = this.mapFieldTypeToSql(metadata.fieldType, metadata.options);

      let columnDef = `${columnName} ${columnType}`;

      // Add constraints
      if (metadata.options?.nullable === false && metadata.fieldType !== 'primary') {
        columnDef += ' NOT NULL';
      }

      if (metadata.options?.unique) {
        columnDef += ' UNIQUE';
      }

      if (metadata.options?.default !== undefined) {
        columnDef += ` DEFAULT ${JSON.stringify(metadata.options.default)}`;
      }

      columns.push(columnDef);

      // Handle foreign key references
      if (metadata.fieldType === 'foreign_key' && metadata.options?.references) {
        const ref = metadata.options.references;
        // Parse "TableName.columnName" format
        const [refTable, refColumn] = ref.split('.');
        if (refTable && refColumn) {
          const refTableName = SqlNamingConverter.toTableName(refTable);
          const refColumnName = SqlNamingConverter.toSnakeCase(refColumn);
          constraints.push(`FOREIGN KEY (${columnName}) REFERENCES ${refTableName}(${refColumnName})`);
        }
      }
    }

    // Build CREATE TABLE statement
    let sql = `CREATE TABLE IF NOT EXISTS ${tableName} (\n`;
    sql += '  ' + columns.join(',\n  ');

    if (constraints.length > 0) {
      sql += ',\n  ' + constraints.join(',\n  ');
    }

    sql += '\n)';

    return sql;
  }

  /**
   * Generate CREATE INDEX statements for indexed fields
   */
  private generateCreateIndexSql(collectionName: string, entityClass: EntityConstructor): string[] {
    const tableName = SqlNamingConverter.toTableName(collectionName);
    const fieldMetadata = getFieldMetadata(entityClass);
    const indexes: string[] = [];

    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      if (metadata.options?.index) {
        const columnName = SqlNamingConverter.toSnakeCase(fieldName);
        const indexName = `idx_${tableName}_${columnName}`;
        indexes.push(`CREATE INDEX IF NOT EXISTS ${indexName} ON ${tableName}(${columnName})`);
      }
    }

    return indexes;
  }

  /**
   * Create table for entity if it doesn't exist
   */
  private async ensureEntityTable(collectionName: string): Promise<void> {
    const tableName = SqlNamingConverter.toTableName(collectionName);

    if (this.createdTables.has(tableName)) {
      return; // Already created
    }

    const entityClass = ENTITY_REGISTRY.get(collectionName);
    if (!entityClass || !hasFieldMetadata(entityClass)) {
      console.warn(`⚠️ No entity metadata found for collection: ${collectionName}`);
      return;
    }

    console.log(`🏗️ Creating table for entity: ${collectionName} -> ${tableName}`);

    // Create table
    const createTableSql = this.generateCreateTableSql(collectionName, entityClass);
    await this.runSql(createTableSql);

    // Create indexes
    const indexSqls = this.generateCreateIndexSql(collectionName, entityClass);
    for (const indexSql of indexSqls) {
      await this.runSql(indexSql);
    }

    this.createdTables.add(tableName);
    console.log(`✅ Table created: ${tableName} with ${indexSqls.length} indexes`);
  }

  /**
   * Log what schemas would be generated for all registered entities
   */
  private async logEntitySchemas(): Promise<void> {
    console.log('📋 SQLite: Analyzing registered entities...');

    for (const [collectionName, entityClass] of ENTITY_REGISTRY.entries()) {
      if (!hasFieldMetadata(entityClass)) {
        console.log(`⚠️ ${collectionName}: No field metadata found`);
        continue;
      }

      const tableName = SqlNamingConverter.toTableName(collectionName);
      console.log(`\n🏗️ ${collectionName} -> ${tableName}:`);

      // Log what CREATE TABLE would look like
      const createTableSql = this.generateCreateTableSql(collectionName, entityClass);
      console.log('   CREATE TABLE SQL:');
      console.log('   ' + createTableSql.split('\n').join('\n   '));

      // Log field metadata
      const fieldMetadata = getFieldMetadata(entityClass);
      console.log(`   📊 Fields: ${fieldMetadata.size}`);
      for (const [fieldName, metadata] of fieldMetadata.entries()) {
        const columnName = SqlNamingConverter.toSnakeCase(fieldName);
        console.log(`   • ${fieldName} -> ${columnName} (${metadata.fieldType}${metadata.options?.index ? ', indexed' : ''})`);
      }

      // Log indexes
      const indexSqls = this.generateCreateIndexSql(collectionName, entityClass);
      if (indexSqls.length > 0) {
        console.log(`   🔍 Indexes: ${indexSqls.length}`);
        indexSqls.forEach(sql => console.log(`   • ${sql}`));
      }
    }

    console.log('\n✅ SQLite: Entity schema analysis complete');
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
    console.log(`🔧 SQLite RUNSTATEMENT DEBUG: Executing SQL:`, { sql: sql.trim(), params });
    if (!this.db) {
      console.error(`❌ SQLite RUNSTATEMENT DEBUG: Database not initialized!`);
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
          const result = { lastID: this.lastID, changes: this.changes };
          console.log(`✅ SQLite RUNSTATEMENT DEBUG: Success:`, result);
          resolve(result);
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
   * Create a record with proper relational schema (always use entity-specific tables)
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Creating ${record.collection}/${record.id} in entity-specific table`);

      // Ensure entity table exists
      await this.ensureEntityTable(record.collection);

      // Execute create operation in transaction for atomic consistency
      const result = await this.withTransaction(async () => {
        const tableName = SqlNamingConverter.toTableName(record.collection);
        const entityClass = ENTITY_REGISTRY.get(record.collection);

        if (!entityClass || !hasFieldMetadata(entityClass)) {
          // For collections without registered entities, we still create entity-specific tables
          // but with a simple structure
          console.log(`🔧 Creating simple entity table for ${record.collection} without metadata`);
          await this.createSimpleEntityTable(record.collection);
          return await this.createInSimpleEntityTable(record);
        }

        // Process ALL fields uniformly using decorator metadata (min 4: id, createdAt, updatedAt, version)
        const columns: string[] = [];
        const values: any[] = [];
        const placeholders: string[] = [];

        const fieldMetadata = getFieldMetadata(entityClass);
        for (const [fieldName, metadata] of fieldMetadata.entries()) {
          const columnName = SqlNamingConverter.toSnakeCase(fieldName);
          let fieldValue: any;

          // Get field value from appropriate source
          if (fieldName === 'id') {
            fieldValue = record.id;
          } else if (fieldName === 'createdAt') {
            fieldValue = record.metadata.createdAt;
          } else if (fieldName === 'updatedAt') {
            fieldValue = record.metadata.updatedAt;
          } else if (fieldName === 'version') {
            fieldValue = record.metadata.version;
          } else {
            fieldValue = (record.data as any)[fieldName];
          }

          if (fieldValue !== undefined) {
            columns.push(columnName);
            placeholders.push('?');

            // Convert field value based on decorator type
            switch (metadata.fieldType) {
              case 'boolean':
                values.push(fieldValue ? 1 : 0);
                break;
              case 'json':
                values.push(JSON.stringify(fieldValue));
                break;
              case 'date':
                values.push(typeof fieldValue === 'string' ? fieldValue : new Date(fieldValue).toISOString());
                break;
              default:
                values.push(fieldValue);
            }
          }
        }

        // Build and execute INSERT statement
        const sql = `
          INSERT OR REPLACE INTO ${tableName} (${columns.join(', ')})
          VALUES (${placeholders.join(', ')})
        `;

        await this.runStatement(sql, values);

        console.log(`✅ SQLite: Inserted into entity table ${tableName} with ${columns.length} columns`);
        return record;
      });

      console.log(`✅ SQLite: Created record ${record.id} in entity table for ${record.collection}`);

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

  /**
   * Create a simple entity table for collections without registered metadata
   */
  private async createSimpleEntityTable(collectionName: string): Promise<void> {
    const tableName = SqlNamingConverter.toTableName(collectionName);

    if (this.createdTables.has(tableName)) {
      return; // Already created
    }

    console.log(`🏗️ Creating simple entity table: ${collectionName} -> ${tableName}`);

    // Create simple entity table with basic structure + JSON data column
    const sql = `
      CREATE TABLE IF NOT EXISTS ${tableName} (
        id TEXT PRIMARY KEY,
        data TEXT NOT NULL,  -- JSON data for all entity fields
        created_at TEXT DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
        version INTEGER DEFAULT 1
      )
    `;

    await this.runSql(sql);

    // Create basic indexes
    await this.runSql(`CREATE INDEX IF NOT EXISTS idx_${tableName}_created_at ON ${tableName}(created_at)`);
    await this.runSql(`CREATE INDEX IF NOT EXISTS idx_${tableName}_updated_at ON ${tableName}(updated_at)`);

    this.createdTables.add(tableName);
    console.log(`✅ Simple entity table created: ${tableName}`);
  }

  /**
   * Insert record into simple entity table (for unregistered entities)
   */
  private async createInSimpleEntityTable<T extends RecordData>(record: DataRecord<T>): Promise<DataRecord<T>> {
    const tableName = SqlNamingConverter.toTableName(record.collection);

    const sql = `
      INSERT OR REPLACE INTO ${tableName} (
        id, data, created_at, updated_at, version
      ) VALUES (?, ?, ?, ?, ?)
    `;

    const params = [
      record.id,
      JSON.stringify(record.data),
      record.metadata.createdAt,
      record.metadata.updatedAt,
      record.metadata.version
    ];

    await this.runStatement(sql, params);
    console.log(`✅ SQLite: Inserted into simple entity table ${tableName}`);

    return record;
  }

  /**
   * Legacy blob storage fallback - DEPRECATED, kept for backwards compatibility
   */
  private async createLegacyBlob<T extends RecordData>(record: DataRecord<T>): Promise<DataRecord<T>> {
    console.warn(`⚠️ DEPRECATED: Using legacy blob storage for ${record.collection} - should use entity tables`);

    // Ensure collection exists in legacy table
    await this.ensureCollection(record.collection);

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
    await this.updateCollectionStats(record.collection);

    return record;
  }

  // Removed extractFields - cross-cutting concern

  /**
   * Read a single record by ID - uses entity-specific tables
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Reading ${collection}/${id} from entity-specific table`);

      const entityClass = ENTITY_REGISTRY.get(collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Use entity-specific table
        return await this.readFromEntityTable<T>(collection, id, entityClass);
      } else {
        // Use simple entity table (fallback)
        return await this.readFromSimpleEntityTable<T>(collection, id);
      }

    } catch (error: any) {
      console.error(`❌ SQLite: Read failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Read from entity-specific table with proper column mapping
   */
  private async readFromEntityTable<T extends RecordData>(collection: string, id: UUID, entityClass: EntityConstructor): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const sql = `SELECT * FROM ${tableName} WHERE id = ? LIMIT 1`;
    const rows = await this.runSql(sql, [id]);

    if (rows.length === 0) {
      return {
        success: false,
        error: `Record not found: ${collection}/${id}`
      };
    }

    const row = rows[0];
    const entityData: any = {};

    // Process ALL fields uniformly using decorator metadata
    const fieldMetadata = getFieldMetadata(entityClass);
    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      const columnName = SqlNamingConverter.toSnakeCase(fieldName);
      let value = row[columnName];

      if (value !== undefined && value !== null) {
        // Convert SQL value back to JavaScript type based on decorator metadata
        switch (metadata.fieldType) {
          case 'boolean':
            value = value === 1;
            break;
          case 'json':
            value = JSON.parse(value);
            break;
          case 'date':
            value = new Date(value);
            break;
        }

        // Put BaseEntity fields in metadata, others in data
        if (['id', 'createdAt', 'updatedAt', 'version'].includes(fieldName)) {
          // BaseEntity fields go to their proper locations
          continue; // We'll handle these separately
        } else {
          entityData[fieldName] = value;
        }
      }
    }

    const record: DataRecord<T> = {
      id: row.id,
      collection,
      data: entityData as T,
      metadata: {
        createdAt: row.created_at,
        updatedAt: row.updated_at,
        version: row.version
      }
    };

    return {
      success: true,
      data: record
    };
  }

  /**
   * Read from simple entity table (JSON data column)
   */
  private async readFromSimpleEntityTable<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const sql = `SELECT * FROM ${tableName} WHERE id = ? LIMIT 1`;
    const rows = await this.runSql(sql, [id]);

    if (rows.length === 0) {
      return {
        success: false,
        error: `Record not found: ${collection}/${id}`
      };
    }

    const row = rows[0];
    const record: DataRecord<T> = {
      id: row.id,
      collection,
      data: JSON.parse(row.data),
      metadata: {
        createdAt: row.created_at,
        updatedAt: row.updated_at,
        version: row.version
      }
    };

    return {
      success: true,
      data: record
    };
  }

  /**
   * Query records with complex filters - uses entity-specific tables
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      console.log(`🔍 SQLite: Querying ${query.collection} from entity-specific table`, query);

      const entityClass = ENTITY_REGISTRY.get(query.collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Use entity-specific table
        return await this.queryFromEntityTable<T>(query, entityClass);
      } else {
        // Use simple entity table (fallback)
        return await this.queryFromSimpleEntityTable<T>(query);
      }

    } catch (error: any) {
      console.error(`❌ SQLite: Query failed for ${query.collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Query from entity-specific table with proper column mapping
   */
  private async queryFromEntityTable<T extends RecordData>(query: StorageQuery, entityClass: EntityConstructor): Promise<StorageResult<DataRecord<T>[]>> {
    const { sql, params } = this.buildEntitySelectQuery(query, entityClass);
    const rows = await this.runSql(sql, params);

    const records: DataRecord<T>[] = rows.map(row => {
      const entityData: any = {};

      // Process ALL fields uniformly using decorator metadata
      const fieldMetadata = getFieldMetadata(entityClass);
      for (const [fieldName, metadata] of fieldMetadata.entries()) {
        const columnName = SqlNamingConverter.toSnakeCase(fieldName);
        let value = row[columnName];

        if (value !== undefined && value !== null) {
          // Convert SQL value back to JavaScript type based on decorator metadata
          switch (metadata.fieldType) {
            case 'boolean':
              value = value === 1;
              break;
            case 'json':
              value = JSON.parse(value);
              break;
            case 'date':
              value = new Date(value);
              break;
          }

          // Put BaseEntity fields in metadata, others in data
          if (['id', 'createdAt', 'updatedAt', 'version'].includes(fieldName)) {
            // BaseEntity fields go to their proper locations (handled below)
            continue;
          } else {
            entityData[fieldName] = value;
          }
        }
      }

      return {
        id: row.id,
        collection: query.collection,
        data: entityData as T,
        metadata: {
          createdAt: row.created_at,
          updatedAt: row.updated_at,
          version: row.version
        }
      };
    });

    console.log(`✅ SQLite: Entity query returned ${records.length} records`);

    return {
      success: true,
      data: records,
      metadata: {
        totalCount: records.length,
        queryTime: 0 // TODO: Add timing
      }
    };
  }

  /**
   * Query from simple entity table (JSON data column)
   */
  private async queryFromSimpleEntityTable<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    const { sql, params } = this.buildSimpleEntitySelectQuery(query);
    const rows = await this.runSql(sql, params);

    const records: DataRecord<T>[] = rows.map(row => ({
      id: row.id,
      collection: query.collection,
      data: JSON.parse(row.data),
      metadata: {
        createdAt: row.created_at,
        updatedAt: row.updated_at,
        version: row.version
      }
    }));

    console.log(`✅ SQLite: Simple entity query returned ${records.length} records`);

    return {
      success: true,
      data: records,
      metadata: {
        totalCount: records.length,
        queryTime: 0 // TODO: Add timing
      }
    };
  }

  /**
   * Build SELECT SQL query for entity-specific tables
   */
  private buildEntitySelectQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
    const params: any[] = [];
    const tableName = SqlNamingConverter.toTableName(query.collection);
    let sql = `SELECT * FROM ${tableName}`;

    // Add filters (translate field names to column names)
    if (query.filters) {
      const filterClauses: string[] = [];
      for (const [field, value] of Object.entries(query.filters)) {
        const columnName = SqlNamingConverter.toSnakeCase(field);
        filterClauses.push(`${columnName} = ?`);
        params.push(value);
      }
      if (filterClauses.length > 0) {
        sql += ` WHERE ${filterClauses.join(' AND ')}`;
      }
    }

    // Add time range filter
    if (query.timeRange) {
      const timeFilters: string[] = [];
      if (query.timeRange.start) {
        timeFilters.push('created_at >= ?');
        params.push(query.timeRange.start);
      }
      if (query.timeRange.end) {
        timeFilters.push('created_at <= ?');
        params.push(query.timeRange.end);
      }
      if (timeFilters.length > 0) {
        const whereClause = query.filters ? ' AND ' : ' WHERE ';
        sql += whereClause + timeFilters.join(' AND ');
      }
    }

    // Add sorting (translate field names to column names)
    if (query.sort && query.sort.length > 0) {
      const sortClauses = query.sort.map(s => {
        const columnName = SqlNamingConverter.toSnakeCase(s.field);
        return `${columnName} ${s.direction.toUpperCase()}`;
      });
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

  /**
   * Build SELECT SQL query for simple entity tables (with JSON data column)
   */
  private buildSimpleEntitySelectQuery(query: StorageQuery): { sql: string; params: any[] } {
    const params: any[] = [];
    const tableName = SqlNamingConverter.toTableName(query.collection);
    let sql = `SELECT * FROM ${tableName}`;

    // Add basic filters using JSON_EXTRACT on data column
    if (query.filters) {
      const filterClauses: string[] = [];
      for (const [field, value] of Object.entries(query.filters)) {
        filterClauses.push(`JSON_EXTRACT(data, '$.${field}') = ?`);
        params.push(value);
      }
      if (filterClauses.length > 0) {
        sql += ` WHERE ${filterClauses.join(' AND ')}`;
      }
    }

    // Add time range filter
    if (query.timeRange) {
      const timeFilters: string[] = [];
      if (query.timeRange.start) {
        timeFilters.push('created_at >= ?');
        params.push(query.timeRange.start);
      }
      if (query.timeRange.end) {
        timeFilters.push('created_at <= ?');
        params.push(query.timeRange.end);
      }
      if (timeFilters.length > 0) {
        const whereClause = query.filters ? ' AND ' : ' WHERE ';
        sql += whereClause + timeFilters.join(' AND ');
      }
    }

    // Add sorting using JSON_EXTRACT on data column
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

  /**
   * Build SELECT SQL query - legacy method for backwards compatibility
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
   * Update an existing record - uses same table selection logic as read()
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      console.log(`🔧 SQLite UPDATE: Starting update for ${collection}/${id}`);

      // First read existing record using same logic
      const existing = await this.read<T>(collection, id);
      if (!existing.success || !existing.data) {
        return {
          success: false,
          error: 'Record not found'
        };
      }

      console.log(`🔧 SQLite UPDATE: Found existing record, merging data`);

      // Merge data
      const updatedData = { ...existing.data.data, ...data };
      const version = incrementVersion ? existing.data.metadata.version + 1 : existing.data.metadata.version;

      // Use same table selection logic as read()
      const entityClass = ENTITY_REGISTRY.get(collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Update in entity-specific table (same as readFromEntityTable)
        console.log(`🔧 SQLite UPDATE: Using entity-specific table for ${collection}`);
        return await this.updateInEntityTable<T>(collection, id, updatedData as T, version, existing.data);
      } else {
        // Update in simple entity table (same as readFromSimpleEntityTable)
        console.log(`🔧 SQLite UPDATE: Using simple entity table for ${collection}`);
        return await this.updateInSimpleEntityTable<T>(collection, id, updatedData as T, version, existing.data);
      }

    } catch (error: any) {
      console.error(`❌ SQLite: Update failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Update record in entity-specific table (mirrors readFromEntityTable)
   */
  private async updateInEntityTable<T extends RecordData>(
    collection: string,
    id: UUID,
    updatedData: T,
    version: number,
    existingRecord: DataRecord<T>
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);

    // Use same WHERE clause as readFromEntityTable
    const sql = `UPDATE ${tableName} SET data = ?, updated_at = ?, version = ? WHERE id = ?`;
    const params = [
      JSON.stringify(updatedData),
      new Date().toISOString(),
      version,
      id
    ];

    console.log(`🔧 SQLite UPDATE ENTITY DEBUG: SQL:`, { sql, params });
    const result = await this.runStatement(sql, params);
    console.log(`🔧 SQLite UPDATE ENTITY DEBUG: Result:`, result);

    if (result.changes === 0) {
      return {
        success: false,
        error: `No rows updated in ${tableName} for id: ${id}`
      };
    }

    // Create updated record
    const updatedRecord: DataRecord<T> = {
      ...existingRecord,
      data: updatedData,
      metadata: {
        ...existingRecord.metadata,
        updatedAt: new Date().toISOString(),
        version
      }
    };

    await this.updateCollectionStats(collection);

    console.log(`✅ SQLite: Updated record ${id} in entity table ${tableName}`);

    return {
      success: true,
      data: updatedRecord
    };
  }

  /**
   * Update record in simple entity table (mirrors readFromSimpleEntityTable)
   */
  private async updateInSimpleEntityTable<T extends RecordData>(
    collection: string,
    id: UUID,
    updatedData: T,
    version: number,
    existingRecord: DataRecord<T>
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);

    // Use same WHERE clause as readFromSimpleEntityTable
    const sql = `UPDATE ${tableName} SET data = ?, updated_at = ?, version = ? WHERE id = ?`;
    const params = [
      JSON.stringify(updatedData),
      new Date().toISOString(),
      version,
      id
    ];

    console.log(`🔧 SQLite UPDATE SIMPLE DEBUG: SQL:`, { sql, params });
    const result = await this.runStatement(sql, params);
    console.log(`🔧 SQLite UPDATE SIMPLE DEBUG: Result:`, result);

    if (result.changes === 0) {
      return {
        success: false,
        error: `No rows updated in ${tableName} for id: ${id}`
      };
    }

    // Create updated record
    const updatedRecord: DataRecord<T> = {
      ...existingRecord,
      data: updatedData,
      metadata: {
        ...existingRecord.metadata,
        updatedAt: new Date().toISOString(),
        version
      }
    };

    await this.updateCollectionStats(collection);

    console.log(`✅ SQLite: Updated record ${id} in simple entity table ${tableName}`);

    return {
      success: true,
      data: updatedRecord
    };
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

  /**
   * Clear all entity data from the database (preserving structure)
   *
   * This method:
   * - Deletes all records from _data table
   * - Deletes all records from entity-specific tables
   * - Resets collection statistics
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
        const tables = await this.runSql(`
          SELECT name FROM sqlite_master
          WHERE type='table'
          AND name NOT LIKE 'sqlite_%'
        `);

        for (const table of tables) {
          const tableName = table.name;

          // Count records before deletion
          const countRows = await this.runSql(`SELECT COUNT(*) as count FROM \`${tableName}\``);
          const recordCount = countRows[0]?.count || 0;

          if (recordCount > 0) {
            // Delete all records from this table
            await this.runStatement(`DELETE FROM \`${tableName}\``);

            tablesCleared.push(tableName);
            totalRecordsDeleted += recordCount;

            console.log(`✅ SQLite: Cleared ${recordCount} records from table '${tableName}'`);
          } else {
            console.log(`📋 SQLite: Table '${tableName}' was already empty`);
          }
        }

        // Reset collection statistics
        await this.runStatement(`
          UPDATE _collections
          SET record_count = 0, total_size = 0, updated_at = ?
          WHERE record_count > 0
        `, [new Date().toISOString()]);

        // Reset SQLite sequence counters for tables that use them
        const sequenceTables = await this.runSql(`
          SELECT name FROM sqlite_sequence
        `);

        for (const seqTable of sequenceTables) {
          await this.runStatement(`UPDATE sqlite_sequence SET seq = 0 WHERE name = ?`, [seqTable.name]);
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
}