/**
 * SqliteSchemaManager - Schema Management for SQLite
 *
 * Handles:
 * - Database initialization and configuration
 * - Table creation and schema migrations
 * - Integrity verification
 * - Entity registration and schema generation
 */

import sqlite3 from 'sqlite3';
import * as path from 'path';
import * as fs from 'fs/promises';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

import type { StorageResult, CollectionSchema, SchemaField, SchemaFieldType } from '../../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';
import type { SqlExecutor } from '../SqlExecutor';
import type { FieldType, FieldMetadata } from '../../../../system/data/decorators/FieldDecorators';
import { Logger } from '../../../../system/core/logging/Logger';

/**
 * Entity constructor type - for migration methods that still need entity class
 */
type EntityConstructor = (new (...args: unknown[]) => unknown) & {
  prototype: {
    [key: string]: unknown;
  };
};

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
 * SqliteSchemaManager - Manages database schema and initialization
 */
export class SqliteSchemaManager {
  private log = Logger.create('SqliteSchemaManager', 'sql');
  private schemaVerified: Set<string> = new Set(); // Cache: only check schema once per process
  private schemaCache = new Map<string, CollectionSchema>(); // Cache passed schemas

  /**
   * Get cached schema for a collection
   *
   * ARCHITECTURE: Provides schema to other managers (WriteManager, QueryExecutor)
   * so they don't need to access ENTITY_REGISTRY directly.
   */
  getCachedSchema(collection: string): CollectionSchema | undefined {
    return this.schemaCache.get(collection);
  }

  constructor(
    private db: sqlite3.Database | null,
    private executor: SqlExecutor,
    private generateCreateTableSql: (
      collectionName: string,
      entityClass: EntityConstructor,
      toTableName: (name: string) => string,
      toSnakeCase: (name: string) => string
    ) => string,
    private generateCreateIndexSql: (
      collectionName: string,
      entityClass: EntityConstructor,
      toTableName: (name: string) => string,
      toSnakeCase: (name: string) => string
    ) => string[],
    private mapFieldTypeToSql: (fieldType: FieldType, options?: FieldMetadata['options']) => string
  ) {}

  /**
   * Configure SQLite performance and behavior settings
   */
  async configureSqlite(options: SqliteOptions): Promise<void> {
    if (!this.db) return;

    // Set temp directory relative to database (for VACUUM operations)
    const { getDatabaseDir } = await import('../../../../system/config/ServerConfig');
    const path = await import('path');
    const fs = await import('fs');
    const tempDir = path.join(getDatabaseDir(), 'tmp');  // Expand $HOME in path
    try {
      await fs.promises.mkdir(tempDir, { recursive: true, mode: 0o755 });
      process.env.SQLITE_TMPDIR = tempDir;
    } catch (error) {
      this.log.warn('Could not set temp directory:', error);
    }

    const settings = [
      // Set foreign keys based on configuration
      options.foreignKeys === false ? 'PRAGMA foreign_keys = OFF' : 'PRAGMA foreign_keys = ON',

      // Incremental auto-vacuum for gradual space reclamation (safe, no exclusive lock needed)
      'PRAGMA auto_vacuum = INCREMENTAL',

      // WAL mode for better concurrency and performance
      'PRAGMA journal_mode = WAL',

      // Balanced safety/performance (not exFAT anymore)
      'PRAGMA synchronous = NORMAL',

      // Set cache size (negative = KB, positive = pages)
      options.cacheSize ? `PRAGMA cache_size = ${options.cacheSize}` : 'PRAGMA cache_size = -2000',

      // Set busy timeout
      options.timeout ? `PRAGMA busy_timeout = ${options.timeout}` : 'PRAGMA busy_timeout = 10000'
    ].filter(Boolean);

    for (const sql of settings) {
      if (sql) {
        await this.executor.runSql(sql);
      }
    }

    this.log.info('SQLite configuration applied (WAL mode, incremental auto-vacuum)');
  }

