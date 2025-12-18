/**
 * Data List Command - Shared Types
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

export interface DataListParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  // Collection name - comes from Entity.collection static property
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
  // Field projection - if specified, only return these fields (plus 'id' always)
  // If omitted, returns all fields (SELECT *) for backward compatibility
  readonly fields?: readonly string[];
  // Verbose flag - if false, returns only id + description field (lean mode)
  // Default: undefined (returns all fields for backward compatibility)
  readonly verbose?: boolean;
  /** Optional database handle for multi-database operations (defaults to 'default') */
  readonly dbHandle?: DbHandle;
}

export interface DataListResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly collection: string;
  readonly count: number;
  readonly timestamp: string;
  readonly error?: string;
}

export const createDataListParams = <T extends BaseEntity>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataListParams<T>, 'context' | 'sessionId'>
): DataListParams<T> => createPayload(context, sessionId, data);

export const createDataListResultFromParams = <T extends BaseEntity>(
  params: DataListParams<T>,
  differences: Omit<Partial<DataListResult<T>>, 'context' | 'sessionId'>
): DataListResult<T> => transformPayload(params, {
  success: false,
  items: [],
  collection: params.collection,
  count: 0,
  timestamp: new Date().toISOString(),
  ...differences
});