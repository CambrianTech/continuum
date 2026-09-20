/**
 * Data Query Close Command Types
 *
 * Closes a query handle and frees resources
 * Should be called when done with pagination
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

export interface DataQueryCloseParams extends CommandParams {
  readonly queryHandle: UUID;
  readonly collection: string; // Required by BaseDataParams
  readonly backend: JTAGEnvironment; // Required by BaseDataParams
  readonly dbHandle: DbHandle;
}

export interface DataQueryCloseResult extends JTAGPayload {
  readonly success: boolean;
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}

/**
 * DataQueryClose — Type-safe command executor
 *
 * Usage:
 *   import { DataQueryClose } from '...shared/DataQueryCloseTypes';
 *   const result = await DataQueryClose.execute({ ... });
 */
export const DataQueryClose = {
  execute(params: CommandInput<DataQueryCloseParams>): Promise<DataQueryCloseResult> {
    return Commands.execute<DataQueryCloseParams, DataQueryCloseResult>('data/query-close', params as Partial<DataQueryCloseParams>);
  },
  commandName: 'data/query-close' as const,
} as const;

/**
 * Factory function for creating DataQueryCloseParams
 */
export const createDataQueryCloseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataQueryCloseParams, 'context' | 'sessionId' | 'userId'>
): DataQueryCloseParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating DataQueryCloseResult with defaults
 */
export const createDataQueryCloseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataQueryCloseResult, 'context' | 'sessionId' | 'userId'>
): DataQueryCloseResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart data/query-close-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDataQueryCloseResultFromParams = (
  params: DataQueryCloseParams,
  differences: Omit<DataQueryCloseResult, 'context' | 'sessionId' | 'userId'>
): DataQueryCloseResult => transformPayload(params, differences);

