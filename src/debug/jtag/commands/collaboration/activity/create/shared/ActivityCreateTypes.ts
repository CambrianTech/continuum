/**
 * Activity Create Command - Create a new activity from a recipe
 *
 * Activities are runtime instances of recipes with:
 * - Participants (humans + AIs with roles)
 * - Mutable state (phase, progress, variables)
 * - Configuration (can override recipe defaults)
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ActivityEntity, ActivityConfig } from '@system/data/entities/ActivityEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface ActivityCreateParams extends CommandParams {
  /**
   * Recipe ID to create activity from (e.g., 'general-chat', 'settings')
   */
  recipeId: string;

  /**
   * Display name for the activity
   */
  displayName: string;

  /**
   * Optional description
   */
  description?: string;

  /**
   * Owner user ID (defaults to current user)
   */
  ownerId?: UUID;

  /**
   * Optional unique ID (auto-generated if not provided)
   */
  uniqueId?: string;

  /**
   * Initial participants (owner is auto-added)
   */
  participants?: Array<{
    userId: UUID;
    role: string;
    roleConfig?: Record<string, unknown>;
  }>;

  /**
   * Initial configuration overrides
   */
  config?: Partial<ActivityConfig>;

  /**
   * Tags for categorization
   */
  tags?: string[];
}

export interface ActivityCreateResult extends CommandResult {
  success: boolean;
  error?: string;
  /**
   * The created activity
   */
  activity?: ActivityEntity;
}

/**
 * ActivityCreate — Type-safe command executor
 *
 * Usage:
 *   import { ActivityCreate } from '...shared/ActivityCreateTypes';
 *   const result = await ActivityCreate.execute({ ... });
 */
export const ActivityCreate = {
  execute(params: CommandInput<ActivityCreateParams>): Promise<ActivityCreateResult> {
    return Commands.execute<ActivityCreateParams, ActivityCreateResult>('collaboration/activity/create', params as Partial<ActivityCreateParams>);
  },
  commandName: 'collaboration/activity/create' as const,
} as const;
