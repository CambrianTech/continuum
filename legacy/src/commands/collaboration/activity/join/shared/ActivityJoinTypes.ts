/**
 * Activity Join Command - Add a participant to an activity
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityParticipant } from '@system/data/entities/ActivityEntity';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

export interface ActivityJoinParams extends CommandParams {
  /**
   * Activity ID (UUID or uniqueId)
   */
  activityId: UUID | string;

  /**
   * Target user ID to add (defaults to params.userId, the current caller)
   */
  targetUserId?: UUID;

  /**
   * Role for the participant
   */
  role?: string;

  /**
   * Role-specific configuration
   */
  roleConfig?: Record<string, unknown>;
}

export interface ActivityJoinResult extends CommandResult {
  success: boolean;
  error?: string;
  activityId?: UUID;
  participant?: ActivityParticipant;
}

/**
 * ActivityJoin — Type-safe command executor
 *
 * Usage:
 *   import { ActivityJoin } from '...shared/ActivityJoinTypes';
 *   const result = await ActivityJoin.execute({ ... });
 */
export const ActivityJoin = {
  execute(params: CommandInput<ActivityJoinParams>): Promise<ActivityJoinResult> {
    return Commands.execute<ActivityJoinParams, ActivityJoinResult>('collaboration/activity/join', params as Partial<ActivityJoinParams>);
  },
  commandName: 'collaboration/activity/join' as const,
} as const;

/**
 * Factory function for creating CollaborationActivityJoinParams
 */
export const createCollaborationActivityJoinParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ActivityJoinParams, 'context' | 'sessionId' | 'userId'>
): ActivityJoinParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationActivityJoinResult with defaults
 */
export const createCollaborationActivityJoinResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ActivityJoinResult, 'context' | 'sessionId' | 'userId'>
): ActivityJoinResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/activity/join-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationActivityJoinResultFromParams = (
  params: ActivityJoinParams,
  differences: Omit<ActivityJoinResult, 'context' | 'sessionId' | 'userId'>
): ActivityJoinResult => transformPayload(params, differences);

