/**
 * Social Community Command - Browser Implementation
 *
 * Manage communities (submolts) — create, list, subscribe, unsubscribe, get info
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SocialCommunityParams, SocialCommunityResult } from '../shared/SocialCommunityTypes';

export class SocialCommunityBrowserCommand extends CommandBase<SocialCommunityParams, SocialCommunityResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/community', context, subpath, commander);
  }

  async execute(params: SocialCommunityParams): Promise<SocialCommunityResult> {
    console.log('🌐 BROWSER: Delegating Social Community to server');
    return await this.remoteExecute(params);
  }
}
