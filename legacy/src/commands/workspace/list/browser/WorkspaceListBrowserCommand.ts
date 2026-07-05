/**
 * Workspace List Command - Browser Implementation
 *
 * List all persona workspaces across the team — worktree paths, git branches, modified files, shell activity. Scans both in-memory active workspaces and persisted git worktrees on disk.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WorkspaceListParams, WorkspaceListResult } from '../shared/WorkspaceListTypes';

export class WorkspaceListBrowserCommand extends CommandBase<WorkspaceListParams, WorkspaceListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/list', context, subpath, commander);
  }

  async execute(params: WorkspaceListParams): Promise<WorkspaceListResult> {
    console.log('🌐 BROWSER: Delegating Workspace List to server');
    return await this.remoteExecute(params);
  }
}
