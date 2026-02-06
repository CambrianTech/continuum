/**
 * Code Git Command - Browser Implementation
 *
 * Workspace-scoped git operations for the coding agent pipeline. Operations: status, diff, log, add, commit, push.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeGitParams, CodeGitResult } from '../shared/CodeGitTypes';

export class CodeGitBrowserCommand extends CommandBase<CodeGitParams, CodeGitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/git', context, subpath, commander);
  }

  async execute(params: CodeGitParams): Promise<CodeGitResult> {
    return await this.remoteExecute(params);
  }
}
