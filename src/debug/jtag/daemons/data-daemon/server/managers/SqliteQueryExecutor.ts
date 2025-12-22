/**
 * SqliteQueryExecutor - Query Operations for SQLite
 *
 * Handles:
 * - Reading single records
 * - Querying multiple records with filters
 * - Building SQL SELECT queries
 * - Query explanation and performance analysis
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type {
  DataRecord,
  RecordData,
  StorageQuery,
  StorageResult,
  QueryExplanation
} from '../../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';
import type { SqlExecutor } from '../SqlExecutor';
import { SqliteQueryBuilder } from '../SqliteQueryBuilder';
import {
  getFieldMetadata,
  hasFieldMetadata
} from '../../../../system/data/decorators/FieldDecorators';
import { ENTITY_REGISTRY, type EntityConstructor } from '../EntityRegistry';
import { Logger } from '../../../../system/core/logging/Logger';

const log = Logger.create('SqliteQueryExecutor', 'sql');

/**
 * SqliteQueryExecutor - Manages read and query operations
 */
export class SqliteQueryExecutor {
  constructor(
    private executor: SqlExecutor
  ) {}

  /**
   * Read a single record by ID
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const entityClass = ENTITY_REGISTRY.get(collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Use entity-specific table
        return await this.readFromEntityTable<T>(collection, id, entityClass);
      } else {
        // Use simple entity table (fallback)
        return await this.readFromSimpleEntityTable<T>(collection, id);
      }

    } catch (error: any) {
      log.error(`Read failed for ${collection}/${id}:`, error.message);
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
    const rows = await this.executor.runSql(sql, [id]);

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
        if (['createdAt', 'updatedAt', 'version'].includes(fieldName)) {
          // BaseEntity metadata fields go to their proper locations
          continue; // We'll handle these separately
        } else {
          // Include ALL entity fields (including id) in the data object
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

    log.info(`TS read() returning record keys: ${JSON.stringify(Object.keys(record))}`);
    log.info(`TS read() record.data keys: ${JSON.stringify(Object.keys(record.data || {}))}`);

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
    const rows = await this.executor.runSql(sql, [id]);

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

    log.info(`TS read(simple) returning record keys: ${JSON.stringify(Object.keys(record))}`);
    log.info(`TS read(simple) record.data keys: ${JSON.stringify(Object.keys(record.data || {}))}`);

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
      log.debug(`Querying ${query.collection}`, query);

      const entityClass = ENTITY_REGISTRY.get(query.collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Use entity-specific table
        return await this.queryFromEntityTable<T>(query, entityClass);
      } else {
        // Use simple entity table (fallback)
        return await this.queryFromSimpleEntityTable<T>(query);
      }

    } catch (error: any) {
      log.error(`Query failed for ${query.collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Count records matching query without fetching data
   *
   * Efficient COUNT(*) query - no SELECT *, no ORDER BY, no LIMIT
   */
  async count(query: StorageQuery): Promise<StorageResult<number>> {
    try {
      log.debug(`Counting ${query.collection}`, query);

      const entityClass = ENTITY_REGISTRY.get(query.collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Use entity-specific table
        return await this.countFromEntityTable(query, entityClass);
      } else {
        // Use simple entity table (fallback)
        return await this.countFromSimpleEntityTable(query);
      }

    } catch (error: any) {
      log.error(`Count failed for ${query.collection}:`, error.message);
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
    const rows = await this.executor.runSql(sql, params);

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

    log.debug(`Entity query returned ${records.length} records`);

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
    const rows = await this.executor.runSql(sql, params);

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

    log.debug(`Simple entity query returned ${records.length} records`);

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
   * Count from entity-specific table with proper column mapping
   */
  private async countFromEntityTable(query: StorageQuery, entityClass: EntityConstructor): Promise<StorageResult<number>> {
    const { sql, params } = this.buildCountQuery(query, entityClass);
    const rows = await this.executor.runSql(sql, params);

    const count = rows.length > 0 ? (rows[0].count as number) : 0;

    log.debug(`Entity count returned ${count} for ${query.collection}`);

    return {
      success: true,
      data: count
    };
  }

  /**
   * Count from simple entity table (JSON data column)
   */
  private async countFromSimpleEntityTable(query: StorageQuery): Promise<StorageResult<number>> {
    const { sql, params } = this.buildSimpleCountQuery(query);
    const rows = await this.executor.runSql(sql, params);

    const count = rows.length > 0 ? (rows[0].count as number) : 0;

    log.debug(`Simple entity count returned ${count} for ${query.collection}`);

    return {
      success: true,
      data: count
    };
  }

  /**
   * Build SELECT SQL query for entity-specific tables
   */
  private buildEntitySelectQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
    const params: any[] = [];
    const tableName = SqlNamingConverter.toTableName(query.collection);
    let sql = `SELECT * FROM ${tableName}`;

    // Build WHERE clause from filters
    const whereClauses: string[] = [];

    // Universal filters with operators
    if (query.filter) {
      for (const [field, filter] of Object.entries(query.filter)) {
        const columnName = SqlNamingConverter.toSnakeCase(field);

        if (typeof filter === 'object' && filter !== null && !Array.isArray(filter)) {
          // Handle operators like { $gt: value, $in: [...] }
          for (const [operator, value] of Object.entries(filter)) {
            switch (operator) {
              case '$eq':
                whereClauses.push(`${columnName} = ?`);
                params.push(value);
                break;
              case '$ne':
                whereClauses.push(`${columnName} != ?`);
                params.push(value);
                break;
              case '$gt':
                whereClauses.push(`${columnName} > ?`);
                params.push(value);
                break;
              case '$gte':
                whereClauses.push(`${columnName} >= ?`);
                params.push(value);
                break;
              case '$lt':
                whereClauses.push(`${columnName} < ?`);
                params.push(value);
                break;
              case '$lte':
                whereClauses.push(`${columnName} <= ?`);
                params.push(value);
                break;
              case '$in':
                if (Array.isArray(value) && value.length > 0) {
                  const placeholders = value.map(() => '?').join(',');
                  whereClauses.push(`${columnName} IN (${placeholders})`);
                  params.push(...value);
                }
                break;
              case '$nin':
                if (Array.isArray(value) && value.length > 0) {
                  const placeholders = value.map(() => '?').join(',');
                  whereClauses.push(`${columnName} NOT IN (${placeholders})`);
                  params.push(...value);
                }
                break;
              case '$exists':
                if (value) {
                  whereClauses.push(`${columnName} IS NOT NULL`);
                } else {
                  whereClauses.push(`${columnName} IS NULL`);
                }
                break;
              case '$regex':
                whereClauses.push(`${columnName} REGEXP ?`);
                params.push(value);
                break;
              case '$contains':
                whereClauses.push(`${columnName} LIKE ?`);
                params.push(`%${value}%`);
                break;
            }
          }
        } else {
          // Direct value implies $eq
          whereClauses.push(`${columnName} = ?`);
          params.push(filter);
        }
      }
    }

    if (whereClauses.length > 0) {
      sql += ` WHERE ${whereClauses.join(' AND ')}`;
    }

    // DEBUG: Log generated SQL and params for range operator debugging
    if (whereClauses.length > 0) {
      log.debug('WHERE clause:', sql.substring(sql.lastIndexOf('WHERE')));
      log.debug('Query params:', params);
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
        sql += ' WHERE ' + timeFilters.join(' AND ');
      }
    }

    // Add cursor-based pagination filter (before sorting)
    if (query.cursor) {
      const cursorColumn = SqlNamingConverter.toSnakeCase(query.cursor.field);
      const operator = query.cursor.direction === 'after' ? '>' : '<';
      const cursorCondition = `${cursorColumn} ${operator} ?`;

      // Add to WHERE clause or create one
      if (whereClauses.length > 0) {
        sql += ` AND ${cursorCondition}`;
      } else {
        sql += ` WHERE ${cursorCondition}`;
      }

      params.push(query.cursor.value);
      log.debug(`Added cursor condition: ${cursorColumn} ${operator} ${query.cursor.value}`);
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
        sql += ' WHERE ' + timeFilters.join(' AND ');
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
   * Build COUNT(*) SQL query for entity-specific tables
   *
   * Uses same WHERE clause as SELECT but:
   * - COUNT(*) instead of SELECT *
   * - No ORDER BY (irrelevant for counting)
   * - No LIMIT/OFFSET (want total count)
   */
  private buildCountQuery(query: StorageQuery, entityClass: EntityConstructor): { sql: string; params: any[] } {
    const params: any[] = [];
    const tableName = SqlNamingConverter.toTableName(query.collection);
    let sql = `SELECT COUNT(*) as count FROM ${tableName}`;

    // Build WHERE clause from filters (same logic as buildEntitySelectQuery)
    const whereClauses: string[] = [];

    // Universal filters with operators
    if (query.filter) {
      for (const [field, filter] of Object.entries(query.filter)) {
        const columnName = SqlNamingConverter.toSnakeCase(field);

        if (typeof filter === 'object' && filter !== null && !Array.isArray(filter)) {
          // Handle operators like { $gt: value, $in: [...] }
          for (const [operator, value] of Object.entries(filter)) {
            switch (operator) {
              case '$eq':
                whereClauses.push(`${columnName} = ?`);
                params.push(value);
                break;
              case '$ne':
                whereClauses.push(`${columnName} != ?`);
                params.push(value);
                break;
              case '$gt':
                whereClauses.push(`${columnName} > ?`);
                params.push(value);
                break;
              case '$gte':
                whereClauses.push(`${columnName} >= ?`);
                params.push(value);
                break;
              case '$lt':
                whereClauses.push(`${columnName} < ?`);
                params.push(value);
                break;
              case '$lte':
                whereClauses.push(`${columnName} <= ?`);
                params.push(value);
                break;
              case '$in':
                if (Array.isArray(value) && value.length > 0) {
                  const placeholders = value.map(() => '?').join(',');
                  whereClauses.push(`${columnName} IN (${placeholders})`);
                  params.push(...value);
                }
                break;
              case '$nin':
                if (Array.isArray(value) && value.length > 0) {
                  const placeholders = value.map(() => '?').join(',');
                  whereClauses.push(`${columnName} NOT IN (${placeholders})`);
                  params.push(...value);
                }
                break;
              case '$exists':
                if (value) {
                  whereClauses.push(`${columnName} IS NOT NULL`);
                } else {
                  whereClauses.push(`${columnName} IS NULL`);
                }
                break;
              case '$regex':
                whereClauses.push(`${columnName} REGEXP ?`);
                params.push(value);
                break;
              case '$contains':
                whereClauses.push(`${columnName} LIKE ?`);
                params.push(`%${value}%`);
                break;
            }
          }
        } else {
          // Direct value implies $eq
          whereClauses.push(`${columnName} = ?`);
          params.push(filter);
        }
      }
    }

    if (whereClauses.length > 0) {
      sql += ` WHERE ${whereClauses.join(' AND ')}`;
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
        if (whereClauses.length > 0) {
          sql += ' AND ' + timeFilters.join(' AND ');
        } else {
          sql += ' WHERE ' + timeFilters.join(' AND ');
        }
      }
    }

    // Add cursor-based pagination filter (before counting)
    if (query.cursor) {
      const cursorColumn = SqlNamingConverter.toSnakeCase(query.cursor.field);
      const operator = query.cursor.direction === 'after' ? '>' : '<';
      const cursorCondition = `${cursorColumn} ${operator} ?`;

      // Add to WHERE clause or create one
      if (whereClauses.length > 0) {
        sql += ` AND ${cursorCondition}`;
      } else {
        sql += ` WHERE ${cursorCondition}`;
      }

      params.push(query.cursor.value);
    }

    // No ORDER BY - irrelevant for counting
    // No LIMIT/OFFSET - want total count

    return { sql, params };
  }

  /**
   * Build COUNT(*) SQL query for simple entity tables
   */
  private buildSimpleCountQuery(query: StorageQuery): { sql: string; params: any[] } {
    const params: any[] = [];
    const tableName = SqlNamingConverter.toTableName(query.collection);
    let sql = `SELECT COUNT(*) as count FROM ${tableName}`;

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
        sql += ' WHERE ' + timeFilters.join(' AND ');
      }
    }

