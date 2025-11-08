/**
 * Data Close Command - Shared Types
 *
 * Closes a database handle, releasing resources.
 * Cannot close the 'default' handle - it remains open for the process lifetime.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

/**
 * Data Close Parameters
 */
export interface DataCloseParams extends JTAGPayload {
  // Database handle to close (cannot be 'default')
  readonly dbHandle: DbHandle;
}

/**
 * Data Close Result
 */
export interface DataCloseResult extends JTAGPayload {
  readonly success: boolean;
  readonly dbHandle: DbHandle;  // The handle that was closed
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
