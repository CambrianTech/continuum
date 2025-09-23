/**
 * JSON File Storage Adapter - File-based Storage Backend
 *
 * Implements universal query operators for JSON files
 * Perfect for development, testing, and small deployments
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type CollectionStats,
  type StorageOperation,
  type RecordData,
  type FieldFilter,
  type QueryOperators
} from '../shared/DataStorageAdapter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Query operator constants
 */
export const QUERY_OPERATORS = {
  EQUAL: '$eq',
  NOT_EQUAL: '$ne',
  GREATER_THAN: '$gt',
  GREATER_THAN_OR_EQUAL: '$gte',
  LESS_THAN: '$lt',
  LESS_THAN_OR_EQUAL: '$lte',
  IN: '$in',
  NOT_IN: '$nin',
  EXISTS: '$exists',
  REGEX: '$regex',
  CONTAINS: '$contains'
} as const;

export type QueryOperatorType = typeof QUERY_OPERATORS[keyof typeof QUERY_OPERATORS];

/**
 * File operation constants
 */
export const FILE_CONSTANTS = {
  JSON_EXTENSION: '.json',
  BACKUP_EXTENSION: '.backup',
  DEFAULT_DATA_DIR: '.continuum',
  ENCODING: 'utf-8' as const
} as const;

/**
 * Base entity field names
 */
export const BASE_ENTITY_FIELDS = {
  ID: 'id',
  COLLECTION: 'collection',
  CREATED_AT: 'createdAt',
  UPDATED_AT: 'updatedAt',
  VERSION: 'version'
} as const;

export type BaseEntityField = typeof BASE_ENTITY_FIELDS[keyof typeof BASE_ENTITY_FIELDS];

/**
 * Type for primitive values that can be compared
 */
export type ComparableValue = string | number | boolean | Date;

/**
 * Type for filter values
 */
export type FilterValue = ComparableValue | ComparableValue[] | null | undefined;

/**
 * Type for record field values
 */
export type RecordFieldValue = FilterValue | Record<string, FilterValue>;

/**
 * JSON File Configuration Options
 */
interface JsonFileOptions {
  dataDirectory?: string;     // Directory for JSON files
  prettyPrint?: boolean;     // Format JSON files for readability
  backupOnWrite?: boolean;   // Create .backup files
  syncWrites?: boolean;      // Use synchronous writes for consistency
}

/**
 * In-memory collection cache for performance
 */
interface CollectionCache<T extends RecordData = RecordData> {
  data: DataRecord<T>[];
  lastModified: number;
  dirty: boolean;
}

/**
 * JSON File Storage Adapter
 *
 * Each collection is stored as a separate JSON file:
 * - ChatMessage.json contains all chat messages
 * - User.json contains all users
 * - etc.
 */
export class JsonFileStorageAdapter extends DataStorageAdapter {
  private config!: StorageAdapterConfig;
  private options: JsonFileOptions = {};
  private dataDirectory!: string;
  private cache = new Map<string, CollectionCache>();
  private isInitialized = false;

  async initialize(config: StorageAdapterConfig): Promise<void> {
    this.config = config;
    this.options = {
      prettyPrint: true,
      backupOnWrite: false,
      syncWrites: false,
      ...config.options
    };

    // Set up data directory
    this.dataDirectory = this.options.dataDirectory ||
      path.join(process.cwd(), '.continuum', 'data', 'json-storage');

    // Ensure data directory exists
    await fs.mkdir(this.dataDirectory, { recursive: true });

    console.log(`📁 JSON Storage: Initialized at ${this.dataDirectory}`);
    this.isInitialized = true;
  }

  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      const collection = await this.loadCollection<T>(record.collection);

      // Check for duplicate IDs
      const existing = collection.find(r => r.id === record.id);
      if (existing) {
        return {
          success: false,
          error: `Record with ID ${record.id} already exists in ${record.collection}`
        };
      }

      // Add to collection
      collection.push(record);
      await this.saveCollection(record.collection, collection);

