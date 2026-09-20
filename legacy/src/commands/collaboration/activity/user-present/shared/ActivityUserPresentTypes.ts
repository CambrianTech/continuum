/**
 * Activity User Presence - Track user tab visibility for temperature
 *
 * Phase 3bis: Browser tab visibility integration
 * Called by MainWidget when tab visibility changes
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

export interface ActivityUserPresentParams extends CommandParams {
  /**
   * Activity ID (room ID for chat)
   */
  activityId: UUID;

  /**
   * Is user present? (true = tab visible, false = tab hidden)
   */
  present: boolean;
}

export interface ActivityUserPresentResult extends CommandResult {
  activityId: UUID;
  present: boolean;
  temperature: number;  // New temperature after update
  timestamp: number;
}

/**
 * ActivityUserPresent — Type-safe command executor
 *
 * Usage:
 *   import { ActivityUserPresent } from '...shared/ActivityUserPresentTypes';
 *   const result = await ActivityUserPresent.execute({ ... });
 */
export const ActivityUserPresent = {
  execute(params: CommandInput<ActivityUserPresentParams>): Promise<ActivityUserPresentResult> {
    return Commands.execute<ActivityUserPresentParams, ActivityUserPresentResult>('collaboration/activity/user-present', params as Partial<ActivityUserPresentParams>);
  },
  commandName: 'collaboration/activity/user-present' as const,
} as const;

/**
 * Factory function for creating CollaborationActivityUserPresentParams
 */
export const createCollaborationActivityUserPresentParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ActivityUserPresentParams, 'context' | 'sessionId' | 'userId'>
): ActivityUserPresentParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationActivityUserPresentResult with defaults
 */
export const createCollaborationActivityUserPresentResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ActivityUserPresentResult, 'context' | 'sessionId' | 'userId'>
): ActivityUserPresentResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/activity/user-present-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationActivityUserPresentResultFromParams = (
  params: ActivityUserPresentParams,
  differences: Omit<ActivityUserPresentResult, 'context' | 'sessionId' | 'userId'>
): ActivityUserPresentResult => transformPayload(params, differences);

