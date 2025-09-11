/**
 * Core Domain Types - Professional Data Architecture
 * 
 * Rust-like strict typing with branded types for type safety
 * Following the widget.executeCommand pattern for data operations
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// Branded types for complete type safety (no string confusion)
export type UserId = string & { readonly __brand: 'UserId' };
export type CitizenId = string & { readonly __brand: 'CitizenId' };
export type MessageId = string & { readonly __brand: 'MessageId' };
export type RoomId = string & { readonly __brand: 'RoomId' };
export type PersonaId = string & { readonly __brand: 'PersonaId' };
export type SessionId = string & { readonly __brand: 'SessionId' };
export type ISOString = string & { readonly __brand: 'ISOString' };

// Branded type constructors
export const UserId = (id: string): UserId => id as UserId;
export const CitizenId = (id: string): CitizenId => id as CitizenId;
export const MessageId = (id: string): MessageId => id as MessageId;
export const RoomId = (id: string): RoomId => id as RoomId;
export const PersonaId = (id: string): PersonaId => id as PersonaId;
export const SessionId = (id: string): SessionId => id as SessionId;
export const ISOString = (timestamp: string): ISOString => timestamp as ISOString;

/**
 * Base Entity - All domain objects extend this
 */
export interface BaseEntity {
  readonly id: UUID;
  readonly createdAt: ISOString;
  readonly updatedAt: ISOString;
  readonly version: number;
}

/**
 * Result Type - Rust-inspired error handling (no throwing)
 */
export type DataResult<T, E = DataError> = 
  | { readonly success: true; readonly data: T }
  | { readonly success: false; readonly error: E };

/**
 * Data Operation Context - Audit trail and traceability
 */
export interface DataOperationContext {
  readonly sessionId: SessionId;
  readonly timestamp: ISOString;
  readonly source: string;
  readonly userId?: UserId;
  readonly transactionId?: UUID;
}

/**
 * Data Errors - Exhaustive union for proper error handling
 */
export interface DataError {
  readonly type: 'NOT_FOUND' | 'VALIDATION_ERROR' | 'PERMISSION_DENIED' | 'CONFLICT' | 'STORAGE_ERROR' | 'NETWORK_ERROR';
  readonly message: string;
  readonly details?: Record<string, unknown>;
  readonly code?: string;
}

/**
 * Query Options - Type-safe querying
 */
export interface QueryOptions<T extends BaseEntity> {
  readonly limit?: number;
  readonly offset?: number;
  readonly orderBy?: Array<{
    readonly field: keyof T;
    readonly direction: 'ASC' | 'DESC';
  }>;
  readonly filters?: Record<keyof T, unknown>;
}

/**
 * Data Operation Types - Like widget.executeCommand but for data
 */
export type DataOperation = 
  | 'create'
  | 'read'
  | 'update' 
  | 'delete'
  | 'list'
  | 'query'
  | 'count';

/**
 * Helper functions for Result type (Rust-inspired)
 */
export const Ok = <T>(data: T): DataResult<T, never> => ({ success: true, data });
export const Err = <E>(error: E): DataResult<never, E> => ({ success: false, error });

/**
 * Error constructors
 */
export const createDataError = (
  type: DataError['type'],
  message: string,
  details?: Record<string, unknown>,
  code?: string
): DataError => ({ type, message, details, code });

export const NotFoundError = (resource: string, id: string): DataError =>
  createDataError('NOT_FOUND', `${resource} not found`, { id }, 'RESOURCE_NOT_FOUND');

export const ValidationError = (field: string, reason: string): DataError =>
  createDataError('VALIDATION_ERROR', `Validation failed for ${field}: ${reason}`, { field }, 'VALIDATION_FAILED');

export const ConflictError = (resource: string, conflict: string): DataError =>
  createDataError('CONFLICT', `${resource} conflict: ${conflict}`, { resource }, 'RESOURCE_CONFLICT');