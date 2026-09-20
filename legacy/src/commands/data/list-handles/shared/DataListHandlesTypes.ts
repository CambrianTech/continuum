/**
 * Data List-Handles Command - Shared Types
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type {
  DbHandle,
  AdapterType,
  AdapterConfig
} from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Data List-Handles Parameters
 */
export interface DataListHandlesParams extends CommandParams {
  // No additional parameters needed
}

/**
 * Handle info returned by data/list-handles
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

/**
 * DataListHandles — Type-safe command executor
 *
 * Usage:
 *   import { DataListHandles } from '...shared/DataListHandlesTypes';
 *   const result = await DataListHandles.execute({ ... });
 */
export const DataListHandles = {
  execute(params: CommandInput<DataListHandlesParams>): Promise<DataListHandlesResult> {
    return Commands.execute<DataListHandlesParams, DataListHandlesResult>('data/list-handles', params as Partial<DataListHandlesParams>);
  },
  commandName: 'data/list-handles' as const,
} as const;
