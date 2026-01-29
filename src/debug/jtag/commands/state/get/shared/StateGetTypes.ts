/**
 * State Get Command - Shared Types
 *
 * Follows data daemon command pattern for elegant entity state management
 */

import type { JTAGPayload, JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Commands } from '../../../../system/core/shared/Commands';

/** State get command parameters */
export interface StateGetParams extends CommandParams {
  /** Collection name to query */
  readonly collection: string;
  /** Maximum number of items to return */
  readonly limit?: number;
  /** Filter criteria */
  readonly filter?: Record<string, any>;
  /** Sort order */
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  /** User ID for context filtering */
  readonly userId?: UUID;
}

// Generic version for internal type safety (not exported for schema)
interface StateGetParamsGeneric<T extends BaseEntity = BaseEntity> extends StateGetParams {}

export interface StateGetResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly collection: string;
  readonly count: number;
  readonly timestamp: string;
  readonly error?: string;
}

export const createStateGetParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<StateGetParams, 'context' | 'sessionId'>
): StateGetParams => createPayload(context, sessionId, data);

export const createStateGetResult = <T extends BaseEntity>(
  params: StateGetParams,
  differences: Omit<Partial<StateGetResult<T>>, 'context' | 'sessionId'>
): StateGetResult<T> => transformPayload(params, {
  success: false,
  items: [],
  collection: params.collection,
  count: 0,
  timestamp: new Date().toISOString(),
  ...differences
});
/**
 * StateGet — Type-safe command executor
 *
 * Usage:
 *   import { StateGet } from '...shared/StateGetTypes';
 *   const result = await StateGet.execute({ ... });
 */
export const StateGet = {
  execute<T extends BaseEntity = BaseEntity>(params: CommandInput<StateGetParams>): Promise<StateGetResult<T>> {
    return Commands.execute<StateGetParams, StateGetResult<T>>('state/get', params as Partial<StateGetParams>);
  },
  commandName: 'state/get' as const,
} as const;
