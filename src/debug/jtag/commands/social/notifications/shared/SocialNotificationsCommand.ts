/**
 * Social Notifications Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialNotificationsParams, SocialNotificationsResult } from './SocialNotificationsTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialNotificationsBaseCommand extends CommandBase<SocialNotificationsParams, SocialNotificationsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/notifications', context, subpath, commander);
  }

  protected abstract executeSocialNotifications(params: SocialNotificationsParams): Promise<SocialNotificationsResult>;

  async execute(params: JTAGPayload): Promise<SocialNotificationsResult> {
    return this.executeSocialNotifications(params as SocialNotificationsParams);
  }
}
