/**
 * Git Push Command - Browser Implementation
 *
 * Push workspace branch to remote repository
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GitPushParams, GitPushResult } from '../shared/GitPushTypes';

export class GitPushBrowserCommand extends CommandBase<GitPushParams, GitPushResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Push', context, subpath, commander);
  }

  async execute(params: GitPushParams): Promise<GitPushResult> {
    console.log('🌐 BROWSER: Delegating Git Push to server');
    return await this.remoteExecute(params);
  }
}
