/**
 * Activity User Presence - Track user tab visibility for temperature
 *
 * Phase 3bis: Browser tab visibility integration
 * Called by MainWidget when tab visibility changes
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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
