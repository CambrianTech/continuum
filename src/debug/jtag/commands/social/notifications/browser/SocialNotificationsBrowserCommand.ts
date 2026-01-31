/**
 * Social Notifications Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialNotificationsBaseCommand } from '../shared/SocialNotificationsCommand';
import type { SocialNotificationsParams, SocialNotificationsResult } from '../shared/SocialNotificationsTypes';

export class SocialNotificationsBrowserCommand extends SocialNotificationsBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialNotifications(params: SocialNotificationsParams): Promise<SocialNotificationsResult> {
    return await this.remoteExecute(params);
  }
}
