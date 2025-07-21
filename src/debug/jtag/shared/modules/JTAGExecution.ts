/**
 * JTAG Execution Module
 * 
 * Handles code execution in both browser and server contexts
 */

import { JTAGExecOptions, JTAGExecResult, JTAGConfig } from '../JTAGTypes';

export class JTAGExecution {
  
  /**
   * Execute code - delegates to appropriate context handler
   */
  static async exec(
    code: string,
    options: JTAGExecOptions = {},
    isServer: boolean,
    isClient: boolean,
    config: JTAGConfig,
    log: (component: string, message: string, data?: any) => void,
    critical: (component: string, message: string, data?: any) => void,
    executeServerCode?: (code: string, options: JTAGExecOptions) => Promise<any>,
    executeBrowserCode?: (code: string, options: JTAGExecOptions) => Promise<any>
  ): Promise<JTAGExecResult> {
    
    log('JTAG_EXEC', `Code execution requested`, { 
      context: config.context,
      codeLength: code.length,
      options 
    });

    const startTime = Date.now();
    
    try {
      let result;
      
      if (isServer) {
        if (!executeServerCode) {
          throw new Error('Server execution handler not available');
        }
        result = await executeServerCode(code, options);
      } else if (isClient) {
        if (!executeBrowserCode) {
          throw new Error('Browser execution handler not available');
        }
        result = await executeBrowserCode(code, options);
      } else {
        throw new Error('Unknown execution context');
      }
      
      const executionTime = Date.now() - startTime;
      
      log('JTAG_EXEC', 'Code execution completed', { 
        executionTime,
        resultType: typeof result,
        success: true
      });
      
      return {
        success: true,
        result,
        executionTime,
        context: config.context,
        timestamp: new Date().toISOString(),
        uuid: options.uuid || 'exec_' + Date.now().toString(36)
      };
      
    } catch (error: any) {
      const executionTime = Date.now() - startTime;
      
      critical('JTAG_EXEC', 'Code execution failed', { 
        error: error.message,
        executionTime,
        code: code.substring(0, 200) + (code.length > 200 ? '...' : '')
      });
      
      return {
        success: false,
        result: null,
        error: error.message,
        executionTime,
        context: config.context,
        timestamp: new Date().toISOString(),
        uuid: options.uuid || 'exec_' + Date.now().toString(36)
      };
    }
  }

  /**
   * Execute code in server context
   */
  static async executeServerCode(
    code: string, 
    options: JTAGExecOptions,
    log: (component: string, message: string, data?: any) => void,
    critical: (component: string, message: string, data?: any) => void
  ): Promise<any> {
    
    log('JTAG_SERVER_EXEC', 'Executing server-side code', { 
      codeLength: code.length,
      timeout: options.timeout 
    });
    
    try {
      // Server-side code execution with timeout
      if (options.timeout) {
        return await Promise.race([
          Promise.resolve(eval(code)),
          new Promise((_, reject) => 
            setTimeout(() => reject(new Error('Execution timeout')), options.timeout)
          )
        ]);
      } else {
        return eval(code);
      }
      
    } catch (error: any) {
      critical('JTAG_SERVER_EXEC', 'Server code execution error', { 
        error: error.message,
        stack: error.stack,
        code: code.substring(0, 100)
      });
      throw error;
    }
  }

  /**
   * Execute code in browser context
   */
  static async executeBrowserCode(
    code: string, 
    options: JTAGExecOptions,
    log: (component: string, message: string, data?: any) => void,
    critical: (component: string, message: string, data?: any) => void
  ): Promise<any> {
    
    if (typeof window === 'undefined') {
      throw new Error('Browser execution only available in browser context');
    }
    
    log('JTAG_BROWSER_EXEC', 'Executing browser-side code', { 
      codeLength: code.length,
      timeout: options.timeout 
    });
    
    try {
      // Browser-side code execution with timeout
      if (options.timeout) {
        return await Promise.race([
          Promise.resolve(eval(code)),
          new Promise((_, reject) => 
            setTimeout(() => reject(new Error('Execution timeout')), options.timeout)
          )
        ]);
      } else {
        return eval(code);
      }
      
    } catch (error: any) {
      critical('JTAG_BROWSER_EXEC', 'Browser code execution error', { 
        error: error.message,
        stack: error.stack,
        code: code.substring(0, 100)
      });
      throw error;
    }
  }
}