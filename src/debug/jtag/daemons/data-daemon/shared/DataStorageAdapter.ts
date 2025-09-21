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
 * Type alias for structured record data that can be indexed by field names
 * This is our primary constraint for data content stored in the system
 */
export type RecordData = Record<string, unknown>;

/**
 * Universal Data Record
 */
export interface DataRecord<T extends RecordData = RecordData> {
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
  readonly filters?: RecordData;
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
 * Storage Adapter Capabilities
 */
export interface StorageCapabilities {
  readonly supportsTransactions: boolean;
  readonly supportsIndexing: boolean;
  readonly supportsFullTextSearch: boolean;
  readonly supportsReplication: boolean;
  readonly maxRecordSize: number;
  readonly concurrentConnections: number;
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
  abstract create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>>;

  /**
   * Read a single record by ID
   */
  abstract read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>>;

  /**
   * Query records with complex filters
   */
  abstract query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>>;

  /**
   * Update an existing record
   */
  abstract update<T extends RecordData>(collection: string, id: UUID, data: Partial<T>, incrementVersion?: boolean): Promise<StorageResult<DataRecord<T>>>;
  
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
  abstract batch<T extends RecordData = RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>>;
  
  /**
   * Clear all data from all collections
   */
  abstract clear(): Promise<StorageResult<boolean>>;

  /**
   * Truncate all records from a specific collection
   */
  abstract truncate(collection: string): Promise<StorageResult<boolean>>;

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
  readonly options?: RecordData;
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
export interface StorageOperation<T extends RecordData = RecordData> {
  readonly type: 'create' | 'read' | 'update' | 'delete';
  readonly collection: string;
  readonly id?: UUID;
  readonly data?: T | Partial<T>;
  readonly query?: StorageQuery;
}

/**
 * Storage Adapter Factory for Plugin System
 */
export abstract class StorageAdapterFactory {
  abstract createAdapter(config: StorageAdapterConfig): DataStorageAdapter;
  abstract getSupportedTypes(): string[];
}