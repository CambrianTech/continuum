/**
 * DataService - Professional ORM-like Data Operations
 * 
 * Following widget.executeCommand pattern for data operations
 * Provides type-safe, adapter-agnostic data access with strict typing
 * Supports multiple backends: JSON files, SQLite, PostgreSQL
 */

import type { 
  BaseEntity,
  DataResult,
  DataError,
  DataOperation,
  DataOperationContext,
  QueryOptions,
  UserId,
  SessionId,
  ISOString
} from '../domains/CoreTypes';
import { Ok, Err, createDataError } from '../domains/CoreTypes';
import { generateUUID } from '../../core/types/CrossPlatformUUID';

/**
 * Data Adapter Interface - Like DataStorageAdapter but cleaner
 */
export interface DataAdapter {
  readonly name: string;
  readonly capabilities: {
    readonly supportsTransactions: boolean;
    readonly supportsFullTextSearch: boolean;
    readonly supportsRelations: boolean;
    readonly supportsJsonQueries: boolean;
  };

  // Core CRUD operations
  create<T extends BaseEntity>(collection: string, data: Omit<T, keyof BaseEntity>, context: DataOperationContext): Promise<DataResult<T>>;
  read<T extends BaseEntity>(collection: string, id: string, context: DataOperationContext): Promise<DataResult<T | null>>;
  update<T extends BaseEntity>(collection: string, id: string, data: Partial<T>, context: DataOperationContext): Promise<DataResult<T>>;
  delete(collection: string, id: string, context: DataOperationContext): Promise<DataResult<boolean>>;
  list<T extends BaseEntity>(collection: string, options?: QueryOptions<T>, context?: DataOperationContext): Promise<DataResult<T[]>>;
  query<T extends BaseEntity>(collection: string, filters: Record<string, unknown>, options?: QueryOptions<T>, context?: DataOperationContext): Promise<DataResult<T[]>>;
  count(collection: string, filters?: Record<string, unknown>, context?: DataOperationContext): Promise<DataResult<number>>;

  // Lifecycle
  initialize(): Promise<DataResult<void>>;
  close(): Promise<DataResult<void>>;
}

/**
 * Hybrid Adapter Configuration - Multi-backend support
 */
export interface HybridAdapterConfig {
  readonly read: readonly DataAdapter[];    // Try these in order for reads
  readonly write: DataAdapter;             // Always write to this one
  readonly migrate?: {
    readonly from: DataAdapter;
    readonly to: DataAdapter;
    readonly autoMigrate: boolean;
  };
}

/**
 * Data Service Configuration
 */
export interface DataServiceConfig {
  readonly adapters: Record<string, DataAdapter | HybridAdapterConfig>;
  readonly defaultAdapter: DataAdapter;
  readonly context: {
    readonly userId?: UserId;
    readonly sessionId: SessionId;
    readonly source: string;
  };
}

/**
 * DataService - Professional data operations like widget.executeCommand
 */
export class DataService {
  private config: DataServiceConfig;
  private isInitialized: boolean = false;

  constructor(config: DataServiceConfig) {
    this.config = config;
  }

