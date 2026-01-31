/**
 * Social Engage Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialEngageBaseCommand } from '../shared/SocialEngageCommand';
import type { SocialEngageParams, SocialEngageResult } from '../shared/SocialEngageTypes';

export class SocialEngageBrowserCommand extends SocialEngageBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialEngage(params: SocialEngageParams): Promise<SocialEngageResult> {
    return await this.remoteExecute(params);
  }
}
