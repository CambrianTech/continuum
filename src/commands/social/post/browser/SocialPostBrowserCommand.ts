/**
 * Social Post Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialPostBaseCommand } from '../shared/SocialPostCommand';
import type { SocialPostParams, SocialPostResult } from '../shared/SocialPostTypes';

export class SocialPostBrowserCommand extends SocialPostBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialPost(params: SocialPostParams): Promise<SocialPostResult> {
    return await this.remoteExecute(params);
  }
}