    // No ORDER BY - irrelevant for counting
    // No LIMIT/OFFSET - want total count

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

  /**
   * Explain query execution (dry-run) - shows what SQL would be generated
   * Uses the same query builder as actual execution for true-to-life results
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    try {
      // Apply SQL naming rules to collection name
      const tableName = SqlNamingConverter.toTableName(query.collection);
      const { sql, params, description } = SqliteQueryBuilder.buildSelect(query, tableName);

      // Get SQLite query plan using EXPLAIN QUERY PLAN
      const executionPlan = await this.getSqliteQueryPlan(sql, params);

      // Estimate row count
      const estimatedRows = await this.estimateRowCount(query);

      return {
        query,
        translatedQuery: sql,
        parameters: params,
        estimatedRows,
        executionPlan: `Query Operations:\n${description}\n\nSQLite Execution Plan:\n${executionPlan}`,
        adapterType: 'sqlite',
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown explanation error';
      return {
        query,
        translatedQuery: `-- Error generating SQL: ${errorMessage}`,
        parameters: [],
        estimatedRows: 0,
        executionPlan: `Error: ${errorMessage}`,
        adapterType: 'sqlite',
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Get SQLite query execution plan
   */
  private async getSqliteQueryPlan(sql: string, params: unknown[]): Promise<string> {
    try {
      const planSql = `EXPLAIN QUERY PLAN ${sql}`;
      const plan = await this.executor.runSql(planSql, params);

      return plan.map((row: any) => {
        return `${row.id || 0}|${row.parent || 0}|${row.notused || 0}|${row.detail || 'No details'}`;
      }).join('\n');
    } catch (error) {
      return `Error getting query plan: ${error instanceof Error ? error.message : 'Unknown error'}`;
    }
  }

  /**
   * Estimate row count for query
   */
  private async estimateRowCount(query: StorageQuery): Promise<number> {
    try {
      const tableName = SqlNamingConverter.toTableName(query.collection);

      // Simple count - could be enhanced with more sophisticated estimation
      const result = await this.executor.runSql(`SELECT COUNT(*) as count FROM \`${tableName}\``);
      return result[0]?.count || 0;
    } catch (error) {
      return 0;
    }
  }
}
