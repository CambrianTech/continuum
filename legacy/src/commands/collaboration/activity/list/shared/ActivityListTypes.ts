/**
 * Activity List Command - Query activities with filters
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityEntity, ActivityStatus } from '@system/data/entities/ActivityEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface ActivityListParams extends CommandParams {
  /**
   * Filter by recipe ID
   */
  recipeId?: string;

  /**
   * Filter by status
   */
  status?: ActivityStatus | ActivityStatus[];

  /**
   * Filter by owner
   */
  ownerId?: UUID;

  /**
   * Filter by participant (find activities user is part of)
   */
  participantId?: UUID;

  /**
   * Filter by tags (any match)
   */
  tags?: string[];

  /**
   * Maximum results (default: 50)
   */
  limit?: number;

  /**
   * Order by field
   */
  orderBy?: 'lastActivityAt' | 'createdAt' | 'displayName';

  /**
   * Order direction
   */
  orderDirection?: 'asc' | 'desc';
}

export interface ActivityListResult extends CommandResult {
  success: boolean;
  error?: string;
  activities?: ActivityEntity[];
  total?: number;
}

/**
 * ActivityList — Type-safe command executor
 *
 * Usage:
 *   import { ActivityList } from '...shared/ActivityListTypes';
 *   const result = await ActivityList.execute({ ... });
 */
export const ActivityList = {
  execute(params: CommandInput<ActivityListParams>): Promise<ActivityListResult> {
    return Commands.execute<ActivityListParams, ActivityListResult>('collaboration/activity/list', params as Partial<ActivityListParams>);
  },
  commandName: 'collaboration/activity/list' as const,
} as const;
