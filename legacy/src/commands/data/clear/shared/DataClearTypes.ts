/**
 * Data Clear Command - Shared Types
 *
 * Clear all data from all collections using adapter methods
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Data Clear Parameters
 */
export interface DataClearParams extends CommandParams {
  // No additional parameters needed - clear all collections
}

/**
 * Data Clear Result
 */
export interface DataClearResult extends JTAGPayload {
  readonly success: boolean;
  readonly collectionsCleared?: number;
  readonly recordsCleared?: number;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating DataClearParams
 */
export const createDataClearParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataClearParams, 'context' | 'sessionId' | 'userId'> = {}
): DataClearParams => createPayload(context, sessionId, { userId: SYSTEM_SCOPES.SYSTEM, ...data });

/**
 * Transform params to result
 */
export const createDataClearResultFromParams = (
  params: DataClearParams,
  differences: Omit<Partial<DataClearResult>, 'context' | 'sessionId'>
): DataClearResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});
/**
 * DataClear — Type-safe command executor
 *
 * Usage:
 *   import { DataClear } from '...shared/DataClearTypes';
 *   const result = await DataClear.execute({ ... });
 */
export const DataClear = {
  execute(params: CommandInput<DataClearParams>): Promise<DataClearResult> {
    return Commands.execute<DataClearParams, DataClearResult>('data/clear', params as Partial<DataClearParams>);
  },
  commandName: 'data/clear' as const,
} as const;
