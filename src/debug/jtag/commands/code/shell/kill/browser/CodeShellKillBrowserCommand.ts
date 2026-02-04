/**
 * Code Shell Kill Command - Browser Implementation
 *
 * Kill a running shell execution. Use the executionId returned by code/shell/execute to identify the target.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeShellKillParams, CodeShellKillResult } from '../shared/CodeShellKillTypes';

export class CodeShellKillBrowserCommand extends CommandBase<CodeShellKillParams, CodeShellKillResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/kill', context, subpath, commander);
  }

  async execute(params: CodeShellKillParams): Promise<CodeShellKillResult> {
    console.log('🌐 BROWSER: Delegating Code Shell Kill to server');
    return await this.remoteExecute(params);
  }
}
