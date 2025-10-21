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
 * Primitive values that can be compared and filtered
 */
export type ComparableValue = string | number | boolean | Date;

/**
 * Universal Query Operators - Database-agnostic filtering
 */
export interface QueryOperators {
  $eq?: ComparableValue | null;           // Equal to
  $ne?: ComparableValue | null;           // Not equal to
  $gt?: ComparableValue;                  // Greater than
  $gte?: ComparableValue;                 // Greater than or equal
  $lt?: ComparableValue;                  // Less than
  $lte?: ComparableValue;                 // Less than or equal
  $in?: ComparableValue[];                // In array
  $nin?: ComparableValue[];               // Not in array
  $exists?: boolean;                      // Field exists
  $regex?: string;                        // Regular expression match
  $contains?: string;                     // String contains (case insensitive)
}

/**
 * Field filter - direct value (implies $eq) or operator object
 */
export type FieldFilter = ComparableValue | null | QueryOperators;

/**
 * Universal filter object - maps field names to filters
 */
export interface UniversalFilter {
  [fieldName: string]: FieldFilter;
}

/**
 * Enhanced Storage Query Interface - Universal Filter System
 */
export interface StorageQuery {
  readonly collection: string;
  readonly filter?: UniversalFilter; // Universal filter system with operator support
  readonly sort?: { field: string; direction: 'asc' | 'desc' }[];
  readonly limit?: number;
  readonly offset?: number;
  readonly cursor?: {
    field: string;
    value: ComparableValue;
    direction: 'before' | 'after';
  };
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
export interface StorageResult<T = unknown> {
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
 * Query Explanation Result - For dry-run/debugging
 */
export interface QueryExplanation {
  readonly query: StorageQuery;
  readonly translatedQuery: string;
  readonly parameters?: readonly unknown[];
  readonly estimatedRows?: number;
  readonly executionPlan?: string;
  readonly adapterType: string;
  readonly timestamp: string;
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
   * Clear all data from all collections with detailed reporting
   *
   * Provides comprehensive information about what was cleared,
   * preserving database structure while removing all records.
   * Useful for reseeding operations and development workflows.
   */
  abstract clearAll(): Promise<StorageResult<{
    tablesCleared: string[];
    recordsDeleted: number;
  }>>;

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

  /**
   * Explain query execution (dry-run) - shows what query would be generated
   */
  abstract explainQuery(query: StorageQuery): Promise<QueryExplanation>;
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