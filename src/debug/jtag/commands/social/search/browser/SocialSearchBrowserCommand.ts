/**
 * Social Search Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialSearchBaseCommand } from '../shared/SocialSearchCommand';
import type { SocialSearchParams, SocialSearchResult } from '../shared/SocialSearchTypes';

export class SocialSearchBrowserCommand extends SocialSearchBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialSearch(params: SocialSearchParams): Promise<SocialSearchResult> {
    return await this.remoteExecute(params);
  }
}
