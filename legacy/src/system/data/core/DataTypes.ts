/**
 * Core Data Types - Rust-like strict typing for database abstraction
 * 
 * Following Rust principles:
 * - No 'any' types allowed
 * - Explicit error handling with Result<T, E>
 * - Strong typing with branded types
 * - Immutable data structures
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// Branded types for type safety (Rust-like)
export type TableName = string & { readonly __brand: 'TableName' };
export type FieldName = string & { readonly __brand: 'FieldName' };
export type DatabaseId = string & { readonly __brand: 'DatabaseId' };

// Create branded type constructors
export const TableName = (name: string): TableName => name as TableName;
export const FieldName = (name: string): FieldName => name as FieldName;
export const DatabaseId = (id: string): DatabaseId => id as DatabaseId;

// Result type (Rust-inspired error handling)
export type Result<T, E = DatabaseError> = 
  | { readonly success: true; readonly data: T }
  | { readonly success: false; readonly error: E };

// Database error types (exhaustive union)
export interface DatabaseError {
  readonly type: 'CONNECTION_FAILED' | 'QUERY_FAILED' | 'VALIDATION_ERROR' | 'NOT_FOUND' | 'CONSTRAINT_VIOLATION' | 'UNKNOWN_ERROR';
  readonly message: string;
  readonly details?: Record<string, unknown>;
}

// Base entity interface (all database entities must extend this)
export interface BaseEntity {
  readonly id: DatabaseId;
  readonly createdAt: string; // ISO timestamp
  readonly updatedAt: string; // ISO timestamp
  readonly version: number;   // Optimistic locking
}

// Query filter types (strongly typed, no 'any')
export interface QueryFilter<T extends BaseEntity> {
  readonly field: keyof T;
  readonly operator: 'equals' | 'contains' | 'startsWith' | 'greaterThan' | 'lessThan' | 'in' | 'notNull';
  readonly value: T[keyof T] | T[keyof T][];
}

// Query options (Rust-like explicit configuration)
export interface QueryOptions<T extends BaseEntity> {
  readonly limit?: number;
  readonly offset?: number;
  readonly orderBy?: Array<{
    readonly field: keyof T;
    readonly direction: 'ASC' | 'DESC';
  }>;
  readonly filters?: Array<QueryFilter<T>>;
}

// Database operation results (explicit success/error handling)
export interface CreateResult<T extends BaseEntity> {
  readonly entity: T;
  readonly wasCreated: boolean;
}

export interface UpdateResult<T extends BaseEntity> {
  readonly entity: T;
  readonly wasUpdated: boolean;
  readonly previousVersion: number;
}

export interface DeleteResult {
  readonly wasDeleted: boolean;
  readonly deletedId: DatabaseId;
}

export interface QueryResult<T extends BaseEntity> {
  readonly entities: readonly T[];
  readonly totalCount: number;
  readonly hasMore: boolean;
}

// Database adapter interface (strategy pattern)
export interface DatabaseAdapter {
  readonly name: string;
  readonly version: string;
  
  // Connection management
  connect(): Promise<Result<void>>;
  disconnect(): Promise<Result<void>>;
  isConnected(): boolean;
  
  // Schema operations
  createTable<T extends BaseEntity>(
    tableName: TableName,
    schema: DatabaseSchema<T>
  ): Promise<Result<void>>;
  
  dropTable(tableName: TableName): Promise<Result<void>>;
  
  // CRUD operations (strongly typed)
  create<T extends BaseEntity>(
    tableName: TableName,
    entity: Omit<T, 'id' | 'createdAt' | 'updatedAt' | 'version'>
  ): Promise<Result<CreateResult<T>>>;
  
  findById<T extends BaseEntity>(
    tableName: TableName,
    id: DatabaseId
  ): Promise<Result<T | null>>;
  
  findMany<T extends BaseEntity>(
    tableName: TableName,
    options?: QueryOptions<T>
  ): Promise<Result<QueryResult<T>>>;
  
  update<T extends BaseEntity>(
    tableName: TableName,
    id: DatabaseId,
    updates: Partial<Omit<T, 'id' | 'createdAt' | 'updatedAt' | 'version'>>,
    expectedVersion: number
  ): Promise<Result<UpdateResult<T>>>;
  
  delete(
    tableName: TableName,
    id: DatabaseId
  ): Promise<Result<DeleteResult>>;
  
  // Transaction support
  transaction<R>(
    operation: (adapter: DatabaseAdapter) => Promise<Result<R>>
  ): Promise<Result<R>>;
}

// Database schema definition (type-safe table definitions)
export interface DatabaseSchema<T extends BaseEntity> {
  readonly tableName: TableName;
  readonly fields: {
    readonly [K in keyof T]: FieldDefinition<T[K]>;
  };
  readonly indexes?: Array<{
    readonly name: string;
    readonly fields: Array<keyof T>;
    readonly unique?: boolean;
  }>;
  readonly constraints?: Array<{
    readonly name: string;
    readonly type: 'FOREIGN_KEY' | 'CHECK' | 'UNIQUE';
    readonly definition: string;
  }>;
}

// Field definition (type-safe column definitions)
export interface FieldDefinition<T> {
  readonly type: 'TEXT' | 'INTEGER' | 'REAL' | 'BOOLEAN' | 'TIMESTAMP' | 'JSON' | 'UUID';
  readonly nullable?: boolean;
  readonly unique?: boolean;
  readonly defaultValue?: T;
  readonly validation?: {
    readonly minLength?: number;
    readonly maxLength?: number;
    readonly pattern?: RegExp;
    readonly customValidator?: (value: T) => boolean;
  };
}

// Helper functions for Result type (Rust-inspired)
export const Ok = <T>(data: T): Result<T, never> => ({ success: true, data });
export const Err = <E>(error: E): Result<never, E> => ({ success: false, error });

// Error constructors
export const createDatabaseError = (
  type: DatabaseError['type'],
  message: string,
  details?: Record<string, unknown>
): DatabaseError => ({ type, message, details });