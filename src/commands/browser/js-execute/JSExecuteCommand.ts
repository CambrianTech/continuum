/**
 * JavaScript Execute Command - Execute JavaScript in browser with session logging
 * 
 * Layer 7: JavaScript execution commands write to session logs
 * Integrates with session console logging to capture execution results
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import type { CommandDefinition, CommandResult } from '../../core/base-command/BaseCommand';

export interface JSExecuteOptions {
  script: string;
  sessionId?: string;
  generateUUID?: boolean;
  waitForResult?: boolean;
  timeout?: number;
  logExecution?: boolean;
}

export class JSExecuteCommand extends BaseCommand {
  /**
   * Parse parameters with command line argument support
   */
  private static parseJSParams(params: any): JSExecuteOptions {
    // Handle object with args array (from continuum script)
    if (params && params.args && Array.isArray(params.args)) {
      const result: any = {};
      for (const arg of params.args) {
        if (typeof arg === 'string' && arg.startsWith('--')) {
          const [key, value] = arg.split('=', 2);
          const cleanKey = key.replace('--', '');
          result[cleanKey] = value;
        }
      }
      return result;
    }
    
    // Handle string array from command line
    if (Array.isArray(params)) {
      const result: any = {};
      for (const param of params) {
        if (typeof param === 'string' && param.startsWith('--')) {
          const [key, value] = param.split('=', 2);
          const cleanKey = key.replace('--', '');
          result[cleanKey] = value;
        }
      }
      return result;
    }
    
    // Handle object parameters
    if (typeof params === 'object' && params !== null) {
      return params;
    }
    
    // Handle JSON string
    if (typeof params === 'string') {
      try {
        return JSON.parse(params);
      } catch {
        // If not JSON, treat as script directly
        return { script: params };
      }
    }
    
    return { script: '' };
  }

  static getDefinition(): CommandDefinition {
    return {
      name: 'js-execute',
      description: 'Execute JavaScript in browser with session logging integration',
      category: 'browser',
      parameters: {
        script: {
          type: 'string' as const,
          description: 'JavaScript code to execute',
          required: true
        },
        sessionId: {
          type: 'string' as const,
          description: 'Session ID for logging (auto-detected if not provided)',
          required: false
        },
        generateUUID: {
          type: 'boolean' as const,
          description: 'Generate tracking UUID for execution',
          required: false,
          default: true
        },
        waitForResult: {
          type: 'boolean' as const,
          description: 'Wait for JavaScript execution result',
          required: false,
          default: true
        },
        timeout: {
          type: 'number' as const,
          description: 'Execution timeout in milliseconds',
          required: false,
          default: 30000
        },
        logExecution: {
          type: 'boolean' as const,
          description: 'Log execution details to session',
          required: false,
          default: true
        }
      },
      examples: [
        {
          description: 'Execute simple JavaScript with UUID tracking',
          command: 'js-execute --script="console.log(\'Hello from session!\')"'
        },
        {
          description: 'Execute with custom session and no UUID',
          command: 'js-execute --script="document.title = \'Test Page\'" --sessionId=my-session-123 --generateUUID=false'
        }
      ]
    };
  }

  static async execute(params: any, _context: any): Promise<CommandResult> {
    try {
      // Parse parameters from command line args
      const options = JSExecuteCommand.parseJSParams(params);
      
      const { 
        script, 
        sessionId, 
        generateUUID = true, 
        waitForResult = true, 
        timeout = 30000
      } = options;
      
      if (!script) {
        return {
          success: false,
          error: 'Script parameter is required',
          data: { providedParams: params, parsedOptions: options }
        };
      }
      
      // Generate execution UUID for JTAG tracking
      const executionUUID = generateUUID ? 
        `exec-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` : 
        null;

      // Create JavaScript with UUID logging for JTAG tracking
      const wrappedScript = `
        // JTAG UUID tracking
        ${executionUUID ? `console.log('🎯 EXECUTION_UUID_${executionUUID}_START');` : ''}
        
        try {
          // User script execution
          ${script}
          
          // JTAG completion marker
          ${executionUUID ? `console.log('🎯 EXECUTION_UUID_${executionUUID}_COMPLETE');` : ''}
          console.log('✅ JTAG: JavaScript executed successfully at ' + new Date().toISOString());
        } catch (error) {
          console.error('❌ JTAG: JavaScript execution error:', error);
          ${executionUUID ? `console.log('🎯 EXECUTION_UUID_${executionUUID}_ERROR:', error.message);` : ''}
          throw error;
        }
      `;

      // Execute JavaScript via browser WebSocket command
      const executeOptions: { timeout: number; waitForResult: boolean; sessionId?: string } = {
        timeout,
        waitForResult
      };
      if (sessionId) {
        executeOptions.sessionId = sessionId;
      }
      const executionResult = await JSExecuteCommand.executeInBrowserViaWebSocket(wrappedScript, executeOptions);
      return {
        success: true,
        data: {
          executionUUID,
          result: executionResult,
          sessionId: sessionId || 'auto-detected',
          timestamp: new Date().toISOString(),
          script: script && script.length > 100 ? script.substring(0, 100) + '...' : script || 'unknown',
          wrappedScript: wrappedScript.length > 200 ? wrappedScript.substring(0, 200) + '...' : wrappedScript
        },
        message: `JTAG JavaScript executed ${executionUUID ? `[UUID: ${executionUUID}]` : ''}`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      // Parse parameters for error handling  
      const options = JSExecuteCommand.parseJSParams(params);
      
      // Log execution error to session
      if (options.logExecution !== false) {
        try {
          await JSExecuteCommand.logToSession(options.sessionId, 'error', 
            `❌ JavaScript execution failed: ${errorMessage}`
          );
        } catch (error) {
          // Continue with error response even if session logging fails
          console.warn('⚠️ Session error logging failed:', error);
        }
      }

      return {
        success: false,
        error: `JavaScript execution failed: ${errorMessage}`,
        data: {
          script: options.script && options.script.length > 100 ? options.script.substring(0, 100) + '...' : options.script || 'unknown',
          sessionId: options.sessionId || 'auto-detected'
        }
      };
    }
  }

  /**
   * Execute JavaScript in browser via real DevTools connection
   */
  private static async executeInBrowserViaWebSocket(script: string, options: {
    sessionId?: string;
    timeout: number;
    waitForResult: boolean;
  }): Promise<any> {
    try {
      console.log(`🌍 JTAG Real Browser Execute: ${script.substring(0, 100)}...`);
      
      // Use real browser execution via DevTools Protocol
      const { ChromiumDevToolsAdapter } = await import('../../../daemons/browser-manager/adapters/ChromiumDevToolsAdapter');
      const adapter = new ChromiumDevToolsAdapter();
      
      // Get debug URL from browser manager (assuming localhost:9222 for now)
      const debugUrl = 'ws://localhost:9222';
      
      try {
        await adapter.connect(debugUrl);
        console.log(`🔗 Connected to browser DevTools at ${debugUrl}`);
        
        // Execute script via real DevTools Protocol
        const result = await adapter.evaluateScript(script);
        
        await adapter.disconnect();
        
        console.log(`✅ JTAG Real Browser Execute Complete: Script executed in real browser`);
        return {
          success: true,
          executedAt: new Date().toISOString(),
          method: 'devtools-real-execution',
          result: result,
          scriptLength: script.length,
          timeout: options.timeout,
          sessionId: options.sessionId || 'default'
        };
        
      } catch (devToolsError) {
        console.error(`❌ DevTools connection failed: ${devToolsError}`);
        
        // Fallback: Send via WebSocket to browser portal if DevTools fails
        const webSocketResult = await JSExecuteCommand.sendViaBrowserWebSocket(script, options);
        return webSocketResult;
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG Browser Execute Failed: ${errorMessage}`);
      throw new Error(`Browser execution failed: ${errorMessage}`);
    }
  }
  
  /**
   * Fallback: Send script to browser via WebSocket portal connection
   */
  private static async sendViaBrowserWebSocket(script: string, options: {
    sessionId?: string;
    timeout: number;
    waitForResult: boolean;
  }): Promise<any> {
    // Send JavaScript to connected browser via WebSocket
    // This should trigger the browser's WebSocket client to execute the script
    console.log(`📡 Sending script to browser via WebSocket portal...`);
    
    // TODO: Get WebSocket server and broadcast to connected browsers
    // For now, return indication that we need browser connection
    return {
      success: false,
      executedAt: new Date().toISOString(),
      method: 'websocket-fallback',
      error: 'No browser connection available - need active browser at localhost:9000',
      scriptLength: script.length,
      timeout: options.timeout,
      sessionId: options.sessionId || 'default'
    };
  }

  /**
   * Log message to session logs (integrates with session manager)
   */
  private static async logToSession(sessionId: string | undefined, level: 'info' | 'warn' | 'error', message: string): Promise<void> {
    try {
      // Get the actual session and append to its server log file
      if (sessionId && sessionId !== 'auto-detected') {
        const timestamp = new Date().toISOString();
        const logMessage = `[${timestamp}] ${level.toUpperCase()}: ${message}`;
        
        // TODO: Send message to SessionManagerDaemon to append to session server log
        // For now, we'll append directly to session log file if we can determine the path
        await JSExecuteCommand.appendToSessionServerLog(sessionId, logMessage);
      } else {
        // Fallback to console logging for auto-detected sessions
        const timestamp = new Date().toISOString();
        const logMessage = `[${timestamp}] ${level.toUpperCase()}: ${message}`;
        console.log(`📝 Session Log [${sessionId || 'auto'}]: ${logMessage}`);
      }
      
    } catch (error) {
      console.warn('⚠️ Failed to log to session:', error);
    }
  }

  /**
   * Append message to session server log file
   */
  private static async appendToSessionServerLog(sessionId: string, message: string): Promise<void> {
    try {
      // TODO: This should integrate with SessionManagerDaemon to get proper session log path
      // For now, use the session artifacts pattern from SessionManagerDaemon
      
      const fs = await import('fs/promises');
      const path = await import('path');
      
      // Construct session log path (this should match SessionManagerDaemon pattern)
      const sessionLogPath = path.join('.continuum/sessions', 'development', 'console-test', sessionId, 'logs', 'server.log');
      
      // Check if session log exists, if not create it
      try {
        await fs.access(sessionLogPath);
      } catch {
        // Session log doesn't exist, create directory structure and file
        await fs.mkdir(path.dirname(sessionLogPath), { recursive: true });
        const sessionStartTime = new Date().toISOString();
        const headerContent = `# Continuum Session Log\n# Session: ${sessionId}\n# Created: ${sessionStartTime}\n# Type: development\n# Owner: console-test\n#\n# Session started at ${sessionStartTime}\n\n`;
        await fs.writeFile(sessionLogPath, headerContent);
      }
      
      // Append the message
      await fs.appendFile(sessionLogPath, message + '\n');
      
    } catch (error) {
      // If session file writing fails, fall back to console
      console.log(`📝 Session Log [${sessionId}]: ${message}`);
    }
  }
}

export default JSExecuteCommand;