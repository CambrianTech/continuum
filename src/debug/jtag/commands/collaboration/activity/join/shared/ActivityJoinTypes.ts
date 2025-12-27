/**
 * Activity Join Command - Add a participant to an activity
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityParticipant } from '@system/data/entities/ActivityEntity';

export interface ActivityJoinParams extends CommandParams {
  /**
   * Activity ID (UUID or uniqueId)
   */
  activityId: UUID | string;

  /**
   * User ID to add (defaults to current user)
   */
  userId?: UUID;

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
