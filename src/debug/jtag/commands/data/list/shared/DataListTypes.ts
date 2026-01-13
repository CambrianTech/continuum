/**
 * Data List Command - Shared Types
 */

import type { JTAGPayload, JTAGContext, CommandParams } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

/** Data list command parameters */
export interface DataListParams extends CommandParams {
  /** Collection name to list */
  readonly collection: string;
  /** Maximum items to return */
  readonly limit?: number;
  /** Filter criteria */
  readonly filter?: Record<string, any>;
  /** Sort order */
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  /** Convert raw data to domain objects */
  readonly convertToDomain?: boolean;
  /** Cursor for pagination */
  readonly cursor?: {
    readonly field: string;
    readonly value: any;
    readonly direction: 'before' | 'after';
  };
  /** Field projection - only return these fields */
  readonly fields?: readonly string[];
  /** Verbose mode - if false, returns lean data */
  readonly verbose?: boolean;
  /** Database handle for multi-database operations */
  readonly dbHandle?: DbHandle;
  /**
   * Backend preference for browser commands:
   * - 'server': Always fetch from server (use for real-time data)
   * - 'local': Only use localStorage, don't fall back to server
   * - 'auto': (default) Try local first, fall back to server if empty
   * - 'stale-while-revalidate': Return cached immediately, fetch fresh in background
   */
  readonly backend?: 'server' | 'local' | 'auto' | 'stale-while-revalidate';
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