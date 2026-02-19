/**
 * Social Browse Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialBrowseBaseCommand } from '../shared/SocialBrowseCommand';
import type { SocialBrowseParams, SocialBrowseResult } from '../shared/SocialBrowseTypes';

export class SocialBrowseBrowserCommand extends SocialBrowseBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialBrowse(params: SocialBrowseParams): Promise<SocialBrowseResult> {
    return await this.remoteExecute(params);
  }
}
