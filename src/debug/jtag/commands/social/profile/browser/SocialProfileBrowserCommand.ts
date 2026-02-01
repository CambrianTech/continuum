/**
 * Social Profile Command - Browser Implementation
 * Delegates to server
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SocialProfileParams, SocialProfileResult } from '../shared/SocialProfileTypes';

export class SocialProfileBrowserCommand extends CommandBase<SocialProfileParams, SocialProfileResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/profile', context, subpath, commander);
  }

  async execute(params: SocialProfileParams): Promise<SocialProfileResult> {
    return await this.remoteExecute(params);
  }
}
