/**
 * Code Verify Command - Browser Implementation
 *
 * Run TypeScript compilation checks and optionally execute tests against a persona workspace.
 * Delegates to server — verification requires file system access and process execution.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeVerifyParams, CodeVerifyResult } from '../shared/CodeVerifyTypes';

export class CodeVerifyBrowserCommand extends CommandBase<CodeVerifyParams, CodeVerifyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/verify', context, subpath, commander);
  }

  async execute(params: CodeVerifyParams): Promise<CodeVerifyResult> {
    console.log('🌐 BROWSER: Delegating Code Verify to server');
    return await this.remoteExecute(params);
  }
}
