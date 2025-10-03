/**
 * Data Daemon - Shared Types
 * 
 * Modern JSON-based data persistence following ChatTypes pattern
 * No SQL - simple, efficient document store with strong typing
 */

import type { JTAGPayload, JTAGContext } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';

// ============================================================================
// DATA OPERATIONS
// ============================================================================

export type DataOperation = 'create' | 'read' | 'update' | 'delete' | 'list' | 'exists';

// ============================================================================
// DATA ENTITY TYPES
// ============================================================================

export interface DataRecord<T extends BaseEntity = BaseEntity> {
  readonly id: UUID;
  readonly collection: string;
  readonly data: T;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly version: number;
}

// ============================================================================
// DATA OPERATION PARAMETERS
// ============================================================================

export interface DataCreateParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly data: T;
  readonly id?: UUID;
}

export interface DataReadParams extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
}

export interface DataUpdateParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
  readonly data: Partial<T>;
  readonly merge?: boolean;
}

export interface DataDeleteParams extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
}

export interface DataListParams extends JTAGPayload {
  readonly collection: string;
  readonly filter?: Partial<BaseEntity>;
  readonly limit?: number;
  readonly offset?: number;
}

export interface DataExistsParams extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
}

// ============================================================================
// FACTORY FUNCTIONS FOR PARAMETERS
// ============================================================================

export const createDataCreateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataCreateParams, 'context' | 'sessionId'>
): DataCreateParams => createPayload(context, sessionId, data);

export const createDataReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataReadParams, 'context' | 'sessionId'>
): DataReadParams => createPayload(context, sessionId, data);

export const createDataUpdateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataUpdateParams, 'context' | 'sessionId'>
): DataUpdateParams => createPayload(context, sessionId, data);

export const createDataDeleteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataDeleteParams, 'context' | 'sessionId'>
): DataDeleteParams => createPayload(context, sessionId, data);

export const createDataListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataListParams, 'context' | 'sessionId'>
): DataListParams => createPayload(context, sessionId, data);

export const createDataExistsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataExistsParams, 'context' | 'sessionId'>
): DataExistsParams => createPayload(context, sessionId, data);

// ============================================================================
// DATA OPERATION RESULTS
// ============================================================================

export interface DataCreateResult extends JTAGPayload {
  readonly success: boolean;
  readonly record: DataRecord;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface DataReadResult extends JTAGPayload {
  readonly success: boolean;
  readonly record?: DataRecord;
  readonly found: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface DataUpdateResult extends JTAGPayload {
  readonly success: boolean;
  readonly record: DataRecord;
  readonly updated: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface DataDeleteResult extends JTAGPayload {
  readonly success: boolean;
  readonly deleted: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface DataListResult extends JTAGPayload {
  readonly success: boolean;
  readonly records: readonly DataRecord[];
  readonly totalCount: number;
  readonly hasMore: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface DataExistsResult extends JTAGPayload {
  readonly success: boolean;
  readonly exists: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

// ============================================================================
// FACTORY FUNCTIONS FOR RESULTS
// ============================================================================

export const createDataCreateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataCreateResult>, 'context' | 'sessionId'>
): DataCreateResult => createPayload(context, sessionId, {
  success: false,
  record: {} as DataRecord,
  timestamp: new Date().toISOString(),
  ...data
});

export const createDataReadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataReadResult>, 'context' | 'sessionId'>
): DataReadResult => createPayload(context, sessionId, {
  success: false,
  found: false,
  timestamp: new Date().toISOString(),
  ...data
});

export const createDataUpdateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataUpdateResult>, 'context' | 'sessionId'>
): DataUpdateResult => createPayload(context, sessionId, {
  success: false,
  record: {} as DataRecord,
  updated: false,
  timestamp: new Date().toISOString(),
  ...data
});

export const createDataDeleteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataDeleteResult>, 'context' | 'sessionId'>
): DataDeleteResult => createPayload(context, sessionId, {
  success: false,
  deleted: false,
  timestamp: new Date().toISOString(),
  ...data
});

export const createDataListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataListResult>, 'context' | 'sessionId'>
): DataListResult => createPayload(context, sessionId, {
  success: false,
  records: [],
  totalCount: 0,
  hasMore: false,
  timestamp: new Date().toISOString(),
  ...data
});

export const createDataExistsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataExistsResult>, 'context' | 'sessionId'>
): DataExistsResult => createPayload(context, sessionId, {
  success: false,
  exists: false,
  timestamp: new Date().toISOString(),
  ...data
});

// ============================================================================
// DATA RESPONSE UNION TYPE
// ============================================================================

export type DataResult = 
  | DataCreateResult 
  | DataReadResult 
  | DataUpdateResult 
  | DataDeleteResult 
  | DataListResult
  | DataExistsResult;

export type DataParams = 
  | DataCreateParams 
  | DataReadParams 
  | DataUpdateParams 
  | DataDeleteParams 
  | DataListParams
  | DataExistsParams;