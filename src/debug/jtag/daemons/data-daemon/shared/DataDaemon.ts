/**
 * Data Daemon - Universal Storage Orchestrator
 * 
 * Heavy abstraction for organizational data with pluggable storage strategies:
 * - SQL backends: PostgreSQL, SQLite with relational concepts (tables, joins, indices)
 * - NoSQL backends: MongoDB, Redis with document concepts (collections, queries)
 * - File backends: JSON, Binary, Structured with filesystem organization
 * - Network backends: Distributed storage with consistency models
 * 
 * Supports both relational and document paradigms through unified interface
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { 
  DataStorageAdapter, 
  DataRecord, 
  StorageQuery, 
  StorageResult, 
  StorageAdapterConfig,
  CollectionStats,
  StorageOperation 
} from './DataStorageAdapter';
import { DefaultStorageAdapterFactory } from '../server/StorageAdapterFactory';

/**
 * Storage Strategy Configuration
 */
export interface StorageStrategyConfig {
  readonly strategy: 'sql' | 'nosql' | 'file' | 'memory' | 'network' | 'hybrid';
  readonly backend: string; // 'postgres', 'sqlite', 'mongodb', 'redis', 'json', etc.
  readonly namespace: string;
  readonly options?: Record<string, any>;
  readonly features?: {
    readonly enableTransactions?: boolean;
    readonly enableIndexing?: boolean;
    readonly enableReplication?: boolean;
    readonly enableSharding?: boolean;
    readonly enableCaching?: boolean;
  };
}

/**
 * Data Operation Context
 */
export interface DataOperationContext {
  readonly sessionId: UUID;
  readonly timestamp: string;
  readonly source: string;
  readonly transactionId?: UUID;
  readonly consistency?: 'eventual' | 'strong' | 'session';
}

/**
 * Universal Data Daemon - Storage Strategy Abstraction
 * 
 * Orchestrates data operations across any storage backend while maintaining
 * consistent interface regardless of underlying SQL/NoSQL/File strategy
 */
export class DataDaemon {
  private adapter: DataStorageAdapter;
  private config: StorageStrategyConfig;
  private factory: DefaultStorageAdapterFactory;
  private isInitialized: boolean = false;
  
  constructor(config: StorageStrategyConfig, factory?: DefaultStorageAdapterFactory) {
    this.config = config;
    this.factory = factory || new DefaultStorageAdapterFactory();
    
    // Create adapter via factory - use backend type, not strategy
    const adapterConfig: StorageAdapterConfig = {
      type: this.config.backend as any,
      namespace: this.config.namespace,
      options: this.config.options
    };

    this.adapter = this.factory.createAdapter(adapterConfig);
  }
  
  /**
   * Initialize storage backend
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) return;
    
    const adapterConfig: StorageAdapterConfig = {
      type: this.config.backend as any,
      namespace: this.config.namespace,
      options: {
        ...this.config.options,
        strategy: this.config.strategy,
        features: this.config.features
      }
    };
    
    await this.adapter.initialize(adapterConfig);
    this.isInitialized = true;
  }
  
  /**
   * Create record - Universal interface for SQL INSERT or NoSQL insert
   */
  async create<T>(
    collection: string, 
    data: T, 
    context: DataOperationContext,
    id?: UUID
  ): Promise<StorageResult<DataRecord<T>>> {
    await this.ensureInitialized();
    
    // Validate context and data
    const validationResult = this.validateOperation(collection, data, context);
    if (!validationResult.success) {
      return validationResult as StorageResult<DataRecord<T>>;
    }
    
    const record: DataRecord<T> = {
      id: id || this.generateId(),
      collection,
      data,
      metadata: {
        createdAt: context.timestamp,
        updatedAt: context.timestamp,
        version: 1
      }
    };
    
    return await this.adapter.create(record);
  }
  
  /**
   * Read single record - Universal interface for SQL SELECT or NoSQL findOne
   */
  async read<T>(
    collection: string, 
    id: UUID,
    context: DataOperationContext
  ): Promise<StorageResult<DataRecord<T>>> {
    await this.ensureInitialized();
    return await this.adapter.read<T>(collection, id);
  }
  
