/**
 * Hybrid Adapter - Multi-backend data access with migration capabilities
 * 
 * Reads from multiple sources in priority order (JSON first, then SQLite)
 * Writes to single target (SQLite for performance)
 * Enables gradual migration from JSON files to structured database
 */

import type { 
  BaseEntity,
  DataResult,
  DataError,
  DataOperationContext,
  QueryOptions
} from '../domains/CoreTypes';
import { Ok, Err, createDataError } from '../domains/CoreTypes';
import type { DataAdapter } from '../services/DataService';

/**
 * Hybrid Adapter Configuration
 */
export interface HybridAdapterConfig {
  readonly readAdapters: readonly DataAdapter[];  // Try these in order for reads
  readonly writeAdapter: DataAdapter;             // Always write to this one
  readonly migration?: {
    readonly autoMigrate: boolean;                // Auto-migrate on successful reads
    readonly migrateOnWrite: boolean;             // Migrate before writes
  };
}

/**
 * HybridAdapter Implementation
 */
export class HybridAdapter implements DataAdapter {
  readonly name = 'HybridAdapter';
  
  // Combine capabilities of all adapters
  readonly capabilities: DataAdapter['capabilities'];

  private config: HybridAdapterConfig;

  constructor(config: HybridAdapterConfig) {
    this.config = config;
    
    // Merge capabilities from all adapters
    this.capabilities = {
      supportsTransactions: config.writeAdapter.capabilities.supportsTransactions,
      supportsFullTextSearch: config.readAdapters.some(a => a.capabilities.supportsFullTextSearch) || 
                              config.writeAdapter.capabilities.supportsFullTextSearch,
      supportsRelations: config.writeAdapter.capabilities.supportsRelations,
      supportsJsonQueries: config.readAdapters.some(a => a.capabilities.supportsJsonQueries) || 
                          config.writeAdapter.capabilities.supportsJsonQueries
    };
  }

