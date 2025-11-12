/**
 * SqlStorageAdapterBase - SQL-Generic Storage Logic
 *
 * Intermediate abstract base class for SQL-based storage adapters.
 * Provides SQL-generic schema generation and type mapping.
 * Concrete implementations (SqliteStorageAdapter, PostgresStorageAdapter) extend this.
 */

import { DataStorageAdapter } from '../shared/DataStorageAdapter';
import { getFieldMetadata, hasFieldMetadata, type FieldMetadata, type FieldType } from '../../../system/data/decorators/FieldDecorators';

/**
 * SQL dialect identifier
 */
export type SqlDialect = 'sqlite' | 'postgres' | 'mysql';

/**
 * SQL value types (for parameterized queries)
 */
export type SqlValue = string | number | boolean | null;

/**
 * Entity constructor function type
 */
type EntityConstructor = new (...args: unknown[]) => unknown;

/**
 * Abstract base class for SQL storage adapters
 */
export abstract class SqlStorageAdapterBase extends DataStorageAdapter {
  /**
   * Entity registry for mapping collections to entity classes
   */
  protected entityRegistry = new Map<string, EntityConstructor>();

  /**
   * Get the SQL dialect for this adapter (sqlite, postgres, mysql)
   */
  protected abstract getSqlDialect(): SqlDialect;

  /**
   * Execute a raw SQL query and return rows
   */
  protected abstract executeRawSql(sql: string, params?: SqlValue[]): Promise<Record<string, unknown>[]>;

  /**
   * Execute a raw SQL statement (INSERT, UPDATE, DELETE) and return metadata
   */
  protected abstract executeRawStatement(sql: string, params?: SqlValue[]): Promise<{ lastID?: number; changes: number }>;

  /**
   * Map FieldType to SQL column type (dialect-aware)
   */
  protected mapFieldTypeToSql(fieldType: FieldType, options?: FieldMetadata['options']): string {
    const dialect = this.getSqlDialect();

    switch (fieldType) {
      case 'primary':
        return dialect === 'postgres' ? 'TEXT PRIMARY KEY' : 'TEXT PRIMARY KEY';

      case 'foreign_key':
        return 'TEXT' + (options?.nullable ? '' : ' NOT NULL');

      case 'text': {
        const maxLength = options?.maxLength;
        if (dialect === 'postgres' && !maxLength) {
          return 'TEXT';
        }
        return maxLength ? `VARCHAR(${maxLength})` : 'TEXT';
      }

      case 'number':
        return dialect === 'mysql' ? 'DOUBLE' : 'REAL';

      case 'boolean':
        return dialect === 'mysql' ? 'BOOLEAN' : 'INTEGER';

      case 'date':
        return dialect === 'postgres' ? 'TIMESTAMP WITH TIME ZONE' : 'TEXT';

      case 'enum':
        return 'TEXT';

      case 'json':
        if (dialect === 'postgres') return 'JSONB';
        if (dialect === 'mysql') return 'JSON';
        return 'TEXT';

      default:
        return 'TEXT';
    }
  }

  /**
   * Generate CREATE TABLE SQL from entity field metadata
   */
  protected generateCreateTableSql(
    collectionName: string,
    entityClass: EntityConstructor,
    tableNameMapper: (name: string) => string,
    fieldNameMapper: (name: string) => string
  ): string {
    const tableName = tableNameMapper(collectionName);
    const fieldMetadata = getFieldMetadata(entityClass);
    const columns: string[] = [];
    const constraints: string[] = [];

    // Generate column definitions
    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      const columnName = fieldNameMapper(fieldName);
      const columnType = this.mapFieldTypeToSql(metadata.fieldType, metadata.options);
      let columnDef = `${columnName} ${columnType}`;

      // Add NOT NULL constraint (except for primary keys which handle this)
      if (metadata.options?.nullable === false && metadata.fieldType !== 'primary') {
        columnDef += ' NOT NULL';
      }

      // Add UNIQUE constraint
      if (metadata.options?.unique) {
        columnDef += ' UNIQUE';
      }

      // Add DEFAULT value
      if (metadata.options?.default !== undefined) {
        columnDef += ` DEFAULT ${JSON.stringify(metadata.options.default)}`;
      }

      columns.push(columnDef);

      // Add FOREIGN KEY constraints
      if (metadata.fieldType === 'foreign_key' && metadata.options?.references) {
        const ref = metadata.options.references;
        const [refTable, refColumn] = ref.split('.');
        if (refTable && refColumn) {
          const refTableName = tableNameMapper(refTable);
          const refColumnName = fieldNameMapper(refColumn);
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
   * Generate CREATE INDEX SQL statements from entity field metadata
   */
  protected generateCreateIndexSql(
    collectionName: string,
    entityClass: EntityConstructor,
    tableNameMapper: (name: string) => string,
    fieldNameMapper: (name: string) => string
  ): string[] {
    const tableName = tableNameMapper(collectionName);
    const fieldMetadata = getFieldMetadata(entityClass);
    const indexes: string[] = [];

    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      if (metadata.options?.index) {
        const columnName = fieldNameMapper(fieldName);
        const indexName = `idx_${tableName}_${columnName}`;
        indexes.push(`CREATE INDEX IF NOT EXISTS ${indexName} ON ${tableName}(${columnName})`);
      }
    }

    return indexes;
  }

  /**
   * Register an entity class for a collection
   */
  protected registerEntity(collectionName: string, entityClass: EntityConstructor): void {
    this.entityRegistry.set(collectionName, entityClass);
  }

  /**
   * Check if entity has field metadata
   */
  protected hasEntityMetadata(collectionName: string): boolean {
    const entityClass = this.entityRegistry.get(collectionName);
    return entityClass ? hasFieldMetadata(entityClass) : false;
  }
}
