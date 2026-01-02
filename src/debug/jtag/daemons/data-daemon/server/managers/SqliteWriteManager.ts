/**
 * SqliteWriteManager - Write Operations for SQLite
 *
 * Handles:
 * - Creating records (single and batch)
 * - Updating records (single and batch)
 * - Deleting records (single and batch)
 * - Entity-specific table operations
 * - Simple entity table operations
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type {
  DataRecord,
  RecordData,
  StorageResult,
  CollectionSchema
} from '../../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';
import type { SqlExecutor } from '../SqlExecutor';
import { Logger } from '../../../../system/core/logging/Logger';

const log = Logger.create('SqliteWriteManager', 'sql');

/**
 * Schema getter function type - provided by SqliteStorageAdapter
 */
export type SchemaGetter = (collection: string) => CollectionSchema | undefined;

/**
 * SqliteWriteManager - Manages create, update, and delete operations
 *
 * ARCHITECTURE: Uses schema from SchemaManager cache instead of ENTITY_REGISTRY.
 * The schema getter is injected from SqliteStorageAdapter.
 */
export class SqliteWriteManager {
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
   * Create a record with proper relational schema
   *
   * ARCHITECTURE: Uses schema from SchemaManager cache. Schema MUST be cached
   * via ensureSchema() before any read/write operations.
   */
  async create<T extends RecordData>(
    collection: string,
    data: T,
    id?: UUID
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      const recordId = id || `${collection}_${Date.now()}_${Math.random()}`;

      const record: DataRecord<T> = {
        id: recordId,
        collection,
        data,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1
        }
      };

