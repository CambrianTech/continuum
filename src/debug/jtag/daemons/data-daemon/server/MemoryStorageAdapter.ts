/**
 * Memory Storage Adapter - In-Memory Data Persistence
 * 
 * Implements DataStorageAdapter for fast in-memory storage with:
 * - Map-based collection organization
 * - Advanced query support with filtering and sorting
 * - Optional persistence to disk for durability
 * - High-performance operations for temporary data
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type CollectionStats,
  type StorageOperation,
  type RecordData,
  type QueryExplanation
} from '../shared/DataStorageAdapter';

/**
 * Memory Storage Configuration
 */
interface MemoryStorageOptions {
  readonly maxRecords?: number;
  readonly enablePersistence?: boolean;
  readonly persistencePath?: string;
  readonly autoSaveInterval?: number;
}

/**
 * Memory Storage Adapter - High-Performance In-Memory Storage
 */
export class MemoryStorageAdapter extends DataStorageAdapter {
  private collections: Map<string, Map<UUID, DataRecord<any>>> = new Map();
  private namespace: string = '';
  private options: MemoryStorageOptions = {};
  private autoSaveTimer?: NodeJS.Timeout;
  
  /**
   * Initialize memory storage with configuration
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    this.namespace = config.namespace;
    this.options = {
      maxRecords: 10000,
      enablePersistence: false,
      autoSaveInterval: 30000, // 30 seconds
      ...config.options
    } as MemoryStorageOptions;
    
    // Load from persistence if enabled
    if (this.options.enablePersistence && this.options.persistencePath) {
      await this.loadFromDisk();
      
      // Setup auto-save if configured
      if (this.options.autoSaveInterval && this.options.autoSaveInterval > 0) {
        this.autoSaveTimer = setInterval(() => {
          this.saveToDisk().catch(error => {
            console.warn('Memory storage auto-save failed:', error);
          });
        }, this.options.autoSaveInterval);
      }
    }
  }
  
  /**
   * Create record in memory
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      // Check memory limits
      if (this.options.maxRecords && this.getTotalRecordCount() >= this.options.maxRecords) {
        return {
          success: false,
          error: `Memory storage limit reached: ${this.options.maxRecords} records`
        };
      }
      
      // Get or create collection
      if (!this.collections.has(record.collection)) {
        this.collections.set(record.collection, new Map());
      }
      
      const collection = this.collections.get(record.collection)!;
      
      // Check if record already exists
      if (collection.has(record.id)) {
        return {
          success: false,
          error: `Record ${record.id} already exists in collection ${record.collection}`
        };
      }
      
      // Store record
      collection.set(record.id, record);
      
      return {
        success: true,
        data: record
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Read record from memory
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const collectionMap = this.collections.get(collection);
      if (!collectionMap) {
        return {
          success: true,
          data: undefined
        };
      }
      
      const record = collectionMap.get(id) as DataRecord<T>;
      
      return {
        success: true,
        data: record
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Query records with in-memory filtering and sorting
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      const collectionMap = this.collections.get(query.collection);
      if (!collectionMap) {
        return {
          success: true,
          data: [],
          metadata: { totalCount: 0 }
        };
      }
      
      let records = Array.from(collectionMap.values()) as DataRecord<T>[];
      
      // Apply filters
      if (query.filters) {
        records = records.filter(record => this.matchesFilters(record, query.filters));
      }
      
      // Apply time range filter
      if (query.timeRange) {
        records = records.filter(record => {
          const recordTime = record.metadata.createdAt;
          const start = query.timeRange!.start;
          const end = query.timeRange!.end;
          
          if (start && recordTime < start) return false;
          if (end && recordTime > end) return false;
          return true;
        });
      }
      
      // Apply tag filter
      if (query.tags && query.tags.length > 0) {
        records = records.filter(record => {
          const recordTags = record.metadata.tags || [];
          return query.tags!.some(tag => recordTags.includes(tag));
        });
      }
      
      // Apply sorting
      if (query.sort && query.sort.length > 0) {
        records.sort((a, b) => this.compareRecords(a, b, query.sort!));
      }
      
      // Apply pagination
      const totalCount = records.length;
      const startIndex = query.offset || 0;
      const endIndex = query.limit ? startIndex + query.limit : records.length;
      const paginatedRecords = records.slice(startIndex, endIndex);
      
      return {
        success: true,
        data: paginatedRecords,
        metadata: {
          totalCount,
          queryTime: Date.now()
        }
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Update record in memory
   */
  async update<T extends RecordData>(
    collection: string, 
    id: UUID, 
    data: Partial<T>, 
    incrementVersion: boolean = true
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      const collectionMap = this.collections.get(collection);
      if (!collectionMap || !collectionMap.has(id)) {
        return {
          success: false,
          error: 'Record not found'
        };
      }
      
      const existingRecord = collectionMap.get(id) as DataRecord<T>;
      
      // Create updated record
      const updated: DataRecord<T> = {
        ...existingRecord,
        data: { ...existingRecord.data, ...data },
        metadata: {
          ...existingRecord.metadata,
          updatedAt: new Date().toISOString(),
          version: incrementVersion ? existingRecord.metadata.version + 1 : existingRecord.metadata.version
        }
      };
      
      // Update in memory
      collectionMap.set(id, updated);
      
      return {
        success: true,
        data: updated
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Delete record from memory
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    try {
      const collectionMap = this.collections.get(collection);
      if (!collectionMap) {
        return {
          success: true,
          data: false
        };
      }
      
      const deleted = collectionMap.delete(id);
      
      // Remove empty collections
      if (collectionMap.size === 0) {
        this.collections.delete(collection);
      }
      
      return {
        success: true,
        data: deleted
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * List all collections
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      const collections = Array.from(this.collections.keys());
      
      return {
        success: true,
        data: collections
      };
      
    } catch (error: any) {
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
      const collectionMap = this.collections.get(collection);
      if (!collectionMap) {
        return {
          success: true,
          data: {
            name: collection,
            recordCount: 0,
            totalSize: 0,
            lastModified: ''
          }
        };
      }
      
      let totalSize = 0;
      let lastModified = '';
      
      for (const record of collectionMap.values()) {
        // Estimate size as JSON string length
        totalSize += JSON.stringify(record).length;
        
        if (record.metadata.updatedAt > lastModified) {
          lastModified = record.metadata.updatedAt;
        }
      }
      
      const stats: CollectionStats = {
        name: collection,
        recordCount: collectionMap.size,
        totalSize,
        lastModified
      };
      
      return {
        success: true,
        data: stats
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Batch operations - Memory operations are fast
   */
  async batch<T extends RecordData = RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    const results: any[] = [];
    
    try {
      for (const op of operations) {
        let result: StorageResult<any>;
        
        switch (op.type) {
          case 'create':
            result = await this.create({
              id: op.id!,
              collection: op.collection,
              data: op.data as T,
              metadata: {
                createdAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                version: 1
              }
            });
            break;
            
          case 'read':
            result = await this.read(op.collection, op.id!);
            break;
            
          case 'update':
            result = await this.update(op.collection, op.id!, op.data as Partial<T>);
            break;
            
          case 'delete':
            result = await this.delete(op.collection, op.id!);
            break;
            
          default:
            result = {
              success: false,
              error: `Unsupported batch operation: ${op.type}`
            };
        }
        
        results.push(result);
      }
      
      return {
        success: true,
        data: results
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        data: results
      };
    }
  }

  /**
   * Clear all data from all collections
   */
  async clear(): Promise<StorageResult<boolean>> {
    try {
      const collectionCount = this.collections.size;
      let totalRecords = 0;

      for (const collection of this.collections.values()) {
        totalRecords += collection.size;
      }

      this.collections.clear();

      console.log(`🧹 MemoryStorage: All data cleared successfully (${collectionCount} collections, ${totalRecords} records)`);
      return {
        success: true,
        data: true
      };

    } catch (error: any) {
      console.error('❌ MemoryStorage: Error clearing data:', error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Truncate all records from a specific collection
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    try {
      const collectionMap = this.collections.get(collection);

      if (!collectionMap) {
        console.log(`ℹ️ MemoryStorage: Collection '${collection}' doesn't exist, skipping truncate`);
        return {
          success: true,
          data: true
        };
      }

      const recordCount = collectionMap.size;
      this.collections.delete(collection);

      console.log(`🗑️ MemoryStorage: Truncated collection '${collection}' (${recordCount} records)`);
      return {
        success: true,
        data: true
      };

    } catch (error: any) {
      console.error(`❌ MemoryStorage: Error truncating collection '${collection}':`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Cleanup - Clear expired records based on TTL
   */
  async cleanup(): Promise<void> {
    try {
      const now = Date.now();
      
      for (const [collectionName, collectionMap] of this.collections.entries()) {
        const expiredIds: UUID[] = [];
        
        for (const [id, record] of collectionMap.entries()) {
          if (record.metadata.ttl) {
            const expiryTime = new Date(record.metadata.createdAt).getTime() + (record.metadata.ttl * 1000);
            if (now > expiryTime) {
              expiredIds.push(id);
            }
          }
        }
        
        // Remove expired records
        for (const id of expiredIds) {
          collectionMap.delete(id);
        }
        
        // Remove empty collections
        if (collectionMap.size === 0) {
          this.collections.delete(collectionName);
        }
      }
      
    } catch (error) {
      console.warn('Memory storage cleanup failed:', error);
    }
  }
  
  /**
   * Close storage and save to disk if persistence enabled
   */
  async close(): Promise<void> {
    if (this.autoSaveTimer) {
      clearInterval(this.autoSaveTimer);
      this.autoSaveTimer = undefined;
    }
    
    if (this.options.enablePersistence) {
      await this.saveToDisk();
    }
    
    this.collections.clear();
  }
  
  /**
   * Get total record count across all collections
   */
  private getTotalRecordCount(): number {
    let total = 0;
    for (const collection of this.collections.values()) {
      total += collection.size;
    }
    return total;
  }
  
  /**
   * Check if record matches query filters
   */
  private matchesFilters<T extends RecordData>(record: DataRecord<T>, filters?: Record<string, any>): boolean {
    if (!filters) return true;
    
    for (const [key, value] of Object.entries(filters)) {
      // Support nested property access with dot notation
      const recordValue = this.getNestedProperty(record, key);
      
      // Support MongoDB-style operators
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        if (value.$gt !== undefined && recordValue <= value.$gt) return false;
        if (value.$gte !== undefined && recordValue < value.$gte) return false;
        if (value.$lt !== undefined && recordValue >= value.$lt) return false;
        if (value.$lte !== undefined && recordValue > value.$lte) return false;
        if (value.$ne !== undefined && recordValue === value.$ne) return false;
        if (value.$in !== undefined && !value.$in.includes(recordValue)) return false;
        if (value.$nin !== undefined && value.$in.includes(recordValue)) return false;
      } else {
        // Simple equality check
        if (recordValue !== value) {
          return false;
        }
      }
    }
    
    return true;
  }
  
  /**
   * Get nested property value using dot notation
   */
  private getNestedProperty(obj: any, path: string): any {
    return path.split('.').reduce((current, key) => current?.[key], obj);
  }
  
  /**
   * Compare records for sorting
   */
  private compareRecords<T extends RecordData>(
    a: DataRecord<T>,
    b: DataRecord<T>,
    sortFields: { field: string; direction: 'asc' | 'desc' }[]
  ): number {
    for (const sort of sortFields) {
      const aValue = this.getNestedProperty(a, sort.field);
      const bValue = this.getNestedProperty(b, sort.field);
      
      if (aValue < bValue) return sort.direction === 'asc' ? -1 : 1;
      if (aValue > bValue) return sort.direction === 'asc' ? 1 : -1;
    }
    
    return 0;
  }
  
  /**
   * Load data from disk persistence
   */
  private async loadFromDisk(): Promise<void> {
    if (!this.options.persistencePath) return;
    
    try {
      const fs = await import('fs/promises');
      const data = await fs.readFile(this.options.persistencePath, 'utf-8');
      const serialized = JSON.parse(data);
      
      // Reconstruct Maps from serialized data
      this.collections.clear();
      for (const [collectionName, records] of Object.entries(serialized)) {
        const collectionMap = new Map<UUID, DataRecord<any>>();
        for (const [id, record] of Object.entries(records as Record<UUID, DataRecord<any>>)) {
          collectionMap.set(id, record);
        }
        this.collections.set(collectionName, collectionMap);
      }
      
    } catch (error) {
      // Persistence file doesn't exist or is invalid - start fresh
      console.warn('Memory storage persistence load failed, starting fresh:', error);
    }
  }
  
  /**
   * Save data to disk persistence
   */
  private async saveToDisk(): Promise<void> {
    if (!this.options.persistencePath) return;
    
    try {
      const fs = await import('fs/promises');
      const path = await import('path');
      
      // Convert Maps to serializable objects
      const serialized: Record<string, Record<UUID, DataRecord<any>>> = {};
      for (const [collectionName, collectionMap] of this.collections.entries()) {
        serialized[collectionName] = {};
        for (const [id, record] of collectionMap.entries()) {
          serialized[collectionName][id] = record;
        }
      }
      
      // Ensure directory exists
      const dir = path.dirname(this.options.persistencePath);
      await fs.mkdir(dir, { recursive: true });
      
      // Atomic write
      const tempPath = `${this.options.persistencePath}.tmp`;
      await fs.writeFile(tempPath, JSON.stringify(serialized, null, 2));
      await fs.rename(tempPath, this.options.persistencePath);

    } catch (error) {
      console.warn('Memory storage persistence save failed:', error);
    }
  }

  /**
   * Clear all data with detailed reporting (Memory Storage implementation)
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    try {
      const tablesCleared: string[] = [];
      let recordsDeleted = 0;

      // Count records before clearing
      for (const [collectionName, collectionMap] of this.collections.entries()) {
        recordsDeleted += collectionMap.size;
        tablesCleared.push(collectionName);
      }

      // Clear all data
      this.collections.clear();

      // Persist if configured
      if (this.options.persistencePath) {
        await this.saveToDisk();
      }

      console.log(`🧹 MemoryStorage: Cleared ${recordsDeleted} records from ${tablesCleared.length} collections`);

      return {
        success: true,
        data: {
          tablesCleared,
          recordsDeleted
        }
      };

    } catch (error) {
      return {
        success: false,
        error: `MemoryStorage clearAll failed: ${error}`
      };
    }
  }

  /**
   * Explain query execution (dry-run) - Memory adapter
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    return {
      query,
      translatedQuery: `In-memory query for collection "${query.collection}"`,
      parameters: [],
      estimatedRows: this.collections.get(query.collection)?.size || 0,
      executionPlan: 'In-memory Map-based filtering and JavaScript array operations',
      adapterType: 'memory',
      timestamp: new Date().toISOString()
    };
  }
}