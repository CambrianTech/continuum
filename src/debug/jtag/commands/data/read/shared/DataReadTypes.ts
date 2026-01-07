/**
 * Data Read Command - Shared Types
 */

import type { JTAGContext, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { BaseDataParams, BaseDataResult } from '../../shared/BaseDataTypes';
import { createBaseDataParams } from '../../shared/BaseDataTypes';
import type { MediaItem } from '../../../../system/data/entities/ChatMessageEntity';

/** Data read command parameters */
export interface DataReadParams extends BaseDataParams {
  /** ID of entity to read */
  readonly id: UUID;
}

export interface DataReadResult<T extends BaseEntity = BaseEntity> extends BaseDataResult {
  readonly data?: T;
  readonly found: boolean;
  readonly id: UUID;
  readonly media?: MediaItem[];
}

export const createDataReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataReadParams, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): DataReadParams => {
  const baseParams = createBaseDataParams(context, sessionId, {
    collection: data.collection,
    backend: data.backend
  });

  return {
    ...baseParams,
    id: data.id
  };
};

export const createDataReadResultFromParams = (
  params: DataReadParams,
  differences: Omit<Partial<DataReadResult>, 'context' | 'sessionId'>
): DataReadResult => transformPayload(params, {
  success: false,
  found: false,
  id: params.id,
  media: [],
  timestamp: new Date().toISOString(),
  ...differences
});