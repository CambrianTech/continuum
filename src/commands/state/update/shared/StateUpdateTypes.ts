/**
 * State Update Types - User-aware entity updates
 *
 * Provides elegant wrapper around data/update with automatic user context injection
 * Following the established state command pattern for simple delegation
 */

import type { JTAGPayload, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Commands } from '../../../../system/core/shared/Commands';

/** State update command parameters */
export interface StateUpdateParams extends CommandParams {
  /** Collection containing the entity */
  readonly collection: string;
  /** ID of entity to update */
  readonly id: UUID;
  /** Update data */
  readonly data: Record<string, any>;
}

export interface StateUpdateResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly item?: T;
  readonly id?: UUID;
  readonly collection: string;
  readonly version?: number;
  readonly error?: string;
}

export function createStateUpdateResult<T extends BaseEntity>(
  context: any,
  sessionId: string,
  overrides: Partial<StateUpdateResult<T>>
): StateUpdateResult<T> {
  return {
    context,
    sessionId,
    success: false,
    collection: '',
    ...overrides
  };
}
/**
 * StateUpdate — Type-safe command executor
 *
 * Usage:
 *   import { StateUpdate } from '...shared/StateUpdateTypes';
 *   const result = await StateUpdate.execute({ ... });
 */
export const StateUpdate = {
  execute<T extends BaseEntity = BaseEntity>(params: CommandInput<StateUpdateParams>): Promise<StateUpdateResult<T>> {
    return Commands.execute<StateUpdateParams, StateUpdateResult<T>>('state/update', params as Partial<StateUpdateParams>);
  },
  commandName: 'state/update' as const,
} as const;
