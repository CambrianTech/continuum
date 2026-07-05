/**
 * Code Tree Command - Browser Implementation
 *
 * Generate a directory tree for the workspace or a subdirectory. Shows file/directory structure with sizes. Skips common ignored directories (node_modules, .git, etc).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeTreeParams, CodeTreeResult } from '../shared/CodeTreeTypes';

export class CodeTreeBrowserCommand extends CommandBase<CodeTreeParams, CodeTreeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/tree', context, subpath, commander);
  }

  async execute(params: CodeTreeParams): Promise<CodeTreeResult> {
    console.log('🌐 BROWSER: Delegating Code Tree to server');
    return await this.remoteExecute(params);
  }
}