  /**
   * Initialize all adapters
   */
  async initialize(): Promise<DataResult<void>> {
    if (this.isInitialized) {
      return Ok(undefined);
    }

    try {
      // Initialize default adapter
      const defaultResult = await this.config.defaultAdapter.initialize();
      if (!defaultResult.success) {
        return defaultResult;
      }

      // Initialize all configured adapters
      for (const [collection, adapterConfig] of Object.entries(this.config.adapters)) {
        if (this.isHybridConfig(adapterConfig)) {
          // Initialize all adapters in hybrid config
          for (const adapter of adapterConfig.read) {
            const result = await adapter.initialize();
            if (!result.success) {
              return Err(createDataError('STORAGE_ERROR', `Failed to initialize read adapter for ${collection}: ${result.error.message}`));
            }
          }
          const writeResult = await adapterConfig.write.initialize();
          if (!writeResult.success) {
            return Err(createDataError('STORAGE_ERROR', `Failed to initialize write adapter for ${collection}: ${writeResult.error.message}`));
          }
        } else {
          const result = await adapterConfig.initialize();
          if (!result.success) {
            return Err(createDataError('STORAGE_ERROR', `Failed to initialize adapter for ${collection}: ${result.error.message}`));
          }
        }
      }

      this.isInitialized = true;
      return Ok(undefined);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `DataService initialization failed: ${error.message}`));
    }
  }

  /**
   * Execute data operation - Like widget.executeCommand but for data
   */
  async executeOperation<T extends BaseEntity>(
    operation: `${string}/${DataOperation}`,
    data?: unknown,
    options?: {
      readonly context?: Partial<DataOperationContext>;
      readonly adapter?: string;
    }
  ): Promise<DataResult<T | T[] | boolean | number>> {
    if (!this.isInitialized) {
      const initResult = await this.initialize();
      if (!initResult.success) {
        return initResult as DataResult<T>;
      }
    }

    const [collection, op] = operation.split('/') as [string, DataOperation];
    const context = this.createContext(options?.context);
    const adapter = this.getAdapter(collection, options?.adapter);

    try {
      switch (op) {
        case 'create':
          // Apply strict validation for create operations (Rust-like enforcement)
          if (collection === 'users' && this.shouldValidateUser(data)) {
            const { validateUserData } = await import('../domains/User');
            const validation = validateUserData(data as any);
            if (!validation.success) {
              return Err(validation.error);
            }
          }
          
          if (collection.includes('message') && this.shouldValidateMessage(data)) {
            const { validateMessageData } = await import('../domains/ChatMessage');
            const validation = validateMessageData(data as any);
            if (!validation.success) {
              return Err(validation.error);
            }
          }
          
          return await adapter.create<T>(collection, data as Omit<T, keyof BaseEntity>, context);
        
        case 'read':
          const id = (data as any)?.id;
          if (!id) {
            return Err(createDataError('VALIDATION_ERROR', 'ID is required for read operation'));
          }
          return await adapter.read<T>(collection, id, context);
        
        case 'update':
          const updateData = data as { id: string } & Partial<T>;
          if (!updateData.id) {
            return Err(createDataError('VALIDATION_ERROR', 'ID is required for update operation'));
          }
          return await adapter.update<T>(collection, updateData.id, updateData, context);
        
        case 'delete':
          const deleteId = (data as any)?.id;
          if (!deleteId) {
            return Err(createDataError('VALIDATION_ERROR', 'ID is required for delete operation'));
          }
          return await adapter.delete(collection, deleteId, context) as DataResult<boolean>;
        
        case 'list':
          const listOptions = data as QueryOptions<T> | undefined;
          return await adapter.list<T>(collection, listOptions, context) as DataResult<T[]>;
        
        case 'query':
          const queryData = data as { filters: Record<string, unknown>; options?: QueryOptions<T> };
          return await adapter.query<T>(collection, queryData.filters, queryData.options, context) as DataResult<T[]>;
        
        case 'count':
          const countFilters = (data as any)?.filters;
          return await adapter.count(collection, countFilters, context) as DataResult<number>;
        
        default:
          return Err(createDataError('VALIDATION_ERROR', `Unknown operation: ${op}`));
      }

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Operation ${operation} failed: ${error.message}`));
    }
  }

  /**
   * Convenient typed methods (ORM-like)
   */
  async create<T extends BaseEntity>(
    collection: string, 
    data: Omit<T, keyof BaseEntity>, 
    context?: Partial<DataOperationContext>
  ): Promise<DataResult<T>> {
    // Add validation for known collections
    if (collection === 'users' && this.shouldValidateUser(data)) {
      const { validateUserData } = await import('../domains/User');
      const validation = validateUserData(data as any);
      if (!validation.success) {
        return Err(validation.error);
      }
    }
    
    if (collection.includes('message') && this.shouldValidateMessage(data)) {
      const { validateMessageData } = await import('../domains/ChatMessage');
      const validation = validateMessageData(data as any);
      if (!validation.success) {
        return Err(validation.error);
      }
    }
    
    return await this.executeOperation<T>(`${collection}/create`, data, { context }) as DataResult<T>;
  }

  async read<T extends BaseEntity>(
    collection: string, 
    id: string, 
    context?: Partial<DataOperationContext>
  ): Promise<DataResult<T | null>> {
    return await this.executeOperation<T>(`${collection}/read`, { id }, { context }) as DataResult<T | null>;
  }

  async update<T extends BaseEntity>(
    collection: string, 
    id: string, 
    data: Partial<T>, 
    context?: Partial<DataOperationContext>
  ): Promise<DataResult<T>> {
    return await this.executeOperation<T>(`${collection}/update`, { id, ...data }, { context }) as DataResult<T>;
  }

  async delete(
    collection: string, 
    id: string, 
    context?: Partial<DataOperationContext>
  ): Promise<DataResult<boolean>> {
    return await this.executeOperation(`${collection}/delete`, { id }, { context }) as DataResult<boolean>;
  }

  async list<T extends BaseEntity>(
    collection: string, 
    options?: QueryOptions<T>, 
    context?: Partial<DataOperationContext>
  ): Promise<DataResult<T[]>> {
    return await this.executeOperation<T>(`${collection}/list`, options, { context }) as DataResult<T[]>;
  }

  async query<T extends BaseEntity>(
    collection: string, 
    filters: Record<string, unknown>, 
    options?: QueryOptions<T>, 
    context?: Partial<DataOperationContext>
  ): Promise<DataResult<T[]>> {
    return await this.executeOperation<T>(`${collection}/query`, { filters, options }, { context }) as DataResult<T[]>;
  }

  /**
   * Close all connections
   */
  async close(): Promise<DataResult<void>> {
    const errors: string[] = [];

    try {
      // Close default adapter
      const defaultResult = await this.config.defaultAdapter.close();
      if (!defaultResult.success) {
        errors.push(`Default adapter: ${defaultResult.error.message}`);
      }

      // Close all configured adapters
      for (const [collection, adapterConfig] of Object.entries(this.config.adapters)) {
        if (this.isHybridConfig(adapterConfig)) {
          for (const adapter of [...adapterConfig.read, adapterConfig.write]) {
            const result = await adapter.close();
            if (!result.success) {
              errors.push(`${collection} adapter: ${result.error.message}`);
            }
          }
        } else {
          const result = await adapterConfig.close();
          if (!result.success) {
            errors.push(`${collection} adapter: ${result.error.message}`);
          }
        }
      }

      this.isInitialized = false;

      if (errors.length > 0) {
        return Err(createDataError('STORAGE_ERROR', `DataService close errors: ${errors.join(', ')}`));
      }

      return Ok(undefined);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `DataService close failed: ${error.message}`));
    }
  }

  // Private helper methods
  private isHybridConfig(config: DataAdapter | HybridAdapterConfig): config is HybridAdapterConfig {
    return 'read' in config && 'write' in config;
  }

  private getAdapter(collection: string, adapterName?: string): DataAdapter {
    if (adapterName) {
      const adapter = this.config.adapters[adapterName];
      if (!adapter) {
        throw new Error(`Adapter ${adapterName} not found`);
      }
      return this.isHybridConfig(adapter) ? adapter.write : adapter;
    }

    const collectionAdapter = this.config.adapters[collection];
    if (collectionAdapter) {
      return this.isHybridConfig(collectionAdapter) ? collectionAdapter.write : collectionAdapter;
    }

    return this.config.defaultAdapter;
  }

  private createContext(override?: Partial<DataOperationContext>): DataOperationContext {
    return {
      sessionId: this.config.context.sessionId,
      timestamp: new Date().toISOString() as ISOString,
      source: this.config.context.source,
      userId: this.config.context.userId,
      transactionId: generateUUID(),
      ...override
    };
  }

  private shouldValidateUser(data: any): boolean {
    return data && typeof data === 'object' && ('displayName' in data || 'type' in data);
  }

  private shouldValidateMessage(data: any): boolean {
    return data && typeof data === 'object' && ('content' in data || 'roomId' in data);
  }
}