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
        // Simple execution with DOM access - wrap in IIFE and return
        const func = new Function(`
          return (function() {
            ${sourceCode}
          })();
        `);

        const result = func();

        console.log(`✅ BROWSER EXEC: Success - result:`, result);

        // If called from server, send result back via remoteExecute pattern
        if (params.context.environment === 'server') {
          return await this.remoteExecute({
            ...params,
            result,
            executedAt: Date.now(),
            executedIn: 'browser'
          }) as ExecCommandResult;
        }

        // Direct browser call
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