  /**
   * Verify database integrity and write capability
   */
  async verifyIntegrity(): Promise<void> {
    // For Rust adapter, db may be null (Rust manages connection)
    // Verification proceeds via executor instead
    if (!this.db && !this.executor) {
      throw new Error('Neither database nor executor initialized');
    }

    this.log.info('Creating system_info table for version tracking...');

    try {
      // Create system_info table to track database version and initialization
      await this.executor.runSql(`
        CREATE TABLE IF NOT EXISTS system_info (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL,
          created_at TEXT DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
      `);
      this.log.info('system_info table created');

      // Insert database version and metadata
      const initTime = new Date().toISOString();
      await this.executor.runSql(
        'INSERT OR REPLACE INTO system_info (key, value, created_at, updated_at) VALUES (?, ?, ?, ?)',
        ['db_version', '1.0.0', initTime, initTime]
      );
      await this.executor.runSql(
        'INSERT OR REPLACE INTO system_info (key, value, created_at, updated_at) VALUES (?, ?, ?, ?)',
        ['adapter_type', 'SqliteStorageAdapter', initTime, initTime]
      );
      await this.executor.runSql(
        'INSERT OR REPLACE INTO system_info (key, value, created_at, updated_at) VALUES (?, ?, ?, ?)',
        ['node_version', process.version, initTime, initTime]
      );
      await this.executor.runSql(
        'INSERT OR REPLACE INTO system_info (key, value, created_at, updated_at) VALUES (?, ?, ?, ?)',
        ['platform', process.platform, initTime, initTime]
      );
      await this.executor.runSql(
        'INSERT OR REPLACE INTO system_info (key, value, created_at, updated_at) VALUES (?, ?, ?, ?)',
        ['last_init', initTime, initTime, initTime]
      );
      this.log.info('System info populated');

      // Verify we can read it back
      const results = await this.executor.runSql('SELECT key, value FROM system_info WHERE key = ?', ['db_version']);
      if (!results || results.length === 0 || results[0].value !== '1.0.0') {
        throw new Error('Read verification failed - system_info data mismatch');
      }
      this.log.info('Read verification successful');

      this.log.info('Database integrity verified - adapter fully functional');

    } catch (error) {
      this.log.error('Integrity verification failed:', error);
      this.log.error('Error details:', error instanceof Error ? error.message : String(error));
      throw new Error(`Database integrity check failed: ${error}`);
    }
  }

  // ============================================================================
  // SCHEMA-BASED TABLE/INDEX GENERATION (New architecture - adapter doesn't know entities)
  // ============================================================================

  /**
   * Map SchemaFieldType to SQLite SQL type
   *
   * ARCHITECTURE: This is how the adapter translates generic schema types
   * to its native storage format. No knowledge of entities or decorators.
   */
  private mapSchemaFieldTypeToSql(fieldType: SchemaFieldType, maxLength?: number): string {
    switch (fieldType) {
      case 'uuid':
        return 'TEXT';  // SQLite doesn't have native UUID, use TEXT
      case 'string':
        return maxLength ? `TEXT` : 'TEXT';  // SQLite doesn't enforce VARCHAR length
      case 'number':
        return 'REAL';
      case 'boolean':
        return 'INTEGER';  // SQLite uses 0/1 for boolean
      case 'date':
        return 'TEXT';  // ISO8601 string format
      case 'json':
        return 'TEXT';  // JSON stored as text
      default:
        return 'TEXT';
    }
  }

