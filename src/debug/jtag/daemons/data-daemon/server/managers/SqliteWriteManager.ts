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
  StorageResult
} from '../../shared/DataStorageAdapter';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';
import { SqliteRawExecutor } from '../SqliteRawExecutor';
import {
  getFieldMetadata,
  hasFieldMetadata
} from '../../../../system/data/decorators/FieldDecorators';
import { ENTITY_REGISTRY, type EntityConstructor } from '../EntityRegistry';
import { Logger } from '../../../../system/core/logging/Logger';

const log = Logger.create('SqliteWriteManager', 'sql');

/**
 * SqliteWriteManager - Manages create, update, and delete operations
 */
export class SqliteWriteManager {
  constructor(
    private executor: SqliteRawExecutor
  ) {}

  /**
   * Create a record with proper relational schema
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

      const entityClass = ENTITY_REGISTRY.get(collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Create in entity-specific table
        return await this.createInEntityTable<T>(record, entityClass);
      } else {
        // Create in simple entity table
        return await this.createInSimpleEntityTable<T>(record);
      }

    } catch (error: any) {
      log.error(`Create failed for ${collection}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Create record in entity-specific table
   */
  private async createInEntityTable<T extends RecordData>(
    record: DataRecord<T>,
    entityClass: EntityConstructor
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(record.collection);
    const fieldMetadata = getFieldMetadata(entityClass);

    const columns: string[] = [];
    const values: any[] = [];
    const placeholders: string[] = [];

    // Process ALL fields uniformly using decorator metadata
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

    await this.executor.runStatement(sql, values);

    log.debug(`Inserted into entity table ${tableName} with ${columns.length} columns`);

    return {
      success: true,
      data: record
    };
  }

  /**
   * Create record in simple entity table (for unregistered entities)
   */
  private async createInSimpleEntityTable<T extends RecordData>(
    record: DataRecord<T>
  ): Promise<StorageResult<DataRecord<T>>> {
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

    await this.executor.runStatement(sql, params);
    log.debug(`Inserted into simple entity table ${tableName}`);

    return {
      success: true,
      data: record
    };
  }

  /**
   * Update an existing record
   */
  async update<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    version?: number
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      log.debug(`Updating ${collection}/${id}`);

      const entityClass = ENTITY_REGISTRY.get(collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Update in entity-specific table
        log.debug(`Using entity-specific table for ${collection}`);
        return await this.updateInEntityTable<T>(collection, id, data, version, entityClass);
      } else {
        // Update in simple entity table
        log.debug(`Using simple entity table for ${collection}`);
        return await this.updateInSimpleEntityTable<T>(collection, id, data, version);
      }

    } catch (error: any) {
      log.error(`Update failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Update record in entity-specific table
   */
  private async updateInEntityTable<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    version: number | undefined,
    entityClass: EntityConstructor
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const fieldMetadata = getFieldMetadata(entityClass);

    const setColumns: string[] = [];
    const params: any[] = [];

    // Always update base entity fields
    setColumns.push('updated_at = ?', 'version = ?');
    const newVersion = version !== undefined ? version : 1;
    params.push(new Date().toISOString(), newVersion);

    // Update each decorated field based on its type
    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      if (fieldName === 'id') continue; // Skip primary key

      const columnName = SqlNamingConverter.toSnakeCase(fieldName);
      const value = (data as any)[fieldName];

      if (value !== undefined) {
        setColumns.push(`${columnName} = ?`);

        if (metadata.fieldType === 'json') {
          params.push(JSON.stringify(value));
        } else if (metadata.fieldType === 'date' && value instanceof Date) {
          params.push(value.toISOString());
        } else if (metadata.fieldType === 'date' && typeof value === 'string') {
          params.push(value);
        } else {
          params.push(value);
        }
      }
    }

    const sql = `UPDATE ${tableName} SET ${setColumns.join(', ')} WHERE id = ?`;
    params.push(id);

    log.debugIf(() => ['UPDATE SQL', { sql, paramCount: params.length }]);
    const result = await this.executor.runStatement(sql, params);
    log.debugIf(() => ['UPDATE result', result]);

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

    log.debug(`Updated record ${id} in entity table ${tableName}`);

    return {
      success: true,
      data: updatedRecord
    };
  }

  /**
   * Update record in simple entity table
   */
  private async updateInSimpleEntityTable<T extends RecordData>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    version: number | undefined
  ): Promise<StorageResult<DataRecord<T>>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const newVersion = version !== undefined ? version : 1;

    const sql = `UPDATE ${tableName} SET data = ?, updated_at = ?, version = ? WHERE id = ?`;
    const params = [
      JSON.stringify(data),
      new Date().toISOString(),
      newVersion,
      id
    ];

    log.debugIf(() => ['UPDATE SIMPLE SQL', { sql, params }]);
    const result = await this.executor.runStatement(sql, params);
    log.debugIf(() => ['UPDATE SIMPLE result', result]);

    if (result.changes === 0) {
      return {
        success: false,
        error: `No rows updated in ${tableName} for id: ${id}`
      };
    }

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

    log.debug(`Updated record ${id} in simple entity table ${tableName}`);

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
      const entityClass = ENTITY_REGISTRY.get(collection);

      if (entityClass && hasFieldMetadata(entityClass)) {
        // Delete from entity-specific table
        return await this.deleteFromEntityTable(collection, id);
      } else {
        // Delete from simple entity table
        return await this.deleteFromSimpleEntityTable(collection, id);
      }

    } catch (error: any) {
      log.error(`Delete failed for ${collection}/${id}:`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Delete from entity-specific table
   */
  private async deleteFromEntityTable(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const sql = `DELETE FROM ${tableName} WHERE id = ?`;
    const result = await this.executor.runStatement(sql, [id]);

    if (result.changes === 0) {
      return {
        success: true,
        data: false  // Record didn't exist
      };
    }

    log.debug(`Deleted record ${id} from entity table ${tableName}`);

    return {
      success: true,
      data: true
    };
  }

  /**
   * Delete from simple entity table
   */
  private async deleteFromSimpleEntityTable(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    const tableName = SqlNamingConverter.toTableName(collection);
    const sql = `DELETE FROM ${tableName} WHERE id = ?`;
    const result = await this.executor.runStatement(sql, [id]);

    if (result.changes === 0) {
      return {
        success: true,
        data: false  // Record didn't exist
      };
    }

    log.debug(`Deleted record ${id} from simple entity table ${tableName}`);

    return {
      success: true,
      data: true
    };
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
