/**
 * Code Shell Watch Command - Browser Implementation
 *
 * Watch a shell execution for new output. Blocks until output is available — no timeout, no polling. Returns classified output lines filtered through sentinel rules. Call in a loop until finished is true.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeShellWatchParams, CodeShellWatchResult } from '../shared/CodeShellWatchTypes';

export class CodeShellWatchBrowserCommand extends CommandBase<CodeShellWatchParams, CodeShellWatchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/watch', context, subpath, commander);
  }

  async execute(params: CodeShellWatchParams): Promise<CodeShellWatchResult> {
    console.log('🌐 BROWSER: Delegating Code Shell Watch to server');
    return await this.remoteExecute(params);
  }
}
