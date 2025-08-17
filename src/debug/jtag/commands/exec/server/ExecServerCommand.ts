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
    console.log(`🔍 SERVER EXEC: Params received:`, JSON.stringify(params, null, 2));
    
    if (!params.code) {
      console.log(`❌ SERVER EXEC: Missing code parameter`);
      return createExecErrorResult('validation', 'Missing required code parameter', 'server', params);
    }
    
    console.log(`✅ SERVER EXEC: Code parameter present, type: ${params.code.type}`);
    
    try {
      // If we have a result from browser execution, return it
      if (params.result !== undefined && params.executedIn === 'browser') {
        console.log(`📤 SERVER EXEC: Returning browser result:`, params.result);
        return createExecSuccessResult(params.result, 'browser', params, params.executedAt || Date.now());
      }
      
      // EXEC COMMANDS EXECUTE LOCALLY - no delegation to prevent infinite loops
      
      // Handle file execution by reading the file first
      if (params.code.type === 'file') {
        console.log(`📁 SERVER EXEC: Reading file ${params.code.path}`);
        const fs = await import('fs/promises');
        const path = await import('path');
        
        try {
          const fullPath = path.resolve(params.code.path);
          const sourceCode = await fs.readFile(fullPath, 'utf-8');
          console.log(`📖 SERVER EXEC: Read ${sourceCode.length} characters from file`);
          
          // Check if this needs to run in browser (DOM manipulation)
          if (sourceCode.includes('document') || sourceCode.includes('window') || sourceCode.includes('querySelector')) {
            console.log(`🔀 SERVER EXEC: File contains DOM code, delegating to browser`);
            
            // Delegate to browser with the file contents as inline code
            const browserParams: ExecCommandParams = {
              ...params,
              code: {
                type: 'inline',
                language: 'javascript',
                source: sourceCode
              },
              result: undefined,
              executedAt: undefined,
              executedIn: undefined
            };
            
            const browserResult = await this.remoteExecute(browserParams);
            console.log(`🔄 SERVER EXEC: Browser delegation result:`, browserResult);
            return browserResult as ExecCommandResult;
          }
          
          // Execute JavaScript file in Node.js context
          const func = new Function(sourceCode);
          const result = await func();
          
          console.log(`✅ SERVER EXEC: File executed successfully`);
          return createExecSuccessResult(result, 'server', params, Date.now());
        } catch (fileError) {
          console.error(`❌ SERVER EXEC: File execution failed:`, fileError);
          const errorMessage = fileError instanceof Error ? fileError.message : String(fileError);
          return createExecErrorResult('runtime', `File execution failed: ${errorMessage}`, 'server', params, Date.now());
        }
      }
      
      // Check if inline JavaScript needs browser delegation
      if (params.code.type === 'inline' && params.code.language === 'javascript') {
        const sourceCode = params.code.source;
        
        // Check if this needs to run in browser (DOM manipulation)
        if (sourceCode.includes('document') || sourceCode.includes('window') || sourceCode.includes('querySelector')) {
          console.log(`🔀 SERVER EXEC: Inline code contains DOM references, delegating to browser`);
          
          // Delegate to browser
          const browserParams: ExecCommandParams = {
            ...params,
            result: undefined,
            executedAt: undefined,
            executedIn: undefined
          };
          
          const browserResult = await this.remoteExecute(browserParams);
          console.log(`🔄 SERVER EXEC: Browser delegation result:`, browserResult);
          return browserResult as ExecCommandResult;
        }
        
        console.log(`🎯 SERVER EXEC: Executing JavaScript in Node.js server context`);
        
        // Execute JavaScript in Node.js context
        const func = new Function(sourceCode);
        const result = await func();
        
        console.log(`✅ SERVER EXEC: JavaScript executed in server context`);
        return createExecSuccessResult(result, 'server', params, Date.now());
      }
      
      // For other inline code types, execute locally
      if (params.code.type === 'inline') {
        console.log(`🎯 SERVER EXEC: Executing in server context`);
        const sourceCode = params.code.source;
        
        // Simple JavaScript execution in server
        const func = new Function(sourceCode);
        const result = await func();
        
        console.log(`✅ SERVER EXEC: Success`);
        return createExecSuccessResult(result, 'server', params, Date.now());
      } else {
        return createExecErrorResult('validation', `Server exec: unsupported code type '${params.code.type}'`, 'server', params);
      }
      
    } catch (error) {
      console.error(`❌ SERVER EXEC: Failed:`, error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      return createExecErrorResult('runtime', errorMessage, 'server', params, Date.now());
    }
  }
}