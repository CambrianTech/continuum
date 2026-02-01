/**
 * Social Feed Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialFeedBaseCommand } from '../shared/SocialFeedCommand';
import type { SocialFeedParams, SocialFeedResult } from '../shared/SocialFeedTypes';

export class SocialFeedBrowserCommand extends SocialFeedBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialFeed(params: SocialFeedParams): Promise<SocialFeedResult> {
    return await this.remoteExecute(params);
  }
}
