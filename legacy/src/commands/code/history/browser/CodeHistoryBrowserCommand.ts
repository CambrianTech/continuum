/**
 * Code History Command - Browser Implementation
 *
 * Get change history for a specific file or the entire workspace. Returns change graph nodes with diffs, timestamps, and descriptions.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeHistoryParams, CodeHistoryResult } from '../shared/CodeHistoryTypes';

export class CodeHistoryBrowserCommand extends CommandBase<CodeHistoryParams, CodeHistoryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/history', context, subpath, commander);
  }

  async execute(params: CodeHistoryParams): Promise<CodeHistoryResult> {
    console.log('🌐 BROWSER: Delegating Code History to server');
    return await this.remoteExecute(params);
  }
}
