/**
 * Data Read Command - Read a single entity by ID
 *
 * Common collections: users, rooms, chat_messages, memories, tasks, skills, wall_documents
 *
 * @example data/read --collection="users" --id="abc-123"
 * @example data/read --collection="chat_messages" --id="msg-456"
 */

import type { JTAGContext, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { BaseDataParams, BaseDataResult, DataCommandInput } from '../../shared/BaseDataTypes';
import { createBaseDataParams } from '../../shared/BaseDataTypes';
import { Commands } from '../../../../system/core/shared/Commands';
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
  data: Omit<DataReadParams, 'context' | 'sessionId' | 'backend' | 'userId'> & { backend?: JTAGEnvironment }
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

/**
 * DataRead — Type-safe command executor
 *
 * Usage:
 *   import { DataRead } from '@commands/data/read/shared/DataReadTypes';
 *   const result = await DataRead.execute({ collection: 'users', id: userId });
 */
export const DataRead = {
  execute<T extends BaseEntity = BaseEntity>(params: DataCommandInput<DataReadParams>): Promise<DataReadResult<T>> {
    return Commands.execute<DataReadParams, DataReadResult<T>>('data/read', params as Partial<DataReadParams>);
  },
  commandName: 'data/read' as const,
} as const;