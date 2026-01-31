/**
 * Social Notifications Command - Server Implementation
 *
 * Fetches unread notifications from a social media platform.
 * This is the data source for SocialMediaRAGSource — personas become
 * aware of social activity through this command.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialNotificationsBaseCommand } from '../shared/SocialNotificationsCommand';
import type { SocialNotificationsParams, SocialNotificationsResult } from '../shared/SocialNotificationsTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialNotificationsServerCommand extends SocialNotificationsBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialNotifications(params: SocialNotificationsParams): Promise<SocialNotificationsResult> {
    const { platform, since, limit } = params;

    if (!platform) throw new Error('platform is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    const notifications = await ctx.provider.getNotifications(since);

    // Apply limit if specified
    const limited = limit ? notifications.slice(0, limit) : notifications;
    const unreadCount = limited.filter(n => !n.read).length;

    return transformPayload(params, {
      success: true,
      message: unreadCount > 0
        ? `${unreadCount} unread notification${unreadCount === 1 ? '' : 's'} on ${platform}`
        : `No unread notifications on ${platform}`,
      notifications: limited,
      unreadCount,
    });
  }
}
