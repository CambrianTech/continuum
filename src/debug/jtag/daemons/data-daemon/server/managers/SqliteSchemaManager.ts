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

import type { StorageResult } from '../../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';
import { SqliteRawExecutor } from '../SqliteRawExecutor';
import {
  getFieldMetadata,
  hasFieldMetadata,
  type FieldMetadata,
  type FieldType
} from '../../../../system/data/decorators/FieldDecorators';
import { ENTITY_REGISTRY, type EntityConstructor } from '../EntityRegistry';

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
  constructor(
    private db: sqlite3.Database,
    private executor: SqliteRawExecutor,
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

    const settings = [
      // Set foreign keys based on configuration
      options.foreignKeys === false ? 'PRAGMA foreign_keys = OFF' : 'PRAGMA foreign_keys = ON',

      // EXFAT FIX: Use MEMORY journal mode to avoid file permission issues
      // WAL and DELETE modes create auxiliary files that may have permission problems
      'PRAGMA journal_mode = MEMORY',

      // EXFAT FIX: Use FULL synchronous mode for data integrity without relying on filesystem
      'PRAGMA synchronous = FULL',

      // Set cache size (negative = KB, positive = pages)
      options.cacheSize ? `PRAGMA cache_size = ${options.cacheSize}` : 'PRAGMA cache_size = -2000',

      // Set busy timeout
      options.timeout ? `PRAGMA busy_timeout = ${options.timeout}` : 'PRAGMA busy_timeout = 10000',

      // EXFAT FIX: Disable locking mode to reduce filesystem permission requirements
      'PRAGMA locking_mode = NORMAL'
    ].filter(Boolean);

    for (const sql of settings) {
      if (sql) {
        await this.executor.runSql(sql);
      }
    }

    console.log('⚙️ SQLite: Configuration applied (exFAT-compatible settings)');
  }

  /**
   * Verify database integrity and write capability
   */
  async verifyIntegrity(): Promise<void> {
    if (!this.db) {
      throw new Error('Database not initialized');
    }

    console.log('🧪 SQLite: Creating system_info table for version tracking...');

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
      console.log('✅ SQLite: system_info table created');

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
      console.log('✅ SQLite: System info populated');

      // Verify we can read it back
      const results = await this.executor.runSql('SELECT key, value FROM system_info WHERE key = ?', ['db_version']);
      if (!results || results.length === 0 || results[0].value !== '1.0.0') {
        throw new Error('Read verification failed - system_info data mismatch');
      }
      console.log('✅ SQLite: Read verification successful');

      console.log('🎉 SQLite: Database integrity verified - adapter fully functional');

    } catch (error) {
      console.error('❌ SQLite: Integrity verification failed:', error);
      console.error('❌ SQLite: Error details:', error instanceof Error ? error.message : String(error));
      throw new Error(`Database integrity check failed: ${error}`);
    }
  }

  /**
   * Ensure schema exists for collection (orchestrated by DataDaemon)
   *
   * This is the ONLY place where tables are created.
   * Handles both registered entities (with metadata) and unregistered entities (simple table).
   * Adapter translates collection → table name and creates snake_case columns.
   */
  async ensureSchema(collectionName: string, _schema?: unknown): Promise<StorageResult<boolean>> {
    try {
      const tableName = SqlNamingConverter.toTableName(collectionName);
      const tableExists = await this.tableExists(tableName);

      const entityClass = ENTITY_REGISTRY.get(collectionName);

      if (!entityClass || !hasFieldMetadata(entityClass)) {
        // ERROR: Unregistered entity - this is a bug that must be fixed
        const errorMessage = `❌ Entity '${collectionName}' is not registered in EntityRegistry!\n\n` +
          `To fix:\n` +
          `1. Create entity class: system/data/entities/${collectionName.charAt(0).toUpperCase() + collectionName.slice(1)}Entity.ts\n` +
          `2. Extend BaseEntity and add @TextField(), @NumberField(), @JsonField() decorators\n` +
          `3. Register in EntityRegistry.ts:\n` +
          `   - Import: import { ${collectionName.charAt(0).toUpperCase() + collectionName.slice(1)}Entity } from '../../../system/data/entities/${collectionName.charAt(0).toUpperCase() + collectionName.slice(1)}Entity';\n` +
          `   - Initialize: new ${collectionName.charAt(0).toUpperCase() + collectionName.slice(1)}Entity();\n` +
          `   - Register: registerEntity(${collectionName.charAt(0).toUpperCase() + collectionName.slice(1)}Entity.collection, ${collectionName.charAt(0).toUpperCase() + collectionName.slice(1)}Entity);\n`;

        console.error(errorMessage);
        return {
          success: false,
          error: errorMessage
        };
      }

      // Registered entity - handle full entity table with metadata
      console.log(`🏗️ Setting up table for entity: ${collectionName} -> ${tableName}`);

      if (tableExists) {
        // Migrate schema: add missing columns
        await this.migrateTableSchema(collectionName, tableName, entityClass);
      } else {
        // Create new table
        const createTableSql = this.generateCreateTableSql(
          collectionName,
          entityClass,
          SqlNamingConverter.toTableName,
          SqlNamingConverter.toSnakeCase
        );
        await this.executor.runSql(createTableSql);
        console.log(`✅ Table created: ${tableName}`);
      }

      // Create indexes (will skip if they already exist due to IF NOT EXISTS)
      const indexSqls = this.generateCreateIndexSql(
        collectionName,
        entityClass,
        SqlNamingConverter.toTableName,
        SqlNamingConverter.toSnakeCase
      );
      for (const indexSql of indexSqls) {
        await this.executor.runSql(indexSql);
      }

      console.log(`✅ Table ready: ${tableName} with ${indexSqls.length} indexes`);

      return { success: true, data: true };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ SQLite: Failed to ensure schema for ${collectionName}:`, errorMessage);
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
   * Migrate table schema by adding missing columns
   * SQLite only supports adding columns, not modifying or removing them
   * This runs automatically every time the server starts, ensuring schema stays in sync with entity definitions
   */
  async migrateTableSchema(
    collectionName: string,
    tableName: string,
    entityClass: EntityConstructor
  ): Promise<void> {
    // Get existing columns
    const existingColumns = await this.getTableColumns(tableName);

    // Get expected columns from entity metadata
    const fieldMetadata = getFieldMetadata(entityClass);
    const missingColumns: string[] = [];

    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      const columnName = SqlNamingConverter.toSnakeCase(fieldName);

      if (!existingColumns.has(columnName)) {
        missingColumns.push(columnName);

        // Generate ALTER TABLE statement
        const sqlType = this.mapFieldTypeToSql(metadata.fieldType, metadata.options);
        const nullable = metadata.options?.nullable !== false;
        const defaultValue = metadata.options?.default;

        let alterSql = `ALTER TABLE ${tableName} ADD COLUMN ${columnName} ${sqlType}`;

        if (!nullable) {
          // For NOT NULL columns on existing tables, we must provide a default
          if (defaultValue !== undefined) {
            alterSql += ` DEFAULT ${this.formatDefaultValue(defaultValue, sqlType)}`;
          } else {
            // Provide sensible defaults for required columns
            alterSql += ` DEFAULT ${this.getDefaultForType(sqlType)}`;
          }
          alterSql += ' NOT NULL';
        }

        console.log(`   🔄 Adding column: ${columnName} (${sqlType})`);
        await this.executor.runSql(alterSql);
      }
    }

    if (missingColumns.length > 0) {
      console.log(`✅ Migrated ${tableName}: added ${missingColumns.length} new columns`);
    } else {
      console.log(`✅ Schema up-to-date: ${tableName}`);
    }
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

        console.log(`   🔄 Adding missing column: ${column.name} (${column.type})`);
        await this.executor.runSql(alterSql);
      }
    }

    if (missingColumns.length > 0) {
      console.log(`✅ Migrated simple entity table ${tableName}: added ${missingColumns.length} columns (${missingColumns.join(', ')})`);
    } else {
      console.log(`✅ Simple entity table ${tableName} schema is up-to-date`);
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
   * Log what schemas would be generated for all registered entities
   */
  async logEntitySchemas(): Promise<void> {
    console.log('📋 SQLite: Analyzing registered entities...');

    for (const [collectionName, entityClass] of ENTITY_REGISTRY.entries()) {
      if (!hasFieldMetadata(entityClass)) {
        console.log(`⚠️ ${collectionName}: No field metadata found`);
        continue;
      }

      const tableName = SqlNamingConverter.toTableName(collectionName);
      console.log(`\n🏗️ ${collectionName} -> ${tableName}:`);

      // Log what CREATE TABLE would look like
      const createTableSql = this.generateCreateTableSql(
        collectionName,
        entityClass,
        SqlNamingConverter.toTableName,
        SqlNamingConverter.toSnakeCase
      );
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
      const indexSqls = this.generateCreateIndexSql(
        collectionName,
        entityClass,
        SqlNamingConverter.toTableName,
        SqlNamingConverter.toSnakeCase
      );
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
    console.log('✅ Core schema: _collections table created');
  }
}
