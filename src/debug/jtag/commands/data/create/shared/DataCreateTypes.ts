/**
 * Data Create Command - Shared Types
 *
 * Clean command interface following screenshot/file-save patterns
 */

import type { JTAGContext, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { BaseDataParams, BaseDataResult } from '../../shared/BaseDataTypes';
import { createBaseDataParams } from '../../shared/BaseDataTypes';

/**
 * Data Create Parameters - extends base data params
 */
export interface DataCreateParams<T extends BaseEntity = BaseEntity> extends BaseDataParams {
  readonly data: T;
}

/**
 * Data Create Result - extends base data result
 */
export interface DataCreateResult<T extends BaseEntity = BaseEntity> extends BaseDataResult {
  readonly data?: T;
}

/**
 * Factory function for creating DataCreateParams
 */
export const createDataCreateParams = <T extends BaseEntity = BaseEntity>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataCreateParams<T>, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): DataCreateParams<T> => {
  // Use base factory to ensure backend defaults are applied
  const baseParams = createBaseDataParams(context, sessionId, {
    collection: data.collection,
    backend: data.backend
  });

  // Return with data property added
  return {
    ...baseParams,
    data: data.data
  };
};

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