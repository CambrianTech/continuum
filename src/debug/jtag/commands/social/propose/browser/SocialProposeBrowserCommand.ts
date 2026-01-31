/**
 * Social Propose Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialProposeBaseCommand } from '../shared/SocialProposeCommand';
import type { SocialProposeParams, SocialProposeResult } from '../shared/SocialProposeTypes';

export class SocialProposeBrowserCommand extends SocialProposeBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialPropose(params: SocialProposeParams): Promise<SocialProposeResult> {
    return await this.remoteExecute(params);
  }
}
