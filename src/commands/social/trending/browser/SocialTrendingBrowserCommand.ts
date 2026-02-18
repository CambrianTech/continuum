/**
 * Social Trending Command - Browser Implementation
 * Delegates to server
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SocialTrendingParams, SocialTrendingResult } from '../shared/SocialTrendingTypes';

export class SocialTrendingBrowserCommand extends CommandBase<SocialTrendingParams, SocialTrendingResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/trending', context, subpath, commander);
  }

  async execute(params: SocialTrendingParams): Promise<SocialTrendingResult> {
    return await this.remoteExecute(params);
  }
}
