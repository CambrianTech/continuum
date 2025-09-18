/**
 * Data Create Command - Shared Types
 * 
 * Clean command interface following screenshot/file-save patterns
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/domains/CoreTypes';

/**
 * Data Create Parameters
 */
export interface DataCreateParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly data: Omit<T, keyof BaseEntity>; // Adapter adds BaseEntity fields automatically
  readonly id?: UUID;
}

/**
 * Data Create Result
 */
export interface DataCreateResult<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly id?: UUID;
  readonly data?: T;
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