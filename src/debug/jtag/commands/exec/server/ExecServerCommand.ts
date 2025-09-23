/**
 * ExecCommand Server - Can execute server-side or delegate to browser
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ExecCommandParams, ExecCommandResult } from '../shared/ExecTypes';
import { createExecErrorResult, createExecSuccessResult } from '../shared/ExecTypes';

export class ExecServerCommand extends CommandBase<ExecCommandParams, ExecCommandResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('exec', context, subpath, commander);
  }
  
  /**
   * Server exec: Always delegate to browser (like screenshot command)
   * Simplifies the system - exec is for DOM inspection/manipulation
   */
  async execute(params: ExecCommandParams): Promise<ExecCommandResult> {
    console.log(`🎯 SERVER EXEC: Starting execution`);

    if (!params.code) {
      return createExecErrorResult('validation', 'Missing required code parameter', 'server', params);
    }

    // Check if we have result data from browser execution (like screenshot pattern)
    if (params.result !== undefined && params.executedIn === 'browser') {
      console.log(`✅ SERVER EXEC: Received result from browser execution`);

      // Return the result with proper server context
      return createExecSuccessResult(params.result, 'browser', params, params.executedAt || Date.now());
    }

    try {
      // No result yet - delegate to browser for DOM access (like screenshot)
      console.log(`🔀 SERVER EXEC: Delegating to browser for DOM access`);
      return await this.remoteExecute(params) as ExecCommandResult;

    } catch (error) {
      console.error(`❌ SERVER EXEC: Error during delegation:`, error);
      return createExecErrorResult('runtime', error instanceof Error ? error.message : String(error), 'server', params);
    }
  }
}