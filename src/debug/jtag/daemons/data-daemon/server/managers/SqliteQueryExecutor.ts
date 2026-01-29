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
  QueryExplanation,
  CollectionSchema
} from '../../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';
import type { SqlExecutor } from '../SqlExecutor';
import { SqliteQueryBuilder } from '../SqliteQueryBuilder';
import { Logger } from '../../../../system/core/logging/Logger';

const log = Logger.create('SqliteQueryExecutor', 'sql');

/**
 * Schema getter function type - provided by SqliteStorageAdapter
 */
export type SchemaGetter = (collection: string) => CollectionSchema | undefined;

/**
 * SqliteQueryExecutor - Manages read and query operations
 *
 * ARCHITECTURE: Uses schema from SchemaManager cache instead of ENTITY_REGISTRY.
 * The schema getter is injected from SqliteStorageAdapter.
 */
export class SqliteQueryExecutor {
  private getSchema: SchemaGetter | null = null;

  constructor(
    private executor: SqlExecutor
  ) {}

  /**
   * Set the schema getter function (injected from SqliteStorageAdapter)
   */
  setSchemaGetter(getter: SchemaGetter): void {
    this.getSchema = getter;
  }

  /**
   * Read a single record by ID
   *
   * ARCHITECTURE: Uses schema from SchemaManager cache. Schema MUST be cached
   * via ensureSchema() before any read/write operations.
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const schema = this.getSchema?.(collection);
      if (!schema) {
        // Schema must be cached by ensureSchema() before any operations
        return {
          success: false,
          error: `No schema cached for collection "${collection}". This indicates ensureSchema() was not called or failed.`
        };
      }
      return await this.readFromSchema<T>(collection, id, schema);

    } catch (error: any) {
      log.error(`Read failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Read record using schema
   *
   * ARCHITECTURE: Uses CollectionSchema passed from daemon instead of
   * looking up entity class from ENTITY_REGISTRY.
   */
  private async readFromSchema<T extends RecordData>(
    collection: string,
    id: UUID,
    schema: CollectionSchema
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const sql = `SELECT * FROM ${tableName} WHERE id = ? LIMIT 1`;
    const rows = await this.executor.runSql(sql, [id]);

    log.debug(`[SCHEMA-PATH] SELECT FROM ${tableName} WHERE id = ${id}`);

    if (rows.length === 0) {
      return {
        success: false,
        error: `Record not found: ${collection}/${id}`
      };
    }

    const row = rows[0];
    // Build entity data with id - uses Record for assignment, cast to T at return
    const entityData: Record<string, unknown> = {
      // CRITICAL: id must be in entityData - BaseEntity requires it
      id: row.id
    };

    // Process fields from schema
    for (const field of schema.fields) {
      // Skip metadata fields (handled in DataRecord.metadata) but NOT id
      // id is part of BaseEntity and MUST be in entityData
      if (['createdAt', 'updatedAt', 'version'].includes(field.name)) {
        continue;
      }
      // id already set above, skip from schema processing
      if (field.name === 'id') {
        continue;
      }

      const columnName = SqlNamingConverter.toSnakeCase(field.name);
      let value = row[columnName];

      if (value !== undefined && value !== null) {
        // Convert SQL value based on schema type
        switch (field.type) {
          case 'boolean':
            value = value === 1;
            break;
          case 'json':
            if (typeof value === 'string') {
              try {
                value = JSON.parse(value);
              } catch (e) {
                console.error(`❌ JSON.parse failed for ${collection}.${field.name} (row ${row.id}): ${(e as Error).message}. Raw value: "${String(value).substring(0, 100)}"`);
                throw e;
              }
            }
            break;
          case 'date':
            value = new Date(value);
            break;
        }
        entityData[field.name] = value;
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
   * Query records with complex filters
   *
   * ARCHITECTURE: Uses schema from SchemaManager cache. Schema MUST be cached
   * via ensureSchema() before any read/write operations.
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      log.debug(`Querying ${query.collection}`, query);

      const schema = this.getSchema?.(query.collection);
      if (!schema) {
        // Schema must be cached by ensureSchema() before any operations
        return {
          success: false,
          error: `No schema cached for collection "${query.collection}". This indicates ensureSchema() was not called or failed.`
        };
      }
      return await this.queryFromSchema<T>(query, schema);

    } catch (error: any) {
      log.error(`Query failed for ${query.collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Query records using schema
   */
  private async queryFromSchema<T extends RecordData>(
    query: StorageQuery,
    schema: CollectionSchema
  ): Promise<StorageResult<DataRecord<T>[]>> {
    const { sql, params } = this.buildSchemaSelectQuery(query, schema);
    const rows = await this.executor.runSql(sql, params);

    log.debug(`[SCHEMA-PATH] Query ${query.collection} returned ${rows.length} rows`);

    const records: DataRecord<T>[] = rows.map(row => {
      // Build entity data with id - uses Record for assignment, cast to T at return
      const entityData: Record<string, unknown> = {
        // CRITICAL: id must be in entityData - BaseEntity requires it
        id: row.id
      };

      // Process fields from schema
      for (const field of schema.fields) {
        // Skip metadata fields (handled in DataRecord.metadata) but NOT id
        // id is part of BaseEntity and MUST be in entityData
        if (['createdAt', 'updatedAt', 'version'].includes(field.name)) {
          continue;
        }
        // id already set above, skip from schema processing
        if (field.name === 'id') {
          continue;
        }

        const columnName = SqlNamingConverter.toSnakeCase(field.name);
        let value = row[columnName];

        if (value !== undefined && value !== null) {
          // Convert SQL value based on schema type
          switch (field.type) {
            case 'boolean':
              value = value === 1;
              break;
            case 'json':
              if (typeof value === 'string') {
                try {
                  value = JSON.parse(value);
                } catch (e) {
                  // Log the exact collection/field for debugging corrupted json data
                  console.error(`❌ JSON.parse failed for ${query.collection}.${field.name} (row ${row.id}): ${(e as Error).message}. Raw value: "${String(value).substring(0, 100)}"`);
                  throw e;
                }
              }
              break;
            case 'date':
              value = new Date(value);
              break;
          }
          entityData[field.name] = value;
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
   * Build SELECT SQL query using schema (NEW ARCHITECTURE)
   *
   * This method builds the same SQL as buildEntitySelectQuery but uses
   * CollectionSchema instead of EntityConstructor/FieldMetadata.
   */
  private buildSchemaSelectQuery(query: StorageQuery, _schema: CollectionSchema): { sql: string; params: any[] } {
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
          sql += ` AND ${timeFilters.join(' AND ')}`;
        } else {
          sql += ` WHERE ${timeFilters.join(' AND ')}`;
        }
      }
    }

    // Add cursor-based pagination filter
    if (query.cursor) {
      const cursorColumn = SqlNamingConverter.toSnakeCase(query.cursor.field);
      const operator = query.cursor.direction === 'after' ? '>' : '<';
      const cursorCondition = `${cursorColumn} ${operator} ?`;

      if (whereClauses.length > 0 || query.timeRange) {
        sql += ` AND ${cursorCondition}`;
      } else {
        sql += ` WHERE ${cursorCondition}`;
      }
      params.push(query.cursor.value);
    }

    // Add sorting
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

  /**
   * Count records matching query filters using SQL COUNT(*)
   * CRITICAL: Uses SQL aggregation, NOT fetching all rows!
   */
  async count(query: StorageQuery): Promise<StorageResult<number>> {
    try {
      const tableName = SqlNamingConverter.toTableName(query.collection);
      const params: any[] = [];
      let sql = `SELECT COUNT(*) as count FROM ${tableName}`;

      // Build WHERE clause from filters (same logic as buildSchemaSelectQuery)
      const whereClauses: string[] = [];

      if (query.filter) {
        for (const [field, filter] of Object.entries(query.filter)) {
          const columnName = SqlNamingConverter.toSnakeCase(field);

          if (typeof filter === 'object' && filter !== null && !Array.isArray(filter)) {
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
                case '$contains':
                  whereClauses.push(`${columnName} LIKE ?`);
                  params.push(`%${value}%`);
                  break;
              }
            }
          } else {
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
            sql += ` AND ${timeFilters.join(' AND ')}`;
          } else {
            sql += ` WHERE ${timeFilters.join(' AND ')}`;
          }
        }
      }

      const result = await this.executor.runSql(sql, params);
      const count = result[0]?.count ?? 0;

      log.debug(`[COUNT] ${query.collection}: ${count} records`);

      return { success: true, data: count };
    } catch (error: any) {
      log.error(`Count failed for ${query.collection}:`, error.message);
      return { success: false, error: error.message };
    }
  }
}
