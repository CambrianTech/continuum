/**
 * Data Query Open Command Types
 *
 * Opens a paginated query and returns a handle (UUID)
 * DataDaemon maintains the query state (filters, sorting, cursor position)
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

export interface DataQueryOpenParams extends CommandParams {
  readonly collection: string;
  readonly backend: JTAGEnvironment;
  readonly dbHandle: DbHandle;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize?: number;
}

export interface DataQueryOpenResult extends JTAGPayload {
  readonly success: boolean;
  readonly queryHandle: UUID; // Opaque handle for subsequent nextPage calls
  readonly totalCount: number;
  readonly pageSize: number;
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}

/**
 * DataQueryOpen — Type-safe command executor
 *
 * Usage:
 *   import { DataQueryOpen } from '...shared/DataQueryOpenTypes';
 *   const result = await DataQueryOpen.execute({ ... });
 */
export const DataQueryOpen = {
  execute(params: CommandInput<DataQueryOpenParams>): Promise<DataQueryOpenResult> {
    return Commands.execute<DataQueryOpenParams, DataQueryOpenResult>('data/query-open', params as Partial<DataQueryOpenParams>);
  },
  commandName: 'data/query-open' as const,
} as const;

/**
 * Factory function for creating DataQueryOpenParams
 */
export const createDataQueryOpenParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataQueryOpenParams, 'context' | 'sessionId' | 'userId'>
): DataQueryOpenParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating DataQueryOpenResult with defaults
 */
export const createDataQueryOpenResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataQueryOpenResult, 'context' | 'sessionId' | 'userId'>
): DataQueryOpenResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart data/query-open-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDataQueryOpenResultFromParams = (
  params: DataQueryOpenParams,
  differences: Omit<DataQueryOpenResult, 'context' | 'sessionId' | 'userId'>
): DataQueryOpenResult => transformPayload(params, differences);

