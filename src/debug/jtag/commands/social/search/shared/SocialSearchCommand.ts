/**
 * Social Search Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialSearchParams, SocialSearchResult } from './SocialSearchTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialSearchBaseCommand extends CommandBase<SocialSearchParams, SocialSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/search', context, subpath, commander);
  }

  protected abstract executeSocialSearch(params: SocialSearchParams): Promise<SocialSearchResult>;

  async execute(params: JTAGPayload): Promise<SocialSearchResult> {
    return this.executeSocialSearch(params as SocialSearchParams);
  }
}
