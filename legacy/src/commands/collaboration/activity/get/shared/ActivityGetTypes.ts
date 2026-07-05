/**
 * Activity Get Command - Retrieve an activity by ID or uniqueId
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityEntity } from '@system/data/entities/ActivityEntity';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

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

/**
 * Factory function for creating CollaborationActivityGetParams
 */
export const createCollaborationActivityGetParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ActivityGetParams, 'context' | 'sessionId' | 'userId'>
): ActivityGetParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationActivityGetResult with defaults
 */
export const createCollaborationActivityGetResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ActivityGetResult, 'context' | 'sessionId' | 'userId'>
): ActivityGetResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/activity/get-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationActivityGetResultFromParams = (
  params: ActivityGetParams,
  differences: Omit<ActivityGetResult, 'context' | 'sessionId' | 'userId'>
): ActivityGetResult => transformPayload(params, differences);

