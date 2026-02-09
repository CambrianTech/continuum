/**
 * Data Update Command Types - Universal Update Interface
 *
 * Follows working data create pattern with strong typing
 */

import type { JTAGContext, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { BaseDataParams, BaseDataResult, DataCommandInput } from '../../shared/BaseDataTypes';
import { createBaseDataParams } from '../../shared/BaseDataTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/** Data update command parameters */
export interface DataUpdateParams extends BaseDataParams {
  /** ID of entity to update */
  readonly id: UUID;
  /** Update data */
  readonly data: Record<string, any>;
  /** Output format */
  readonly format?: 'json' | 'yaml' | 'table';
  /** Increment version on update */
  readonly incrementVersion?: boolean;
  /** Suppress event emission (useful for bulk updates like seeding) */
  readonly suppressEvents?: boolean;
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
export const createDataUpdateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataUpdateParams, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): DataUpdateParams => {
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

/**
 * DataUpdate — Type-safe command executor
 *
 * Usage:
 *   import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
 *   const result = await DataUpdate.execute({ collection: 'users', id: userId, data: { name: 'New' } });
 */
export const DataUpdate = {
  execute<T extends BaseEntity = BaseEntity>(params: DataCommandInput<DataUpdateParams>): Promise<DataUpdateResult<T>> {
    return Commands.execute<DataUpdateParams, DataUpdateResult<T>>('data/update', params as Partial<DataUpdateParams>);
  },
  commandName: 'data/update' as const,
} as const;