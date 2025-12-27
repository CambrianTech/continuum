/**
 * Activity Update Command - Update activity state, phase, or config
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityEntity, ActivityStatus } from '@system/data/entities/ActivityEntity';

export interface ActivityUpdateParams extends CommandParams {
  /**
   * Activity ID (UUID or uniqueId)
   */
  activityId: UUID | string;

  /**
   * Update display name
   */
  displayName?: string;

  /**
   * Update description
   */
  description?: string;

  /**
   * Update status
   */
  status?: ActivityStatus;

  /**
   * Update phase
   */
  phase?: string;

  /**
   * Update progress (0-100)
   */
  progress?: number;

  /**
   * Set state variables (merged with existing)
   */
  variables?: Record<string, unknown>;

  /**
   * Update settings (merged with existing)
   */
  settings?: Record<string, unknown>;

  /**
   * Update tags
   */
  tags?: string[];
}

export interface ActivityUpdateResult extends CommandResult {
  success: boolean;
  error?: string;
  activity?: ActivityEntity;
  changedFields?: string[];
}
