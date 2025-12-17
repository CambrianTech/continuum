/**
 * File Storage Adapter - Filesystem-based Data Persistence
 * 
 * Implements DataStorageAdapter for JSON file storage with:
 * - Collection-based directory organization
 * - Atomic file operations
 * - Query support via filesystem traversal
 * - Metadata indexing for performance
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { getDatabaseDir } from '../../../system/config/ServerConfig';
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
 * File Storage Configuration
 */
interface FileStorageOptions {
  readonly basePath: string;
  readonly createDirectories?: boolean;
  readonly atomicWrites?: boolean;
  readonly enableIndexes?: boolean;
}

/**
 * File Storage Adapter - JSON-based Persistence
 */
export class FileStorageAdapter extends DataStorageAdapter {
  private basePath: string = '';
  private namespace: string = '';
  private options: FileStorageOptions = { basePath: '' };
  
  /**
   * Initialize file storage with base path and namespace
   */
  async initialize(config: StorageAdapterConfig): Promise<void> {
    this.namespace = config.namespace;
    this.options = {
      basePath: getDatabaseDir(),
      createDirectories: true,
      atomicWrites: true,
      enableIndexes: false,
      ...config.options
    } as FileStorageOptions;
    
    this.basePath = path.resolve(this.options.basePath, this.namespace);
    
    if (this.options.createDirectories) {
      await fs.mkdir(this.basePath, { recursive: true });
    }
  }
  