  /**
   * Generate CREATE TABLE SQL from CollectionSchema
   *
   * ARCHITECTURE: Adapter generates native SQL from generic schema.
   * Daemon passed the schema, adapter translates to native format.
   */
  private generateCreateTableFromSchema(schema: CollectionSchema): string {
    const tableName = SqlNamingConverter.toTableName(schema.collection);

    const columns: string[] = [
      'id TEXT PRIMARY KEY',
      'created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP',
      'updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP',
      'version INTEGER DEFAULT 1'
    ];

    for (const field of schema.fields) {
      // Skip base entity fields (already added above)
      if (['id', 'createdAt', 'updatedAt', 'version'].includes(field.name)) {
        continue;
      }

      const columnName = SqlNamingConverter.toSnakeCase(field.name);
      const sqlType = this.mapSchemaFieldTypeToSql(field.type, field.maxLength);
      const nullable = field.nullable !== false ? '' : ' NOT NULL';
      const unique = field.unique ? ' UNIQUE' : '';

      columns.push(`${columnName} ${sqlType}${nullable}${unique}`);
    }

    return `CREATE TABLE IF NOT EXISTS ${tableName} (${columns.join(', ')})`;
  }

  /**
   * Generate CREATE INDEX SQL statements from CollectionSchema
   *
   * ARCHITECTURE: Creates indexes for indexed fields and composite indexes.
   * Uses IF NOT EXISTS for idempotent operations.
   */
  private generateCreateIndexFromSchema(schema: CollectionSchema): string[] {
    const tableName = SqlNamingConverter.toTableName(schema.collection);
    const indexes: string[] = [];

    // Single-field indexes from field.indexed
    for (const field of schema.fields) {
      if (field.indexed) {
        const columnName = SqlNamingConverter.toSnakeCase(field.name);
        const indexName = `idx_${tableName}_${columnName}`;
        indexes.push(`CREATE INDEX IF NOT EXISTS ${indexName} ON ${tableName} (${columnName})`);
      }
    }

    // Composite indexes
    if (schema.indexes) {
      for (const idx of schema.indexes) {
        const indexColumns = idx.fields.map(f => SqlNamingConverter.toSnakeCase(f)).join(', ');
        const uniqueStr = idx.unique ? 'UNIQUE ' : '';
        indexes.push(`CREATE ${uniqueStr}INDEX IF NOT EXISTS ${idx.name} ON ${tableName} (${indexColumns})`);
      }
    }

    return indexes;
  }

