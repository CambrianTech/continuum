/**
 * Git Workspace Clean Command - Browser Implementation
 *
 * Clean up git workspace and remove worktree
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { GitWorkspaceCleanParams, GitWorkspaceCleanResult } from '../shared/GitWorkspaceCleanTypes';

export class GitWorkspaceCleanBrowserCommand extends CommandBase<GitWorkspaceCleanParams, GitWorkspaceCleanResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Workspace Clean', context, subpath, commander);
  }

  async execute(params: GitWorkspaceCleanParams): Promise<GitWorkspaceCleanResult> {
    console.log('🌐 BROWSER: Delegating Git Workspace Clean to server');
    return await this.remoteExecute(params);
  }
}
