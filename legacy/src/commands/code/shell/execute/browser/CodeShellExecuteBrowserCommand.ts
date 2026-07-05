/**
 * Code Shell Execute Command - Browser Implementation
 *
 * Execute a shell command in the persona's workspace. Async mode (default) returns execution handle immediately — use code/shell/watch to stream output. Sync mode (wait=true) blocks until completion and returns full stdout/stderr.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeShellExecuteParams, CodeShellExecuteResult } from '../shared/CodeShellExecuteTypes';

export class CodeShellExecuteBrowserCommand extends CommandBase<CodeShellExecuteParams, CodeShellExecuteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/execute', context, subpath, commander);
  }

  async execute(params: CodeShellExecuteParams): Promise<CodeShellExecuteResult> {
    console.log('🌐 BROWSER: Delegating Code Shell Execute to server');
    return await this.remoteExecute(params);
  }
}
