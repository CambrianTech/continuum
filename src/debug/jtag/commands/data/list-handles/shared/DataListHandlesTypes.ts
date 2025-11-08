/**
 * Data List-Handles Command - Shared Types
 *
 * Lists all open database handles with metadata.
 * Useful for debugging, monitoring, and handle management.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type {
  DbHandle,
  AdapterType,
  AdapterConfig
} from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

/**
 * Data List-Handles Parameters
 */
export interface DataListHandlesParams extends JTAGPayload {
  // No additional parameters needed - lists all handles
}

/**
 * Handle info returned by list-handles command
 */
export interface HandleInfo {
  readonly handle: DbHandle;
  readonly adapter: AdapterType;
  readonly config: AdapterConfig;
  readonly isDefault: boolean;
  readonly openedAt: number;
  readonly lastUsedAt: number;
}

/**
 * Data List-Handles Result
 */
export interface DataListHandlesResult extends JTAGPayload {
  readonly success: boolean;
  readonly handles: readonly HandleInfo[];
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating data/list-handles params
 */
export const createDataListHandlesParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataListHandlesParams, 'context' | 'sessionId'>
): DataListHandlesParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataListHandlesResultFromParams = (
  params: DataListHandlesParams,
  differences: Omit<Partial<DataListHandlesResult>, 'context' | 'sessionId'>
): DataListHandlesResult => transformPayload(params, {
  success: false,
  handles: [],
  timestamp: new Date().toISOString(),
  ...differences
});

// Re-export types for convenience
export type { DbHandle, AdapterType, AdapterConfig };
