/**
 * Git Status Command - Browser Implementation
 *
 * Show git workspace status and changes
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GitStatusParams, GitStatusResult } from '../shared/GitStatusTypes';

export class GitStatusBrowserCommand extends CommandBase<GitStatusParams, GitStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Status', context, subpath, commander);
  }

  async execute(params: GitStatusParams): Promise<GitStatusResult> {
    console.log('🌐 BROWSER: Delegating Git Status to server');
    return await this.remoteExecute(params);
  }
}
