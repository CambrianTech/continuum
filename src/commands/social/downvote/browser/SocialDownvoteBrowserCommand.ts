/**
 * Social Downvote Command - Browser Implementation
 *
 * Downvote a post on a social media platform
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SocialDownvoteParams, SocialDownvoteResult } from '../shared/SocialDownvoteTypes';

export class SocialDownvoteBrowserCommand extends CommandBase<SocialDownvoteParams, SocialDownvoteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/downvote', context, subpath, commander);
  }

  async execute(params: SocialDownvoteParams): Promise<SocialDownvoteResult> {
    console.log('🌐 BROWSER: Delegating Social Downvote to server');
    return await this.remoteExecute(params);
  }
}
