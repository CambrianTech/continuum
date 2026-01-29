/**
 * Activity Get Command - Retrieve an activity by ID or uniqueId
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityEntity } from '@system/data/entities/ActivityEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface ActivityGetParams extends CommandParams {
  /**
   * Activity UUID (primary key)
   */
  id?: UUID;

  /**
   * Activity unique ID (human-readable)
   */
  uniqueId?: string;
}

export interface ActivityGetResult extends CommandResult {
  success: boolean;
  error?: string;
  activity?: ActivityEntity;
}

/**
 * ActivityGet — Type-safe command executor
 *
 * Usage:
 *   import { ActivityGet } from '...shared/ActivityGetTypes';
 *   const result = await ActivityGet.execute({ ... });
 */
export const ActivityGet = {
  execute(params: CommandInput<ActivityGetParams>): Promise<ActivityGetResult> {
    return Commands.execute<ActivityGetParams, ActivityGetResult>('collaboration/activity/get', params as Partial<ActivityGetParams>);
  },
  commandName: 'collaboration/activity/get' as const,
} as const;
