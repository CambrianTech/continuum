/**
 * ExecCommand Browser - Simple JavaScript execution in browser
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ExecCommandParams, ExecCommandResult } from '../shared/ExecTypes';
import { createExecErrorResult, createExecSuccessResult } from '../shared/ExecTypes';

export class ExecBrowserCommand extends CommandBase<ExecCommandParams, ExecCommandResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('exec', context, subpath, commander);
  }
  
  /**
   * Browser exec: Execute JavaScript directly in browser context
   * Note: Error handling is done by CommandDaemon, not individual commands
   */
  async execute(params: ExecCommandParams): Promise<ExecCommandResult> {
    console.log(`🎯 BROWSER EXEC: Starting execution`);
    
    if (!params.code) {
      return createExecErrorResult('validation', 'Missing required code parameter', 'browser', params);
    }
    
    if (params.code.type === 'inline' && params.code.language === 'javascript') {
      const sourceCode = params.code.source;
      console.log(`🎯 BROWSER EXEC: Executing JavaScript in browser`);
      
      // Simple JavaScript execution - let errors bubble up to CommandDaemon
      const func = new Function(sourceCode);
      const result = await func();
      
      console.log(`✅ BROWSER EXEC: Success - result:`, result);
      
      // Add result to params and route back to server (like screenshot does)
      params.result = result;
      params.executedAt = Date.now();
      params.executedIn = 'browser';
      
      console.log(`🔀 BROWSER EXEC: Sending result back to server`);
      return await this.remoteExecute(params);
    } else {
      return createExecErrorResult('validation', 'Browser exec: unsupported code type', 'browser', params, Date.now());
    }
  }
}