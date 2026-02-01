/**
 * Social Browse Command - Shared base class
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialBrowseParams, SocialBrowseResult } from './SocialBrowseTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialBrowseBaseCommand extends CommandBase<SocialBrowseParams, SocialBrowseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/browse', context, subpath, commander);
  }

  protected abstract executeSocialBrowse(params: SocialBrowseParams): Promise<SocialBrowseResult>;

  async execute(params: JTAGPayload): Promise<SocialBrowseResult> {
    return this.executeSocialBrowse(params as SocialBrowseParams);
  }
}
