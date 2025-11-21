/**
 * Git Issue Create Command - Browser Implementation
 *
 * Pass-through to server - no browser-specific logic needed.
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { GitIssueCreateParams, GitIssueCreateResult } from '../shared/GitIssueCreateTypes';

export class GitIssueCreateBrowserCommand extends CommandBase<GitIssueCreateParams, GitIssueCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('git/issue/create', context, subpath, commander);
  }

  async execute(params: GitIssueCreateParams): Promise<GitIssueCreateResult> {
    return await this.remoteExecute(params);
  }
}