      return {
        success: true,
        data: record
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const records = await this.loadCollection<T>(collection);
      const record = records.find(r => r.id === id);

      if (!record) {
        return {
          success: false,
          error: `Record ${id} not found in ${collection}`
        };
      }

      return {
        success: true,
        data: record
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      let records = await this.loadCollection<T>(query.collection);

      // Apply filters using universal query operators
      records = this.applyFilters(records, query);

      // Apply sorting
      if (query.sort && query.sort.length > 0) {
        records = this.applySorting(records, query.sort);
      }

      // Apply cursor pagination
      if (query.cursor) {
        records = this.applyCursor(records, query.cursor);
      }

      // Apply offset
      if (query.offset) {
        records = records.slice(query.offset);
      }

      // Apply limit
      if (query.limit) {
        records = records.slice(0, query.limit);
      }

      return {
        success: true,
        data: records,
        metadata: {
          totalCount: records.length,
          queryTime: 0 // TODO: Add timing
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async update<T extends RecordData>(collection: string, id: UUID, data: Partial<T>, incrementVersion = true): Promise<StorageResult<DataRecord<T>>> {
    try {
      const records = await this.loadCollection<T>(collection);
      const recordIndex = records.findIndex(r => r.id === id);

      if (recordIndex === -1) {
        return {
          success: false,
          error: `Record ${id} not found in ${collection}`
        };
      }

      // Update the record
      const existingRecord = records[recordIndex];
      const updatedRecord: DataRecord<T> = {
        ...existingRecord,
        data: { ...existingRecord.data, ...data },
        metadata: {
          ...existingRecord.metadata,
          updatedAt: new Date().toISOString(),
          version: incrementVersion ? existingRecord.metadata.version + 1 : existingRecord.metadata.version
        }
      };

      records[recordIndex] = updatedRecord;
      await this.saveCollection(collection, records);

      return {
        success: true,
        data: updatedRecord
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    try {
      const records = await this.loadCollection(collection);
      const recordIndex = records.findIndex(r => r.id === id);

      if (recordIndex === -1) {
        return {
          success: false,
          error: `Record ${id} not found in ${collection}`
        };
      }

      records.splice(recordIndex, 1);
      await this.saveCollection(collection, records);

      return {
        success: true,
        data: true
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      const files = await fs.readdir(this.dataDirectory);
      const collections = files
        .filter(file => file.endsWith('.json'))
        .map(file => path.basename(file, '.json'));

      return {
        success: true,
        data: collections
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    try {
      const records = await this.loadCollection(collection);
      const filePath = this.getCollectionPath(collection);

      let fileSize = 0;
      try {
        const stats = await fs.stat(filePath);
        fileSize = stats.size;
      } catch {
        // File doesn't exist yet
      }

      return {
        success: true,
        data: {
          name: collection,
          recordCount: records.length,
          totalSize: fileSize,
          indices: [], // JSON files don't have indexes
          lastModified: new Date().toISOString()
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async batch<T extends RecordData = RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    const results: unknown[] = [];

    try {
      for (const op of operations) {
        switch (op.type) {
          case 'create': {
            if (!op.data) {
              results.push({ success: false, error: 'Create operation missing data' });
              break;
            }
            // Create DataRecord from operation data
            const record: DataRecord<T> = {
              id: op.id ?? generateUUID(),
              collection: op.collection,
              data: op.data as T,
              metadata: {
                createdAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                version: 1
              }
            };
            const createResult = await this.create(record);
            results.push(createResult);
            break;
          }
          case 'update': {
            if (!op.id || !op.data) {
              results.push({ success: false, error: 'Update operation missing id or data' });
              break;
            }
            const updateResult = await this.update(op.collection, op.id, op.data, true);
            results.push(updateResult);
            break;
          }
          case 'delete': {
            if (!op.id) {
              results.push({ success: false, error: 'Delete operation missing id' });
              break;
            }
            const deleteResult = await this.delete(op.collection, op.id);
            results.push(deleteResult);
            break;
          }
        }
      }

      return {
        success: true,
        data: results
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown batch operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async clear(): Promise<StorageResult<boolean>> {
    try {
      const files = await fs.readdir(this.dataDirectory);
      const jsonFiles = files.filter(file => file.endsWith('.json'));

      for (const file of jsonFiles) {
        const filePath = path.join(this.dataDirectory, file);
        await fs.unlink(filePath);
      }

      this.cache.clear();

      return {
        success: true,
        data: true
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    try {
      const files = await fs.readdir(this.dataDirectory);
      const jsonFiles = files.filter(file => file.endsWith('.json'));
      let totalRecords = 0;

      // Count records before clearing
      for (const file of jsonFiles) {
        const collection = path.basename(file, '.json');
        const records = await this.loadCollection(collection);
        totalRecords += records.length;
      }

      // Clear all files
      const clearResult = await this.clear();
      if (!clearResult.success) {
        return {
          success: false,
          error: clearResult.error ?? 'Failed to clear collections'
        };
      }

      return {
        success: true,
        data: {
          tablesCleared: jsonFiles.map(file => path.basename(file, '.json')),
          recordsDeleted: totalRecords
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async truncate(collection: string): Promise<StorageResult<boolean>> {
    try {
      await this.saveCollection(collection, []);
      return {
        success: true,
        data: true
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown create operation error';
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  async cleanup(): Promise<void> {
    // Write any dirty cache entries to disk
    for (const [collection, cache] of this.cache.entries()) {
      if (cache.dirty) {
        await this.saveCollection(collection, cache.data);
        cache.dirty = false;
      }
    }
  }

  async close(): Promise<void> {
    await this.cleanup();
    this.cache.clear();
    this.isInitialized = false;
  }

  /**
   * Load collection from JSON file or cache
   */
  private async loadCollection<T extends RecordData>(collection: string): Promise<DataRecord<T>[]> {
    const cached = this.cache.get(collection);
    if (cached) {
      return cached.data as DataRecord<T>[];
    }

    const filePath = this.getCollectionPath(collection);

    try {
      const content = await fs.readFile(filePath, 'utf-8');
      const data = JSON.parse(content) as DataRecord<T>[];

      // Cache the data
      this.cache.set(collection, {
        data: data as DataRecord[],
        lastModified: Date.now(),
        dirty: false
      });

      return data;
    } catch (error: unknown) {
      const fsError = error as { code?: string };
      if (fsError.code === 'ENOENT') {
        // File doesn't exist, return empty array
        const emptyData: DataRecord<T>[] = [];
        this.cache.set(collection, {
          data: emptyData as DataRecord[],
          lastModified: Date.now(),
          dirty: false
        });
        return emptyData;
      }
      throw error;
    }
  }

  /**
   * Save collection to JSON file and update cache
   */
  private async saveCollection<T extends RecordData>(collection: string, data: DataRecord<T>[]): Promise<void> {
    const filePath = this.getCollectionPath(collection);

    // Create backup if requested
    if (this.options.backupOnWrite) {
      try {
        await fs.copyFile(filePath, `${filePath}.backup`);
      } catch {
        // Backup failed, but continue with save
      }
    }

    const content = this.options.prettyPrint ?
      JSON.stringify(data, null, 2) :
      JSON.stringify(data);

    if (this.options.syncWrites) {
      // Synchronous write for consistency
      await fs.writeFile(filePath, content, 'utf-8');
    } else {
      // Asynchronous write for performance
      await fs.writeFile(filePath, content, 'utf-8');
    }

    // Update cache
    this.cache.set(collection, {
      data: data as DataRecord[],
      lastModified: Date.now(),
      dirty: false
    });
  }

  /**
   * Get file path for collection
   */
  private getCollectionPath(collection: string): string {
    return path.join(this.dataDirectory, `${collection}.json`);
  }

  /**
   * Apply universal filters to records array
   */
  private applyFilters<T extends RecordData>(records: DataRecord<T>[], query: StorageQuery): DataRecord<T>[] {
    // Legacy filters (backward compatibility)
    if (query.filters) {
      records = records.filter(record => {
        for (const [field, value] of Object.entries(query.filters!)) {
          const recordValue = this.getFieldValue(record, field);
          if (recordValue !== value) return false;
        }
        return true;
      });
    }

    // Universal filters with operators
    if (query.filter) {
      records = records.filter(record => {
        for (const [field, filter] of Object.entries(query.filter!)) {
          if (!this.matchesFilter(record, field, filter)) {
            return false;
          }
        }
        return true;
      });
    }

    // Time range filters
    if (query.timeRange) {
      records = records.filter(record => {
        const createdAt = new Date(record.metadata.createdAt).getTime();
        if (query.timeRange!.start && createdAt < new Date(query.timeRange!.start).getTime()) {
          return false;
        }
        if (query.timeRange!.end && createdAt > new Date(query.timeRange!.end).getTime()) {
          return false;
        }
        return true;
      });
    }

    return records;
  }

  /**
   * Check if record matches a filter condition
   */
  private matchesFilter<T extends RecordData>(record: DataRecord<T>, field: string, filter: FieldFilter): boolean {
    const recordValue = this.getFieldValue(record, field);

    if (this.isOperatorObject(filter)) {
      // Handle operators
      for (const [operator, value] of Object.entries(filter)) {
        if (!this.evaluateOperator(recordValue, operator as QueryOperatorType, value)) {
          return false;
        }
      }
      return true;
    } else {
      // Direct value implies $eq
      return this.compareValues(recordValue, filter, QUERY_OPERATORS.EQUAL);
    }
  }

  /**
   * Check if filter is an operator object
   */
  private isOperatorObject(filter: FieldFilter): filter is QueryOperators {
    return typeof filter === 'object' && filter !== null && !Array.isArray(filter);
  }

  /**
   * Evaluate a single operator condition
   */
  private evaluateOperator(recordValue: RecordFieldValue, operator: QueryOperatorType, value: FilterValue): boolean {
    // Handle null/undefined values early
    if (recordValue === null || recordValue === undefined) {
      switch (operator) {
        case QUERY_OPERATORS.EXISTS:
          return value === false;
        case QUERY_OPERATORS.EQUAL:
          return value === recordValue;
        case QUERY_OPERATORS.NOT_EQUAL:
          return value !== recordValue;
        default:
          return false;
      }
    }

    switch (operator) {
      case QUERY_OPERATORS.EQUAL:
        return this.compareValues(recordValue, value as FilterValue, QUERY_OPERATORS.EQUAL);
      case QUERY_OPERATORS.NOT_EQUAL:
        return !this.compareValues(recordValue, value as FilterValue, QUERY_OPERATORS.EQUAL);
      case QUERY_OPERATORS.GREATER_THAN:
        if (value === null || value === undefined) return false;
        return this.compareValues(recordValue, value as FilterValue, QUERY_OPERATORS.GREATER_THAN);
      case QUERY_OPERATORS.GREATER_THAN_OR_EQUAL:
        if (value === null || value === undefined) return false;
        return this.compareValues(recordValue, value as FilterValue, QUERY_OPERATORS.GREATER_THAN_OR_EQUAL);
      case QUERY_OPERATORS.LESS_THAN:
        if (value === null || value === undefined) return false;
        return this.compareValues(recordValue, value as FilterValue, QUERY_OPERATORS.LESS_THAN);
      case QUERY_OPERATORS.LESS_THAN_OR_EQUAL:
        if (value === null || value === undefined) return false;
        return this.compareValues(recordValue, value as FilterValue, QUERY_OPERATORS.LESS_THAN_OR_EQUAL);
      case QUERY_OPERATORS.IN:
        return Array.isArray(value) && value.some(v => v === recordValue);
      case QUERY_OPERATORS.NOT_IN:
        return !Array.isArray(value) || !value.some(v => v === recordValue);
      case QUERY_OPERATORS.EXISTS: {
        const exists = recordValue !== undefined && recordValue !== null;
        return value === exists;
      }
      case QUERY_OPERATORS.REGEX:
        return this.matchesRegex(recordValue, value);
      case QUERY_OPERATORS.CONTAINS:
        return this.containsString(recordValue, value);
      default:
        return false;
    }
  }

  /**
   * Compare two values with specific operator
   */
  private compareValues(recordValue: RecordFieldValue, filterValue: FilterValue, operator: QueryOperatorType): boolean {
    // Handle null/undefined values
    if (recordValue === null || recordValue === undefined || filterValue === null || filterValue === undefined) {
      return operator === QUERY_OPERATORS.EQUAL ? recordValue === filterValue : false;
    }

    // Handle array values (shouldn't be used for comparison operators)
    if (Array.isArray(filterValue)) {
      return operator === QUERY_OPERATORS.EQUAL ? false : false;
    }

    switch (operator) {
      case QUERY_OPERATORS.EQUAL:
        return recordValue === filterValue;
      case QUERY_OPERATORS.GREATER_THAN:
        return (recordValue as ComparableValue) > (filterValue as ComparableValue);
      case QUERY_OPERATORS.GREATER_THAN_OR_EQUAL:
        return (recordValue as ComparableValue) >= (filterValue as ComparableValue);
      case QUERY_OPERATORS.LESS_THAN:
        return (recordValue as ComparableValue) < (filterValue as ComparableValue);
      case QUERY_OPERATORS.LESS_THAN_OR_EQUAL:
        return (recordValue as ComparableValue) <= (filterValue as ComparableValue);
      default:
        return false;
    }
  }

  /**
   * Check if value matches regex
   */
  private matchesRegex(recordValue: RecordFieldValue, pattern: FilterValue): boolean {
    if (typeof recordValue !== 'string' || typeof pattern !== 'string') {
      return false;
    }
    return new RegExp(pattern).test(recordValue);
  }

  /**
   * Check if value contains string (case insensitive)
   */
  private containsString(recordValue: RecordFieldValue, searchValue: FilterValue): boolean {
    if (typeof recordValue !== 'string' || typeof searchValue !== 'string') {
      return false;
    }
    return recordValue.toLowerCase().includes(searchValue.toLowerCase());
  }

  /**
   * Get field value from record (supports nested paths)
   */
  private getFieldValue<T extends RecordData>(record: DataRecord<T>, field: string): RecordFieldValue {
    if (field.includes('.')) {
      return this.getNestedFieldValue(record.data, field.split('.'));
    } else {
      // Check BaseEntity fields first
      switch (field) {
        case BASE_ENTITY_FIELDS.ID:
          return record.id;
        case BASE_ENTITY_FIELDS.COLLECTION:
          return record.collection;
        case BASE_ENTITY_FIELDS.CREATED_AT:
          return record.metadata.createdAt;
        case BASE_ENTITY_FIELDS.UPDATED_AT:
          return record.metadata.updatedAt;
        case BASE_ENTITY_FIELDS.VERSION:
          return record.metadata.version;
        default:
          // Check data fields
          return this.getDataFieldValue(record.data, field);
      }
    }
  }

  /**
   * Get nested field value from object
   */
  private getNestedFieldValue(obj: RecordData, parts: string[]): RecordFieldValue {
    let value: RecordFieldValue | RecordData = obj;
    for (const part of parts) {
      if (value === null || value === undefined || typeof value !== 'object') {
        return undefined;
      }
      value = (value as RecordData)[part] as RecordFieldValue;
    }
    return value as RecordFieldValue;
  }

  /**
   * Get data field value safely
   */
  private getDataFieldValue(data: RecordData, field: string): RecordFieldValue {
    return data[field] as RecordFieldValue;
  }

  /**
   * Apply sorting to records
   */
  private applySorting<T extends RecordData>(records: DataRecord<T>[], sortSpecs: { field: string; direction: 'asc' | 'desc' }[]): DataRecord<T>[] {
    return records.sort((a, b) => {
      for (const { field, direction } of sortSpecs) {
        const aVal = this.getFieldValue(a, field);
        const bVal = this.getFieldValue(b, field);

        // Handle null/undefined values
        if (aVal === null || aVal === undefined) {
          return bVal === null || bVal === undefined ? 0 : -1;
        }
        if (bVal === null || bVal === undefined) {
          return 1;
        }

        let comparison = 0;
        if ((aVal as ComparableValue) < (bVal as ComparableValue)) comparison = -1;
        else if ((aVal as ComparableValue) > (bVal as ComparableValue)) comparison = 1;

        if (comparison !== 0) {
          return direction === 'asc' ? comparison : -comparison;
        }
      }
      return 0;
    });
  }

  /**
   * Apply cursor-based pagination
   */
  private applyCursor<T extends RecordData>(records: DataRecord<T>[], cursor: { field: string; value: ComparableValue; direction: 'before' | 'after' }): DataRecord<T>[] {
    return records.filter(record => {
      const fieldValue = this.getFieldValue(record, cursor.field);

      // Skip null/undefined values
      if (fieldValue === null || fieldValue === undefined) {
        return false;
      }

      if (typeof fieldValue !== typeof cursor.value) {
        return false; // Type mismatch
      }

      if (cursor.direction === 'after') {
        return this.compareValues(fieldValue, cursor.value, QUERY_OPERATORS.GREATER_THAN);
      } else {
        return this.compareValues(fieldValue, cursor.value, QUERY_OPERATORS.LESS_THAN);
      }
    });
  }
}