      const schema = this.getSchema?.(collection);
      if (!schema) {
        // Schema must be cached by ensureSchema() before any operations
        return {
          success: false,
          error: `No schema cached for collection "${collection}". This indicates ensureSchema() was not called or failed.`
        };
      }
      return await this.createFromSchema<T>(record, schema);

    } catch (error: any) {
      log.error(`Create failed for ${collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Create record using schema (NEW ARCHITECTURE)
   *
   * ARCHITECTURE: Uses CollectionSchema passed from daemon instead of
   * looking up entity class from ENTITY_REGISTRY.
   */
  private async createFromSchema<T extends RecordData>(
    record: DataRecord<T>,
    schema: CollectionSchema
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(record.collection);

    const columns: string[] = [];
    const values: any[] = [];
    const placeholders: string[] = [];

    // Add base entity fields
    columns.push('id');
    values.push(record.id);
    placeholders.push('?');

    columns.push('created_at');
    values.push(record.metadata.createdAt);
    placeholders.push('?');

    columns.push('updated_at');
    values.push(record.metadata.updatedAt);
    placeholders.push('?');

    columns.push('version');
    values.push(record.metadata.version);
    placeholders.push('?');

    // Process fields from schema
    for (const field of schema.fields) {
      // Skip base entity fields (already added above)
      if (['id', 'createdAt', 'updatedAt', 'version'].includes(field.name)) {
        continue;
      }

      const columnName = SqlNamingConverter.toSnakeCase(field.name);
      const fieldValue = (record.data as any)[field.name];

      if (fieldValue !== undefined) {
        columns.push(columnName);
        placeholders.push('?');

        // Convert value based on schema type
        if (field.type === 'json' && typeof fieldValue === 'object') {
          values.push(JSON.stringify(fieldValue));
        } else if (field.type === 'boolean') {
          values.push(fieldValue ? 1 : 0);
        } else if (field.type === 'date') {
          // Convert Date objects to ISO strings for SQLite storage
          values.push(typeof fieldValue === 'string' ? fieldValue : new Date(fieldValue).toISOString());
        } else {
          values.push(fieldValue);
        }
      }
    }

    const sql = `INSERT INTO ${tableName} (${columns.join(', ')}) VALUES (${placeholders.join(', ')})`;
    log.debug(`[SCHEMA-PATH] INSERT INTO ${tableName}`);

    await this.executor.runStatement(sql, values);

    return {
      success: true,
      data: record
    };
  }

  /**
   * Update an existing record
   *
   * ARCHITECTURE: Uses schema from SchemaManager cache. Schema MUST be cached
   * via ensureSchema() before any read/write operations.
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    version?: number
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      log.debug(`Updating ${collection}/${id}`);

      const schema = this.getSchema?.(collection);
      if (!schema) {
        // Schema must be cached by ensureSchema() before any operations
        return {
          success: false,
          error: `No schema cached for collection "${collection}". This indicates ensureSchema() was not called or failed.`
        };
      }
      return await this.updateFromSchema<T>(collection, id, data, version, schema);

    } catch (error: any) {
      log.error(`Update failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Update record using schema (NEW ARCHITECTURE)
   *
   * ARCHITECTURE: Uses CollectionSchema passed from daemon instead of
   * looking up entity class from ENTITY_REGISTRY.
   */
  private async updateFromSchema<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    version: number | undefined,
    schema: CollectionSchema
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);

    const setColumns: string[] = [];
    const params: any[] = [];

    // Always update base entity fields
    setColumns.push('updated_at = ?', 'version = ?');
    const newVersion = version !== undefined ? version : 1;
    params.push(new Date().toISOString(), newVersion);

    // Update each field based on schema
    for (const field of schema.fields) {
      // Skip base entity fields (already handled above) and primary key
      if (['id', 'createdAt', 'updatedAt', 'version'].includes(field.name)) {
        continue;
      }

      const columnName = SqlNamingConverter.toSnakeCase(field.name);
      const value = (data as any)[field.name];

      if (value !== undefined) {
        setColumns.push(`${columnName} = ?`);

        // Convert value based on schema type
        if (field.type === 'json' && typeof value === 'object') {
          params.push(JSON.stringify(value));
        } else if (field.type === 'boolean') {
          params.push(value ? 1 : 0);
        } else if (field.type === 'date') {
          // Convert Date objects to ISO strings for SQLite storage
          params.push(typeof value === 'string' ? value : new Date(value).toISOString());
        } else {
          params.push(value);
        }
      }
    }

    const sql = `UPDATE ${tableName} SET ${setColumns.join(', ')} WHERE id = ?`;
    params.push(id);

    log.debug(`[SCHEMA-PATH] UPDATE ${tableName} WHERE id = ${id}`);
    const result = await this.executor.runStatement(sql, params);

    if (result.changes === 0) {
      return {
        success: false,
        error: `No rows updated in ${tableName} for id: ${id}`
      };
    }

    // Build updated record with merged data
    const updatedRecord: DataRecord<T> = {
      id,
      collection,
      data: data as T,
      metadata: {
        createdAt: new Date().toISOString(), // Note: Ideally we'd preserve original createdAt
        updatedAt: new Date().toISOString(),
        version: newVersion
      }
    };

    return {
      success: true,
      data: updatedRecord
    };
  }

  /**
   * Delete a record
   *
   * ARCHITECTURE: Uses schema from SchemaManager cache. Schema MUST be cached
   * via ensureSchema() before any read/write operations.
   * Note: Delete doesn't need field metadata - just the table name.
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    try {
      const schema = this.getSchema?.(collection);
      if (!schema) {
        // Schema must be cached by ensureSchema() before any operations
        return {
          success: false,
          error: `No schema cached for collection "${collection}". This indicates ensureSchema() was not called or failed.`
        };
      }

      const tableName = SqlNamingConverter.toTableName(collection);
      const sql = `DELETE FROM ${tableName} WHERE id = ?`;
      const result = await this.executor.runStatement(sql, [id]);

      log.debug(`DELETE FROM ${tableName} WHERE id = ${id}`);

      return {
        success: true,
        data: result.changes > 0
      };

    } catch (error: any) {
      log.error(`Delete failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Batch create records
   */
  async batchCreate<T extends RecordData>(
    collection: string,
    records: T[]
  ): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      const results: DataRecord<T>[] = [];

      for (const data of records) {
        const result = await this.create<T>(collection, data);
        if (result.success && result.data) {
          results.push(result.data);
        } else {
          return {
            success: false,
            error: `Batch create failed: ${result.error}`
          };
        }
      }

      log.debug(`Batch created ${results.length} records in ${collection}`);

      return {
        success: true,
        data: results
      };

    } catch (error: any) {
      log.error(`Batch create failed for ${collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Batch update records
   */
  async batchUpdate<T extends RecordData>(
    collection: string,
    updates: Array<{ id: UUID; data: Partial<T>; version?: number }>
  ): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      const results: DataRecord<T>[] = [];

      for (const update of updates) {
        const result = await this.update<T>(collection, update.id, update.data, update.version);
        if (result.success && result.data) {
          results.push(result.data);
        } else {
          return {
            success: false,
            error: `Batch update failed: ${result.error}`
          };
        }
      }

      log.debug(`Batch updated ${results.length} records in ${collection}`);

      return {
        success: true,
        data: results
      };

    } catch (error: any) {
      log.error(`Batch update failed for ${collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Batch delete records
   */
  async batchDelete(
    collection: string,
    ids: UUID[]
  ): Promise<StorageResult<boolean>> {
    try {
      let deletedCount = 0;

      for (const id of ids) {
        const result = await this.delete(collection, id);
        if (result.success && result.data) {
          deletedCount++;
        }
      }

      log.debug(`Batch deleted ${deletedCount}/${ids.length} records from ${collection}`);

      return {
        success: true,
        data: deletedCount > 0
      };

    } catch (error: any) {
      log.error(`Batch delete failed for ${collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }
}