  /**
   * Ensure schema exists for collection (orchestrated by DataDaemon)
   *
   * This is the ONLY place where tables are created.
   *
   * ARCHITECTURE: Daemon extracts schema from entity decorators and passes it here.
   * The adapter doesn't need to know about entities or decorators.
   * Schema MUST be provided - no fallback to ENTITY_REGISTRY.
   */
  async ensureSchema(collectionName: string, schema?: CollectionSchema): Promise<StorageResult<boolean>> {
    try {
      // Fast path: already verified this schema in this process
      if (this.schemaVerified.has(collectionName)) {
        return { success: true, data: true };
      }

      // Schema MUST be provided by daemon - no fallback
      if (!schema) {
        const errorMessage = `No schema provided for collection "${collectionName}". ` +
          `DataDaemon must extract schema from entity decorators and pass it to ensureSchema(). ` +
          `This usually means the entity is not registered in EntityRegistry.ts.`;
        this.log.error(errorMessage);
        return {
          success: false,
          error: errorMessage
        };
      }

      const tableName = SqlNamingConverter.toTableName(collectionName);
      const tableExists = await this.tableExists(tableName);

      // Cache the schema for later use in queries/writes
      this.schemaCache.set(collectionName, schema);

      let stateChanged = false;

      if (tableExists) {
        // TODO: Implement schema-based migration (add missing columns)
        // For now, skip migration - table already exists
        this.log.debug(`Table ${tableName} exists, using existing schema`);
      } else {
        // Create new table from schema - STATE CHANGE
        const createTableSql = this.generateCreateTableFromSchema(schema);
        this.log.info(`CREATE TABLE ${tableName}`);
        await this.executor.runSql(createTableSql);
        stateChanged = true;
      }

      // Create indexes from schema
      const indexSqls = this.generateCreateIndexFromSchema(schema);
      for (const indexSql of indexSqls) {
        this.log.debug(`Creating index: ${indexSql}`);
        await this.executor.runSql(indexSql);
      }

      if (stateChanged) {
        this.log.info(`Table ready: ${tableName} (created with ${indexSqls.length} indexes)`);
      }

      this.schemaVerified.add(collectionName);
      return { success: true, data: true };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log.error(`Failed to ensure schema for ${collectionName}:`, errorMessage);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Check if a table exists in the database
   */
  async tableExists(tableName: string): Promise<boolean> {
    const result = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [tableName]
    );
    return result.length > 0;
  }

  /**
   * Get existing columns for a table
   */
  async getTableColumns(tableName: string): Promise<Set<string>> {
    const result = await this.executor.runSql(`PRAGMA table_info(${tableName})`);
    return new Set(result.map((row: { name: string }) => row.name));
  }

  /**
   * Migrate simple entity table by ensuring required snake_case columns exist
   * Handles old tables created with camelCase columns (createdAt, updatedAt)
   * by adding the proper snake_case columns (created_at, updated_at)
   */
  async migrateSimpleEntityTable(tableName: string): Promise<void> {
    // Get existing columns
    const existingColumns = await this.getTableColumns(tableName);

    // Required columns for simple entity table (snake_case)
    const requiredColumns = [
      { name: 'id', type: 'TEXT', nullable: false },
      { name: 'data', type: 'TEXT', nullable: false },
      { name: 'created_at', type: 'TEXT', nullable: true },
      { name: 'updated_at', type: 'TEXT', nullable: true },
      { name: 'version', type: 'INTEGER', nullable: true }
    ];

    const missingColumns: string[] = [];

    for (const column of requiredColumns) {
      if (!existingColumns.has(column.name)) {
        missingColumns.push(column.name);

        // Generate ALTER TABLE statement
        let alterSql = `ALTER TABLE ${tableName} ADD COLUMN ${column.name} ${column.type}`;

        if (!column.nullable) {
          // For NOT NULL columns, provide default
          alterSql += ` DEFAULT ${this.getDefaultForType(column.type)} NOT NULL`;
        } else {
          // For nullable columns, allow NULL
          alterSql += ' DEFAULT NULL';
        }

        this.log.info(`Adding missing column: ${column.name} (${column.type})`);
        await this.executor.runSql(alterSql);
      }
    }

    if (missingColumns.length > 0) {
      this.log.info(`Migrated simple entity table ${tableName}: added ${missingColumns.length} columns (${missingColumns.join(', ')})`);
    } else {
      this.log.info(`Simple entity table ${tableName} schema is up-to-date`);
    }
  }

  /**
   * Format default value for SQL
   */
  private formatDefaultValue(value: unknown, sqlType: string): string {
    if (value === null) return 'NULL';
    if (typeof value === 'string') return `'${value.replace(/'/g, "''")}'`;
    if (typeof value === 'number') return String(value);
    if (typeof value === 'boolean') return value ? '1' : '0';
    if (sqlType === 'TEXT') return `'${JSON.stringify(value).replace(/'/g, "''")}'`;
    return 'NULL';
  }

  /**
   * Get sensible default value for SQL type
   */
  private getDefaultForType(sqlType: string): string {
    switch (sqlType) {
      case 'INTEGER':
      case 'REAL':
        return '0';
      case 'TEXT':
        return "''";
      default:
        return 'NULL';
    }
  }

  /**
   * Create core schema for collections and metadata
   */
  async createCoreSchema(): Promise<void> {
    // Collections registry table
    await this.executor.runSql(`
      CREATE TABLE IF NOT EXISTS _collections (
        name TEXT PRIMARY KEY,
        schema_version INTEGER DEFAULT 1,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
        record_count INTEGER DEFAULT 0,
        metadata TEXT DEFAULT '{}'
      )
    `);
    this.log.info('Core schema: _collections table created');
  }
}