  /**
   * Create record as JSON file
   */
  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    try {
      const collectionPath = path.join(this.basePath, record.collection);
      await fs.mkdir(collectionPath, { recursive: true });
      
      const filePath = path.join(collectionPath, `${record.id}.json`);
      
      if (this.options.atomicWrites) {
        const tempPath = `${filePath}.tmp`;
        await fs.writeFile(tempPath, JSON.stringify(record, null, 2));
        await fs.rename(tempPath, filePath);
      } else {
        await fs.writeFile(filePath, JSON.stringify(record, null, 2));
      }
      
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
   * Read record from JSON file
   */
  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    try {
      const filePath = path.join(this.basePath, collection, `${id}.json`);
      const content = await fs.readFile(filePath, 'utf-8');
      const record = JSON.parse(content) as DataRecord<T>;
      
      return {
        success: true,
        data: record
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: undefined
        };
      }
      
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Query records via filesystem traversal with filters
   */
  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    try {
      const collectionPath = path.join(this.basePath, query.collection);
      
      try {
        const files = await fs.readdir(collectionPath);
        const records: DataRecord<T>[] = [];
        
        for (const file of files) {
          if (file.endsWith('.json')) {
            try {
              const content = await fs.readFile(path.join(collectionPath, file), 'utf-8');
              const record = JSON.parse(content) as DataRecord<T>;
              
              if (this.matchesFilters(record, query.filter)) {
                records.push(record);
              }
            } catch (parseError) {
              // Skip invalid JSON files
              continue;
            }
          }
        }
        
        // Apply sorting
        if (query.sort && query.sort.length > 0) {
          records.sort((a, b) => this.compareRecords(a, b, query.sort!));
        }
        
        // Apply pagination
        const startIndex = query.offset || 0;
        const endIndex = query.limit ? startIndex + query.limit : records.length;
        const paginatedRecords = records.slice(startIndex, endIndex);
        
        return {
          success: true,
          data: paginatedRecords,
          metadata: {
            totalCount: records.length,
            queryTime: Date.now()
          }
        };
        
      } catch (dirError: any) {
        if (dirError.code === 'ENOENT') {
          // Collection doesn't exist
          return {
            success: true,
            data: [],
            metadata: { totalCount: 0 }
          };
        }
        throw dirError;
      }
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Update record in place
   */
  async update<T extends RecordData>(
    collection: string, 
    id: UUID, 
    data: Partial<T>, 
    incrementVersion: boolean = true
  ): Promise<StorageResult<DataRecord<T>>> {
    try {
      // Read existing record
      const existingResult = await this.read<T>(collection, id);
      if (!existingResult.success || !existingResult.data) {
        return {
          success: false,
          error: 'Record not found'
        };
      }
      
      // Create updated record
      const updated: DataRecord<T> = {
        ...existingResult.data,
        data: { ...existingResult.data.data, ...data },
        metadata: {
          ...existingResult.data.metadata,
          updatedAt: new Date().toISOString(),
          version: incrementVersion ? existingResult.data.metadata.version + 1 : existingResult.data.metadata.version
        }
      };
      
      // Write back to file
      const createResult = await this.create(updated);
      return createResult;
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Delete record file
   */
  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    try {
      const filePath = path.join(this.basePath, collection, `${id}.json`);
      await fs.unlink(filePath);
      
      return {
        success: true,
        data: true
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: false // File didn't exist
        };
      }
      
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * List collections as directories
   */
  async listCollections(): Promise<StorageResult<string[]>> {
    try {
      const entries = await fs.readdir(this.basePath, { withFileTypes: true });
      const collections = entries
        .filter(entry => entry.isDirectory())
        .map(entry => entry.name);
      
      return {
        success: true,
        data: collections
      };
      
    } catch (error: any) {
      if (error.code === 'ENOENT') {
        return {
          success: true,
          data: []
        };
      }
      
      return {
        success: false,
        error: error.message
      };
    }
  }
  
  /**
   * Get collection statistics via directory analysis
   */
  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    try {
      const collectionPath = path.join(this.basePath, collection);
      const files = await fs.readdir(collectionPath);
      const jsonFiles = files.filter(f => f.endsWith('.json'));
      
      let totalSize = 0;
      let lastModified = '';
      
      for (const file of jsonFiles) {
        const stat = await fs.stat(path.join(collectionPath, file));
        totalSize += stat.size;
        if (stat.mtime.toISOString() > lastModified) {
          lastModified = stat.mtime.toISOString();
        }
      }
      
      const stats: CollectionStats = {
        name: collection,
        recordCount: jsonFiles.length,
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
   * Batch operations - Sequential file operations
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
   * Ensure schema exists (no-op - file storage is schemaless)
   */
  async ensureSchema(_collection: string, _schema?: unknown): Promise<StorageResult<boolean>> {
    return { success: true, data: true };
  }

  /**
   * Clear all data from all collections
   */

  async clear(): Promise<StorageResult<boolean>> {
    try {
      const collections = await this.listCollections();
      if (collections.success && collections.data) {
        for (const collection of collections.data) {
          const truncateResult = await this.truncate(collection);
          if (!truncateResult.success) {
            return truncateResult;
          }
        }
      }

      console.log('🧹 FileStorage: All data cleared successfully');
      return {
        success: true,
        data: true
      };

    } catch (error: any) {
      console.error('❌ FileStorage: Error clearing data:', error.message);
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
      const collectionPath = path.join(this.basePath, collection);

      try {
        const files = await fs.readdir(collectionPath);
        const jsonFiles = files.filter(f => f.endsWith('.json'));

        // Delete all JSON files in the collection
        for (const file of jsonFiles) {
          await fs.unlink(path.join(collectionPath, file));
        }

        console.log(`🗑️ FileStorage: Truncated collection '${collection}' (${jsonFiles.length} records)`);
        return {
          success: true,
          data: true
        };

      } catch (dirError: any) {
        if (dirError.code === 'ENOENT') {
          // Collection doesn't exist, nothing to truncate
          console.log(`ℹ️ FileStorage: Collection '${collection}' doesn't exist, skipping truncate`);
          return {
            success: true,
            data: true
          };
        }
        throw dirError;
      }

    } catch (error: any) {
      console.error(`❌ FileStorage: Error truncating collection '${collection}':`, error.message);
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Cleanup - Remove empty directories, optimize storage
   */
  async cleanup(): Promise<void> {
    try {
      const collections = await this.listCollections();
      if (collections.success && collections.data) {
        for (const collection of collections.data) {
          const collectionPath = path.join(this.basePath, collection);
          const files = await fs.readdir(collectionPath);
          if (files.length === 0) {
            await fs.rmdir(collectionPath);
          }
        }
      }
    } catch (error) {
      // Cleanup errors are non-fatal
    }
  }
  
  /**
   * Close storage connections (no-op for file storage)
   */
  async close(): Promise<void> {
    // File storage doesn't need connection cleanup
  }
  
  /**
   * Check if record matches query filters
   */
  private matchesFilters<T extends RecordData>(record: DataRecord<T>, filters?: Record<string, any>): boolean {
    if (!filters) return true;
    
    for (const [key, value] of Object.entries(filters)) {
      // Support nested property access with dot notation
      const recordValue = this.getNestedProperty(record, key);
      
      if (recordValue !== value) {
        return false;
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
   * Clear all data with detailed reporting (File Storage implementation)
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    try {
      const collections = await this.listCollections();
      if (!collections.success || !collections.data) {
        return { success: false, error: 'Failed to list collections for clearing' };
      }

      const tablesCleared: string[] = [];
      let recordsDeleted = 0;

      for (const collection of collections.data) {
        const query = await this.query({ collection });
        if (query.success && query.data) {
          recordsDeleted += query.data.length;

          // Clear collection by truncating
          const truncateResult = await this.truncate(collection);
          if (truncateResult.success) {
            tablesCleared.push(collection);
          }
        }
      }

      console.log(`🧹 FileStorage: Cleared ${recordsDeleted} records from ${tablesCleared.length} collections`);

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
        error: `FileStorage clearAll failed: ${error}`
      };
    }
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
   * Explain query execution (dry-run) - Legacy file adapter
   */
  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    return {
      query,
      translatedQuery: `Legacy FileStorageAdapter query for collection "${query.collection}"`,
      parameters: [],
      estimatedRows: 0,
      executionPlan: 'This is a legacy adapter - use JsonFileStorageAdapter for detailed explanations',
      adapterType: 'file-legacy',
      timestamp: new Date().toISOString()
    };
  }
}