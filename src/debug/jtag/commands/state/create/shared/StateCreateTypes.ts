/**
 * State Create Command - Shared Types
 *
 * Follows data daemon command pattern for elegant entity state management
 */

import type { JTAGPayload, JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Commands } from '../../../../system/core/shared/Commands';

/** State create command parameters */
export interface StateCreateParams extends CommandParams {
  /** Collection to create entity in */
  readonly collection: string;
  /** Entity data to create */
  readonly data: Record<string, any>;
  /** Optional explicit ID */
  readonly id?: UUID;
  /** User ID for context */
  readonly userId?: UUID;
}

export interface StateCreateResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly item?: T;
  readonly id?: UUID;
  readonly collection: string;
  readonly version?: number;
  readonly error?: string;
}

export function createStateCreateResult<T extends BaseEntity>(
  params: StateCreateParams,
  overrides: Partial<StateCreateResult<T>>
): StateCreateResult<T> {
  return transformPayload(params, {
    success: false,
    collection: params.collection,
    ...overrides
  });
}
/**
 * StateCreate — Type-safe command executor
 *
 * Usage:
 *   import { StateCreate } from '...shared/StateCreateTypes';
 *   const result = await StateCreate.execute({ ... });
 */
export const StateCreate = {
  execute<T extends BaseEntity = BaseEntity>(params: CommandInput<StateCreateParams>): Promise<StateCreateResult<T>> {
    return Commands.execute<StateCreateParams, StateCreateResult<T>>('state/create', params as Partial<StateCreateParams>);
  },
  commandName: 'state/create' as const,
} as const;