  async initialize(): Promise<DataResult<void>> {
    try {
      // Initialize write adapter first
      const writeResult = await this.config.writeAdapter.initialize();
      if (!writeResult.success) {
        return writeResult;
      }

      // Initialize all read adapters
      for (const adapter of this.config.readAdapters) {
        const result = await adapter.initialize();
        if (!result.success) {
          return Err(createDataError(
            'STORAGE_ERROR', 
            `Failed to initialize read adapter ${adapter.name}: ${result.error.message}`
          ));
        }
      }

      return Ok(undefined);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `HybridAdapter initialization failed: ${error.message}`));
    }
  }

  async close(): Promise<DataResult<void>> {
    const errors: string[] = [];

    try {
      // Close write adapter
      const writeResult = await this.config.writeAdapter.close();
      if (!writeResult.success) {
        errors.push(`Write adapter: ${writeResult.error.message}`);
      }

      // Close all read adapters
      for (const adapter of this.config.readAdapters) {
        const result = await adapter.close();
        if (!result.success) {
          errors.push(`Read adapter ${adapter.name}: ${result.error.message}`);
        }
      }

      if (errors.length > 0) {
        return Err(createDataError('STORAGE_ERROR', `HybridAdapter close errors: ${errors.join(', ')}`));
      }

      return Ok(undefined);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `HybridAdapter close failed: ${error.message}`));
    }
  }

  async create<T extends BaseEntity>(
    collection: string, 
    data: Omit<T, keyof BaseEntity>, 
    context: DataOperationContext
  ): Promise<DataResult<T>> {
    // Always create in the write adapter
    return await this.config.writeAdapter.create<T>(collection, data, context);
  }

  async read<T extends BaseEntity>(
    collection: string, 
    id: string, 
    context: DataOperationContext
  ): Promise<DataResult<T | null>> {
    // Try read adapters in order until we find the record
    for (const adapter of this.config.readAdapters) {
      try {
        const result = await adapter.read<T>(collection, id, context);
        
        if (!result.success) {
          // If adapter failed, try next one
          continue;
        }

        if (result.data) {
          // Found the record! Optionally migrate to write adapter
          if (this.config.migration?.autoMigrate) {
            await this.migrateRecord(collection, result.data, context);
          }
          return result;
        }

        // Record not found in this adapter, try next
      } catch (error) {
        // Adapter error, try next
        continue;
      }
    }

    // Try write adapter last (in case it's also a read source)
    return await this.config.writeAdapter.read<T>(collection, id, context);
  }

  async update<T extends BaseEntity>(
    collection: string, 
    id: string, 
    data: Partial<T>, 
    context: DataOperationContext
  ): Promise<DataResult<T>> {
    // First, ensure the record exists and is migrated
    if (this.config.migration?.migrateOnWrite) {
      const readResult = await this.read<T>(collection, id, context);
      if (!readResult.success) {
        return readResult as DataResult<T>;
      }
      
      if (!readResult.data) {
        return Err(createDataError('NOT_FOUND', `${collection}/${id} not found for update`));
      }
    }

    // Always update in the write adapter
    return await this.config.writeAdapter.update<T>(collection, id, data, context);
  }

  async delete(
    collection: string, 
    id: string, 
    context: DataOperationContext
  ): Promise<DataResult<boolean>> {
    let deletedAny = false;

    // Delete from write adapter
    const writeResult = await this.config.writeAdapter.delete(collection, id, context);
    if (writeResult.success && writeResult.data) {
      deletedAny = true;
    }

    // Optionally delete from read adapters too (cleanup)
    for (const adapter of this.config.readAdapters) {
      try {
        const result = await adapter.delete(collection, id, context);
        if (result.success && result.data) {
          deletedAny = true;
        }
      } catch (error) {
        // Non-critical error, continue
      }
    }

    return Ok(deletedAny);
  }

  async list<T extends BaseEntity>(
    collection: string, 
    options: QueryOptions<T> = {}, 
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<T[]>> {
    // Collect records from all adapters and merge
    const allRecords = new Map<string, T>();
    const errors: string[] = [];

    // Read from all adapters
    for (const adapter of [this.config.writeAdapter, ...this.config.readAdapters]) {
      try {
        const result = await adapter.list<T>(collection, options, context);
        
        if (result.success) {
          for (const record of result.data) {
            // Use the record with the highest version number
            const existing = allRecords.get(record.id);
            if (!existing || record.version > existing.version) {
              allRecords.set(record.id, record);
            }
          }
        } else {
          errors.push(`${adapter.name}: ${result.error.message}`);
        }
      } catch (error: any) {
        errors.push(`${adapter.name}: ${error.message}`);
      }
    }

    const records = Array.from(allRecords.values());

    // Apply sorting (options.orderBy was handled by individual adapters, but we need to re-sort merged results)
    if (options.orderBy?.length) {
      records.sort((a, b) => {
        for (const sort of options.orderBy!) {
          const aVal = (a as any)[sort.field];
          const bVal = (b as any)[sort.field];
          
          if (aVal < bVal) return sort.direction === 'ASC' ? -1 : 1;
          if (aVal > bVal) return sort.direction === 'ASC' ? 1 : -1;
        }
        return 0;
      });
    }

    // Apply pagination to merged results
    let finalRecords = records;
    const offset = options.offset || 0;
    const limit = options.limit;
    
    if (limit !== undefined) {
      finalRecords = records.slice(offset, offset + limit);
    } else if (offset > 0) {
      finalRecords = records.slice(offset);
    }

    return Ok(finalRecords);
  }

  async query<T extends BaseEntity>(
    collection: string,
    filters: Record<string, unknown>,
    options: QueryOptions<T> = {},
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<T[]>> {
    // Use list with filters for hybrid query
    return await this.list<T>(collection, { ...options, filters: filters as Record<keyof T, unknown> }, context);
  }

  async count(
    collection: string,
    filters: Record<string, unknown> = {},
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<number>> {
    // Count unique records across all adapters
    const listResult = await this.query(collection, filters, {}, context);
    if (!listResult.success) {
      return listResult as DataResult<number>;
    }
    
    return Ok(listResult.data.length);
  }

  /**
   * Migrate a record from read adapter to write adapter
   */
  private async migrateRecord<T extends BaseEntity>(
    collection: string,
    record: T,
    context: DataOperationContext
  ): Promise<void> {
    try {
      // Check if record already exists in write adapter
      const existsResult = await this.config.writeAdapter.read<T>(collection, record.id, context);
      
      if (existsResult.success && !existsResult.data) {
        // Record doesn't exist in write adapter, migrate it
        const { id, createdAt, updatedAt, version, ...data } = record;
        await this.config.writeAdapter.create<T>(collection, data as Omit<T, keyof BaseEntity>, context);
      }
    } catch (error) {
      // Migration error is non-critical, just log it
      console.warn(`Failed to migrate ${collection}/${record.id}:`, error);
    }
  }

  /**
   * Get adapter statistics for debugging
   */
  async getAdapterStats(): Promise<{
    writeAdapter: string;
    readAdapters: string[];
    capabilities: DataAdapter['capabilities'];
  }> {
    return {
      writeAdapter: this.config.writeAdapter.name,
      readAdapters: this.config.readAdapters.map(a => a.name),
      capabilities: this.capabilities
    };
  }
}