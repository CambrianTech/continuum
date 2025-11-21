/**
 * Git Issue List Command - Browser Implementation
 *
 * Pass-through to server - no browser-specific logic needed.
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { GitIssueListParams, GitIssueListResult } from '../shared/GitIssueListTypes';

export class GitIssueListBrowserCommand extends CommandBase<GitIssueListParams, GitIssueListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('git/issue/list', context, subpath, commander);
  }

  async execute(params: GitIssueListParams): Promise<GitIssueListResult> {
    return await this.remoteExecute(params);
  }
}
