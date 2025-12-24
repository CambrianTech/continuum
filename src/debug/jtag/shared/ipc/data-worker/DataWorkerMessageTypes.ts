/**
 * Data Worker - Worker-Specific Message Types
 *
 * Protocol types for Rust data worker communication.
 * These types will be exported from Rust via ts-rs to ensure type safety.
 *
 * ARCHITECTURE:
 * - TypeScript: Orchestration, validation, decorators, events
 * - Rust Worker: I/O, connection pooling, storage detection, massive concurrency
 * - Communication: Unix domain socket (low overhead, high throughput)
 *
 * USAGE:
 * ```typescript
 * const request: WorkerRequest<OpenDatabaseRequest> = {
 *   id: uuid(),
 *   type: 'open-database',
 *   timestamp: new Date().toISOString(),
 *   payload: {
 *     filename: '/Users/joel/.continuum/data/database.sqlite',
 *     storageType: 'auto-detect'
 *   }
 * };
 * ```
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type {
  DataRecord,
  StorageQuery,
  CollectionStats,
  RecordData,
  QueryExplanation
} from '../../../daemons/data-daemon/shared/DataStorageAdapter';

// ============================================================================
// Database Handle Management
// ============================================================================

/**
 * Storage type detected by Rust worker
 */
export type StorageType = 'internal-ssd' | 'external-ssd' | 'sd-card' | 'unknown';

/**
 * Request to open a new database handle
 */
export interface OpenDatabaseRequest {
  filename: string;                          // Database file path
  adapterType: 'sqlite' | 'json';            // Adapter type (expandable)
  storageType?: 'auto-detect' | StorageType; // Storage detection override
  mode?: 'readonly' | 'readwrite' | 'create'; // Open mode
}

/**
 * Response with database handle ID
 */
export interface OpenDatabaseResponse {
  handle: string;                            // UUID handle for this database
  storageType: StorageType;                  // Detected storage type
  pragmaMode: 'WAL' | 'DELETE';              // SQLite journal mode used
  poolSize: number;                          // Connection pool size
}

/**
 * Request to close a database handle
 */
export interface CloseDatabaseRequest {
  handle: string;                            // UUID handle to close
}

/**
 * Response confirming database closed
 */
export interface CloseDatabaseResponse {
  success: boolean;
}

// ============================================================================
// CRUD Operations
// ============================================================================

/**
 * Request to create a record
 */
export interface CreateRecordRequest<T extends RecordData = RecordData> {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
  record: DataRecord<T>;                     // Complete record with metadata
}

/**
 * Response with created record
 */
export interface CreateRecordResponse<T extends RecordData = RecordData> {
  record: DataRecord<T>;                     // Created record
}

/**
 * Request to read a single record by ID
 */
export interface ReadRecordRequest {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
  id: UUID;                                  // Record ID
}

/**
 * Response with record data
 */
export interface ReadRecordResponse<T extends RecordData = RecordData> {
  record?: DataRecord<T>;                    // Record data (undefined if not found)
}

/**
 * Request to query records with filters
 */
export interface QueryRecordsRequest {
  handle: string;                            // Database handle
  query: StorageQuery;                       // Query with filters, sorting, pagination
}

/**
 * Response with query results
 */
export interface QueryRecordsResponse<T extends RecordData = RecordData> {
  records: DataRecord<T>[];                  // Matching records
  totalCount?: number;                       // Total count (if requested)
  queryTime?: number;                        // Query execution time in ms
}

/**
 * Request to update a record
 */
export interface UpdateRecordRequest<T extends RecordData = RecordData> {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
  id: UUID;                                  // Record ID
  data: Partial<T>;                          // Fields to update
  incrementVersion?: boolean;                // Whether to increment version
}

/**
 * Response with updated record
 */
export interface UpdateRecordResponse<T extends RecordData = RecordData> {
  record: DataRecord<T>;                     // Updated record
}

/**
 * Request to delete a record
 */
export interface DeleteRecordRequest {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
  id: UUID;                                  // Record ID
}

/**
 * Response confirming deletion
 */
export interface DeleteRecordResponse {
  success: boolean;
}

// ============================================================================
// Schema and Collection Management
// ============================================================================

/**
 * Request to ensure collection schema exists
 */
export interface EnsureSchemaRequest {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
  schema?: unknown;                          // Optional schema metadata
}

/**
 * Response confirming schema exists
 */
export interface EnsureSchemaResponse {
  success: boolean;
}

/**
 * Request to list all collections
 */
export interface ListCollectionsRequest {
  handle: string;                            // Database handle
}

/**
 * Response with collection names
 */
export interface ListCollectionsResponse {
  collections: string[];
}

/**
 * Request to get collection statistics
 */
export interface GetCollectionStatsRequest {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
}

/**
 * Response with collection statistics
 */
export interface GetCollectionStatsResponse {
  stats: CollectionStats;
}

/**
 * Request to truncate a collection
 */
export interface TruncateCollectionRequest {
  handle: string;                            // Database handle
  collection: string;                        // Collection/table name
}

/**
 * Response confirming truncation
 */
export interface TruncateCollectionResponse {
  success: boolean;
}

/**
 * Request to clear all collections
 */
export interface ClearAllRequest {
  handle: string;                            // Database handle
}

/**
 * Response with clear statistics
 */
export interface ClearAllResponse {
  tablesCleared: string[];
  recordsDeleted: number;
}

// ============================================================================
// Query Analysis
// ============================================================================

/**
 * Request to explain query execution (dry-run)
 */
export interface ExplainQueryRequest {
  handle: string;                            // Database handle
  query: StorageQuery;                       // Query to explain
}

/**
 * Response with query explanation
 */
export interface ExplainQueryResponse {
  explanation: QueryExplanation;
}

// ============================================================================
// Health Check
// ============================================================================

/**
 * Request to ping worker (health check)
 */
export interface PingRequest {}

/**
 * Response with worker health statistics
 */
export interface PingResponse {
  uptimeMs: number;
  activeHandles: number;
  totalHandles: number;
  requestsProcessed: number;
  avgResponseTimeMs: number;
}
