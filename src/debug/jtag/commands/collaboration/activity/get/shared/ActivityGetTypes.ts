/**
 * Activity Get Command - Retrieve an activity by ID or uniqueId
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityEntity } from '@system/data/entities/ActivityEntity';

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
