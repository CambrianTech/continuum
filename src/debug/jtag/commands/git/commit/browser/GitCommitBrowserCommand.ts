/**
 * Git Commit Command - Browser Implementation
 *
 * Commit changes in git workspace with persona identity
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GitCommitParams, GitCommitResult } from '../shared/GitCommitTypes';

export class GitCommitBrowserCommand extends CommandBase<GitCommitParams, GitCommitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Commit', context, subpath, commander);
  }

  async execute(params: GitCommitParams): Promise<GitCommitResult> {
    console.log('🌐 BROWSER: Delegating Git Commit to server');
    return await this.remoteExecute(params);
  }
}
