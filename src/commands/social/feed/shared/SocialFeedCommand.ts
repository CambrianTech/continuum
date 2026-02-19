/**
 * Social Feed Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialFeedParams, SocialFeedResult } from './SocialFeedTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialFeedBaseCommand extends CommandBase<SocialFeedParams, SocialFeedResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/feed', context, subpath, commander);
  }

  protected abstract executeSocialFeed(params: SocialFeedParams): Promise<SocialFeedResult>;

  async execute(params: JTAGPayload): Promise<SocialFeedResult> {
    return this.executeSocialFeed(params as SocialFeedParams);
  }
}
