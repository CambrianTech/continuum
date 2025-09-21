/**
 * Data Clear Command - Shared Types
 *
 * Clear all data from all collections using adapter methods
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Data Clear Parameters
 */
export interface DataClearParams extends JTAGPayload {
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
  data: Omit<DataClearParams, 'context' | 'sessionId'> = {}
): DataClearParams => createPayload(context, sessionId, data);

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