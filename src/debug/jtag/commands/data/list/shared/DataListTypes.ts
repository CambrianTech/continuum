/**
 * Data List Command - Shared Types
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/domains/CoreTypes';

export interface DataListParams extends JTAGPayload {
  readonly collection: string;
  readonly limit?: number;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly convertToDomain?: boolean; // Optional: convert raw data to domain objects
  readonly cursor?: {
    readonly field: string;
    readonly value: any;
    readonly direction: 'before' | 'after';
  };
}

export interface DataListResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly collection: string;
  readonly count: number;
  readonly timestamp: string;
  readonly error?: string;
}

export const createDataListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataListParams, 'context' | 'sessionId'>
): DataListParams => createPayload(context, sessionId, data);

export const createDataListResultFromParams = <T extends BaseEntity>(
  params: DataListParams,
  differences: Omit<Partial<DataListResult<T>>, 'context' | 'sessionId'>
): DataListResult<T> => transformPayload(params, {
  success: false,
  items: [],
  collection: params.collection,
  count: 0,
  timestamp: new Date().toISOString(),
  ...differences
});