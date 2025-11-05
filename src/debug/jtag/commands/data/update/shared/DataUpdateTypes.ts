/**
 * Data Update Command Types - Universal Update Interface
 *
 * Follows working data create pattern with strong typing
 */

import type { JTAGContext, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { BaseDataParams, BaseDataResult } from '../../shared/BaseDataTypes';
import { createBaseDataParams } from '../../shared/BaseDataTypes';

/**
 * Data Update Parameters
 */
export interface DataUpdateParams<T extends BaseEntity = BaseEntity> extends BaseDataParams {
  readonly id: UUID;
  readonly data: Partial<T>;
  readonly format?: 'json' | 'yaml' | 'table';
  readonly incrementVersion?: boolean;
}

/**
 * Data Update Result
 */
export interface DataUpdateResult<T extends BaseEntity = BaseEntity> extends BaseDataResult {
  readonly id: UUID;
  readonly found: boolean;
  readonly data?: T;
  readonly previousVersion?: number;
  readonly newVersion?: number;
}

/**
 * Factory function for creating DataUpdateParams
 */
export const createDataUpdateParams = <T extends BaseEntity = BaseEntity>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataUpdateParams<T>, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): DataUpdateParams<T> => {
  const baseParams = createBaseDataParams(context, sessionId, {
    collection: data.collection,
    backend: data.backend
  });

  return {
    ...baseParams,
    id: data.id,
    data: data.data,
    format: data.format,
    incrementVersion: data.incrementVersion
  };
};

/**
 * Transform params to result
 */
export const createDataUpdateResultFromParams = (
  params: DataUpdateParams,
  differences: Omit<Partial<DataUpdateResult>, 'context' | 'sessionId'>
): DataUpdateResult => transformPayload(params, {
  success: false,
  found: false,
  id: params.id,
  timestamp: new Date().toISOString(),
  ...differences
});