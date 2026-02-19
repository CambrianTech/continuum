/**
 * Code Shell Status Command - Browser Implementation
 *
 * Get shell session info for the persona's workspace — current working directory, active and total execution count. No parameters required (userId auto-injected).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeShellStatusParams, CodeShellStatusResult } from '../shared/CodeShellStatusTypes';

export class CodeShellStatusBrowserCommand extends CommandBase<CodeShellStatusParams, CodeShellStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/status', context, subpath, commander);
  }

  async execute(params: CodeShellStatusParams): Promise<CodeShellStatusResult> {
    console.log('🌐 BROWSER: Delegating Code Shell Status to server');
    return await this.remoteExecute(params);
  }
}
