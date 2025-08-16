/**
 * Data Create Command - Shared Types
 * 
 * Clean command interface following screenshot/file-save patterns
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Data Create Parameters
 */
export interface DataCreateParams extends JTAGPayload {
  readonly collection: string;
  readonly data: any;
  readonly id?: UUID;
}

/**
 * Data Create Result
 */
export interface DataCreateResult extends JTAGPayload {
  readonly success: boolean;
  readonly id?: UUID;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating DataCreateParams
 */
export const createDataCreateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataCreateParams, 'context' | 'sessionId'>
): DataCreateParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataCreateResultFromParams = (
  params: DataCreateParams,
  differences: Omit<Partial<DataCreateResult>, 'context' | 'sessionId'>
): DataCreateResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});