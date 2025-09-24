/**
 * ExecCommand Browser - Simple JavaScript execution in browser
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ExecCommandParams, ExecCommandResult } from '../shared/ExecTypes';
import { createExecErrorResult, createExecSuccessResult, DEFAULT_EXEC_TIMEOUT } from '../shared/ExecTypes';

export class ExecBrowserCommand extends CommandBase<ExecCommandParams, ExecCommandResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('exec', context, subpath, commander);
  }
  
  /**
   * Browser exec: Execute JavaScript directly in browser context
   * Simple execution with DOM access preserved
   */
  async execute(params: ExecCommandParams): Promise<ExecCommandResult> {
    console.log(`🎯 BROWSER EXEC: Starting execution`);

    if (!params.code) {
      return createExecErrorResult('validation', 'Missing required code parameter', 'browser', params);
    }

    if (params.code.type === 'inline' && params.code.language === 'javascript') {
      const sourceCode = params.code.source;

      console.log(`🎯 BROWSER EXEC: Executing JavaScript in browser`);

      try {
        // Simple execution with DOM access
        const func = new Function(`
          ${sourceCode}
        `);

        const result = func();

        console.log(`✅ BROWSER EXEC: Success - result:`, result);

        // FOLLOW SCREENSHOT PATTERN: Enrich params with result data before delegation
        params.result = result;
        params.executedAt = Date.now();
        params.executedIn = 'browser';

        // For cross-context execution, delegate to server with enriched params (like screenshot)
        if (params.context.uuid !== this.context.uuid) {
          console.log(`🔀 BROWSER EXEC: Cross-context detected - delegating to server with result`);
          return await this.remoteExecute(params);
        }

        // Same context - return directly
        console.log(`✅ BROWSER EXEC: Same context - returning result directly`);
        return createExecSuccessResult(result, 'browser', params, Date.now());

      } catch (error) {
        console.error(`❌ BROWSER EXEC: JavaScript execution failed:`, error);

        const errorMessage = error instanceof Error ? error.message : String(error);
        return createExecErrorResult('runtime', errorMessage, 'browser', params, Date.now());
      }
    } else {
      return createExecErrorResult('validation', 'Browser exec: unsupported code type', 'browser', params, Date.now());
    }
  }
}