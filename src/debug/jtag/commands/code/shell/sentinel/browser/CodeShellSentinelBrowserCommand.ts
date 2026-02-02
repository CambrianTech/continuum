/**
 * Code Shell Sentinel Command - Browser Implementation
 *
 * Configure sentinel filter rules on a shell execution. Rules classify output lines and control which lines are emitted or suppressed during watch. Patterns are compiled to regex on the Rust side for performance.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeShellSentinelParams, CodeShellSentinelResult } from '../shared/CodeShellSentinelTypes';

export class CodeShellSentinelBrowserCommand extends CommandBase<CodeShellSentinelParams, CodeShellSentinelResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/sentinel', context, subpath, commander);
  }

  async execute(params: CodeShellSentinelParams): Promise<CodeShellSentinelResult> {
    console.log('🌐 BROWSER: Delegating Code Shell Sentinel to server');
    return await this.remoteExecute(params);
  }
}
