/**
 * Social Post Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialPostParams, SocialPostResult } from './SocialPostTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialPostBaseCommand extends CommandBase<SocialPostParams, SocialPostResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/post', context, subpath, commander);
  }

  protected abstract executeSocialPost(params: SocialPostParams): Promise<SocialPostResult>;

  async execute(params: JTAGPayload): Promise<SocialPostResult> {
    return this.executeSocialPost(params as SocialPostParams);
  }
}
