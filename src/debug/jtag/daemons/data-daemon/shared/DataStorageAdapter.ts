/**
 * Data Storage Adapter - Universal Storage Abstraction
 * 
 * Plugin architecture for persistent data with multiple storage backends:
 * - FileStorageAdapter: Session-based filesystem (current implementation)
 * - MemoryStorageAdapter: In-memory with optional persistence
 * - DatabaseStorageAdapter: SQLite, PostgreSQL, MongoDB support
 * - NetworkStorageAdapter: Distributed storage across JTAG nodes
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Universal Data Record
 */
export interface DataRecord<T = any> {
  readonly id: UUID;
  readonly collection: string;
  readonly data: T;
  readonly metadata: DataRecordMetadata;
}

/**
 * Data Record Metadata - Versioning & Timestamps
 */
export interface DataRecordMetadata {
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly version: number;
  readonly tags?: readonly string[];
  readonly schema?: string;
  readonly ttl?: number; // Time to live in seconds
}

/**
 * Storage Query Interface - Data Retrieval
 */
export interface StorageQuery {
  readonly collection: string;
  readonly filters?: Record<string, any>;
  readonly sort?: { field: string; direction: 'asc' | 'desc' }[];
  readonly limit?: number;
  readonly offset?: number;
  readonly tags?: readonly string[];
  readonly timeRange?: {
    start?: string;
    end?: string;
  };
}

/**
 * Storage Operation Result
 */
export interface StorageResult<T = any> {
  readonly success: boolean;
  readonly data?: T;
  readonly error?: string;
  readonly metadata?: {
    totalCount?: number;
    queryTime?: number;
    cacheHit?: boolean;
  };
}

/**
 * Universal Storage Adapter Interface
 * 
 * ALL storage backends must implement this interface for AI memory persistence
 */
export abstract class DataStorageAdapter {
  
  /**
   * Initialize storage backend with configuration
   */
  abstract initialize(config: StorageAdapterConfig): Promise<void>;
  
  /**
   * Create or update a record
   */
  abstract create<T>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>>;
  
  /**
   * Read a single record by ID
   */
  abstract read<T>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>>;
  
  /**
   * Query records with complex filters
   */
  abstract query<T>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>>;
  
  /**
   * Update an existing record
   */
  abstract update<T>(collection: string, id: UUID, data: Partial<T>, incrementVersion?: boolean): Promise<StorageResult<DataRecord<T>>>;
  
  /**
   * Delete a record
   */
  abstract delete(collection: string, id: UUID): Promise<StorageResult<boolean>>;
  
  /**
   * List collections
   */
  abstract listCollections(): Promise<StorageResult<string[]>>;
  
  /**
   * Get collection statistics
   */
  abstract getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>>;
  
  /**
   * Batch operations for efficiency
   */
  abstract batch(operations: StorageOperation[]): Promise<StorageResult<any[]>>;
  
  /**
   * Cleanup and optimization
   */
  abstract cleanup(): Promise<void>;
  
  /**
   * Close storage connection
   */
  abstract close(): Promise<void>;
}

/**
 * Storage Adapter Configuration
 */
export interface StorageAdapterConfig {
  readonly type: 'file' | 'memory' | 'sqlite' | 'postgres' | 'mongodb' | 'network';
  readonly namespace: string; // Instead of sessionId - more generic
  readonly options?: Record<string, any>;
}

/**
 * Collection Statistics
 */
export interface CollectionStats {
  readonly name: string;
  readonly recordCount: number;
  readonly totalSize: number;
  readonly lastModified: string;
  readonly schema?: string;
  readonly indices?: string[];
}

/**
 * Batch Storage Operation
 */
export interface StorageOperation {
  readonly type: 'create' | 'read' | 'update' | 'delete';
  readonly collection: string;
  readonly id?: UUID;
  readonly data?: any;
  readonly query?: StorageQuery;
}

/**
 * Storage Adapter Factory for Plugin System
 */
export abstract class StorageAdapterFactory {
  abstract createAdapter(config: StorageAdapterConfig): DataStorageAdapter;
  abstract getSupportedTypes(): string[];
}