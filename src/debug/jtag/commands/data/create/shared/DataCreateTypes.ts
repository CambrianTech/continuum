/**
 * Data Create Command - Shared Types
 *
 * Clean command interface following screenshot/file-save patterns
 */

import type { JTAGContext, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { BaseDataParams, BaseDataResult, DataCommandInput } from '../../shared/BaseDataTypes';
import { createBaseDataParams } from '../../shared/BaseDataTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/** Data create command parameters */
export interface DataCreateParams extends BaseDataParams {
  /** Entity data to create */
  readonly data: Record<string, any>;
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
export const createDataCreateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataCreateParams, 'context' | 'sessionId' | 'backend' | 'userId'> & { backend?: JTAGEnvironment }
): DataCreateParams => {
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

/**
 * DataCreate — Type-safe command executor
 *
 * Usage:
 *   import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
 *   const result = await DataCreate.execute({ collection: 'users', data: entity });
 */
export const DataCreate = {
  execute<T extends BaseEntity = BaseEntity>(params: DataCommandInput<DataCreateParams>): Promise<DataCreateResult<T>> {
    return Commands.execute<DataCreateParams, DataCreateResult<T>>('data/create', params as Partial<DataCreateParams>);
  },
  commandName: 'data/create' as const,
} as const;