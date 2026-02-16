/**
 * Social Notifications Command - Shared Types
 *
 * Check for unread notifications (replies, mentions, followers) on a social media platform.
 * Key data source for SocialMediaRAGSource — personas become aware of social activity through this.
 *
 * Usage:
 *   ./jtag social/notifications --platform=moltbook
 *   ./jtag social/notifications --platform=moltbook --since=2026-01-30T00:00:00Z
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialNotification } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Notifications Command Parameters
 */
export interface SocialNotificationsParams extends CommandParams {
  /** Platform to check (e.g., 'moltbook') */
  platform: string;

  /** ISO timestamp to fetch notifications since */
  since?: string;

  /** Maximum number of notifications to return */
  limit?: number;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Factory function for creating SocialNotificationsParams
 */
export const createSocialNotificationsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    since?: string;
    limit?: number;
    personaId?: UUID;
  }
): SocialNotificationsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  since: data.since ?? '',
  limit: data.limit ?? 0,
  personaId: data.personaId ?? undefined,
  ...data
});

/**
 * Social Notifications Command Result
 */
export interface SocialNotificationsResult extends CommandResult {
  success: boolean;
  message: string;

  /** Array of notifications */
  notifications?: SocialNotification[];

  /** Count of unread notifications */
  unreadCount?: number;

  error?: JTAGError;
}

/**
 * Factory function for creating SocialNotificationsResult with defaults
 */
export const createSocialNotificationsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    notifications?: SocialNotification[];
    unreadCount?: number;
    error?: JTAGError;
  }
): SocialNotificationsResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  unreadCount: data.unreadCount ?? 0,
  ...data
});

/**
 * Smart Social Notifications-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialNotificationsResultFromParams = (
  params: SocialNotificationsParams,
  differences: Omit<SocialNotificationsResult, 'context' | 'sessionId'>
): SocialNotificationsResult => transformPayload(params, differences);

/**
 * SocialNotifications — Type-safe command executor
 *
 * Usage:
 *   import { SocialNotifications } from '...shared/SocialNotificationsTypes';
 *   const result = await SocialNotifications.execute({ platform: 'moltbook' });
 */
export const SocialNotifications = {
  execute(params: CommandInput<SocialNotificationsParams>): Promise<SocialNotificationsResult> {
    return Commands.execute<SocialNotificationsParams, SocialNotificationsResult>('social/notifications', params as Partial<SocialNotificationsParams>);
  },
  commandName: 'social/notifications' as const,
} as const;
