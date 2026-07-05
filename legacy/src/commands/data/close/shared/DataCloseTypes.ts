/**
 * Data Close Command - Shared Types
 *
 * Closes a database handle and releases associated resources.
 * Cannot close the default handle - it remains open for the lifetime of the process.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Data Close Parameters
 */
export interface DataCloseParams extends CommandParams {
  // Database handle to close (cannot be 'default')
  readonly dbHandle: DbHandle;
}

/**
 * Data Close Result
 */
export interface DataCloseResult extends JTAGPayload {
  readonly success: boolean;
  readonly dbHandle: DbHandle;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating data/close params
 */
export const createDataCloseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataCloseParams, 'context' | 'sessionId'>
): DataCloseParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataCloseResultFromParams = (
  params: DataCloseParams,
  differences: Omit<Partial<DataCloseResult>, 'context' | 'sessionId'>
): DataCloseResult => transformPayload(params, {
  success: false,
  dbHandle: params.dbHandle,
  timestamp: new Date().toISOString(),
  ...differences
});

// Re-export DbHandle type for convenience
export type { DbHandle };

/**
 * DataClose — Type-safe command executor
 *
 * Usage:
 *   import { DataClose } from '...shared/DataCloseTypes';
 *   const result = await DataClose.execute({ ... });
 */
export const DataClose = {
  execute(params: CommandInput<DataCloseParams>): Promise<DataCloseResult> {
    return Commands.execute<DataCloseParams, DataCloseResult>('data/close', params as Partial<DataCloseParams>);
  },
  commandName: 'data/close' as const,
} as const;