  /**
   * Query with complex filters - SQL WHERE clauses or NoSQL queries
   */
  async query<T>(
    query: StorageQuery,
    context: DataOperationContext
  ): Promise<StorageResult<DataRecord<T>[]>> {
    await this.ensureInitialized();
    return await this.adapter.query<T>(query);
  }
  
  /**
   * Update record - SQL UPDATE or NoSQL updateOne
   */
  async update<T>(
    collection: string, 
    id: UUID, 
    data: Partial<T>,
    context: DataOperationContext,
    incrementVersion: boolean = true
  ): Promise<StorageResult<DataRecord<T>>> {
    await this.ensureInitialized();
    return await this.adapter.update<T>(collection, id, data, incrementVersion);
  }
  
  /**
   * Delete record - SQL DELETE or NoSQL deleteOne
   */
  async delete(
    collection: string, 
    id: UUID,
    context: DataOperationContext
  ): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();
    return await this.adapter.delete(collection, id);
  }
  
  /**
   * List collections/tables
   */
  async listCollections(context: DataOperationContext): Promise<StorageResult<string[]>> {
    await this.ensureInitialized();
    return await this.adapter.listCollections();
  }
  
  /**
   * Collection statistics - Record counts, schema info, indices
   */
  async getCollectionStats(
    collection: string,
    context: DataOperationContext
  ): Promise<StorageResult<CollectionStats>> {
    await this.ensureInitialized();
    return await this.adapter.getCollectionStats(collection);
  }
  
  /**
   * Batch operations - Transactions for SQL, bulk operations for NoSQL
   */
  async batch(
    operations: StorageOperation[],
    context: DataOperationContext
  ): Promise<StorageResult<any[]>> {
    await this.ensureInitialized();
    return await this.adapter.batch(operations);
  }
  
  /**
   * Storage maintenance - VACUUM, reindex, cleanup
   */
  async maintenance(): Promise<void> {
    await this.ensureInitialized();
    await this.adapter.cleanup();
  }
  
  /**
   * Close daemon and storage connections
   */
  async close(): Promise<void> {
    if (this.isInitialized) {
      await this.adapter.close();
      this.isInitialized = false;
    }
  }
  
  /**
   * Shutdown daemon (alias for close)
   */
  async shutdown(): Promise<void> {
    await this.close();
  }
  
  /**
   * Ensure daemon is initialized before operations
   */
  private async ensureInitialized(): Promise<void> {
    if (!this.isInitialized) {
      await this.initialize();
    }
  }
  
  /**
   * Generate unique ID for records
   */
  private generateId(): UUID {
    return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as UUID;
  }
  
  /**
   * Validate operation parameters
   */
  private validateOperation(collection: string, data: any, context: DataOperationContext): StorageResult<any> {
    if (!collection || collection.trim() === '') {
      return {
        success: false,
        error: 'Collection name is required and cannot be empty'
      };
    }
    
    if (data === undefined || data === null) {
      return {
        success: false,
        error: 'Data is required and cannot be null or undefined'
      };
    }
    
    if (!context.sessionId || context.sessionId.trim() === '') {
      return {
        success: false,
        error: 'DataOperationContext.sessionId is required'
      };
    }
    
    if (!context.timestamp) {
      return {
        success: false,
        error: 'DataOperationContext.timestamp is required'
      };
    }
    
    return { success: true, data: null };
  }
}

/**
 * Storage Strategy Factory - Plugin System
 * 
 * Creates appropriate adapters based on storage strategy:
 * - SQL strategies → SQL adapters
 * - NoSQL strategies → Document adapters  
 * - File strategies → Filesystem adapters
 * - Network strategies → Distributed adapters
 */
export interface StorageStrategyFactory {
  createAdapter(config: StorageStrategyConfig): DataStorageAdapter;
  getSupportedStrategies(): readonly string[];
  getSupportedBackends(strategy: string): readonly string[];
}