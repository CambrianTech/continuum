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
   * Server exec: Default to browser execution (like screenshot command)
   */
  async execute(params: ExecCommandParams): Promise<ExecCommandResult> {
    console.log(`🎯 SERVER EXEC: Starting execution`);
    
    if (!params.code) {
      return createExecErrorResult('validation', 'Missing required code parameter', 'server', params);
    }
    
    try {
      // If we have a result from browser execution, return it
      if (params.result !== undefined && params.executedIn === 'browser') {
        console.log(`📤 SERVER EXEC: Returning browser result:`, params.result);
        return createExecSuccessResult(params.result, 'browser', params, params.executedAt || Date.now());
      }
      
      // For JavaScript execution, delegate to browser by default (like screenshot)  
      if (params.code.type === 'inline' && params.code.language === 'javascript') {
        console.log(`🌐 SERVER EXEC: Delegating to browser for JavaScript execution`);
        return await this.remoteExecute(params);
      }
      
      // For server-specific code types, execute locally
      if (params.code.type === 'inline') {
        console.log(`🎯 SERVER EXEC: Executing in server context`);
        const sourceCode = params.code.source;
        
        // Simple JavaScript execution in server
        const func = new Function(sourceCode);
        const result = await func();
        
        console.log(`✅ SERVER EXEC: Success`);
        return createExecSuccessResult(result, 'server', params, Date.now());
      } else {
        return createExecErrorResult('validation', 'Server exec: unsupported code type', 'server', params);
      }
      
    } catch (error) {
      console.error(`❌ SERVER EXEC: Failed:`, error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      return createExecErrorResult('runtime', errorMessage, 'server', params, Date.now());
    }
  }
}