/**
 * Git Workspace Init Command - Browser Implementation
 *
 * Initialize git workspace for persona collaboration with isolated worktree
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { GitWorkspaceInitParams, GitWorkspaceInitResult } from '../shared/GitWorkspaceInitTypes';

export class GitWorkspaceInitBrowserCommand extends CommandBase<GitWorkspaceInitParams, GitWorkspaceInitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Workspace Init', context, subpath, commander);
  }

  async execute(params: GitWorkspaceInitParams): Promise<GitWorkspaceInitResult> {
    console.log('🌐 BROWSER: Delegating Git Workspace Init to server');
    return await this.remoteExecute(params);
  }
}
