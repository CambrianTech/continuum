/**
 * Data Truncate Command - Shared Types
 *
 * Truncate all records from a specific collection using adapter methods
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Data Truncate Parameters
 */
export interface DataTruncateParams extends JTAGPayload {
  readonly collection: string; // Collection to truncate
}

/**
 * Data Truncate Result
 */
export interface DataTruncateResult extends JTAGPayload {
  readonly success: boolean;
  readonly collection: string;
  readonly recordsDeleted?: number;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating DataTruncateParams
 */
export const createDataTruncateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataTruncateParams, 'context' | 'sessionId'>
): DataTruncateParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataTruncateResultFromParams = (
  params: DataTruncateParams,
  differences: Omit<Partial<DataTruncateResult>, 'context' | 'sessionId'>
): DataTruncateResult => transformPayload(params, {
  success: false,
  collection: params.collection,
  timestamp: new Date().toISOString(),
  ...differences
});