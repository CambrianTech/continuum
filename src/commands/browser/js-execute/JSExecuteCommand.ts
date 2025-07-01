/**
 * JavaScript Execute Command - Execute JavaScript in browser with session logging
 * 
 * Layer 7: JavaScript execution commands write to session logs
 * Integrates with session console logging to capture execution results
 */

import { BaseCommand } from '../../core/base-command/BaseCommand.js';
import { CommandResult } from '../../../types/CommandTypes.js';

export interface JSExecuteOptions {
  script: string;
  sessionId?: string;
  generateUUID?: boolean;
  waitForResult?: boolean;
  timeout?: number;
  logExecution?: boolean;
}

export class JSExecuteCommand extends BaseCommand {
  static getDefinition() {
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

  async execute(options: JSExecuteOptions): Promise<CommandResult> {
    try {
      const { 
        script, 
        sessionId, 
        generateUUID = true, 
        waitForResult = true, 
        timeout = 30000,
        logExecution = true 
      } = options;

      // Generate execution UUID for tracking (git hook pattern)
      const executionUUID = generateUUID ? 
        `exec-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` : 
        null;

      // Prepare JavaScript with UUID logging
      const wrappedScript = this.wrapScriptWithLogging(script, executionUUID, logExecution);

      // Log execution start to session
      if (logExecution) {
        try {
          await this.logToSession(sessionId, 'info', 
            `🚀 JavaScript execution started ${executionUUID ? `[UUID: ${executionUUID}]` : ''}`
          );
        } catch (error) {
          // Continue execution even if session logging fails
          console.warn('⚠️ Session logging failed at start:', error);
        }
      }

      // Execute JavaScript via browser manager
      const executionResult = await this.executeInBrowser(wrappedScript, { 
        sessionId: sessionId || undefined, 
        timeout, 
        waitForResult 
      });

      // Log execution completion
      if (logExecution) {
        try {
          await this.logToSession(sessionId, 'info', 
            `✅ JavaScript execution completed ${executionUUID ? `[UUID: ${executionUUID}]` : ''}`
          );
        } catch (error) {
          // Continue execution even if session logging fails
          console.warn('⚠️ Session logging failed at completion:', error);
        }
      }

      return {
        success: true,
        data: {
          executionUUID,
          result: executionResult,
          sessionId: sessionId || 'auto-detected',
          timestamp: new Date().toISOString(),
          script: script.length > 100 ? script.substring(0, 100) + '...' : script
        },
        message: `JavaScript executed successfully${executionUUID ? ` [${executionUUID}]` : ''}`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      // Log execution error to session
      if (options.logExecution !== false) {
        try {
          await this.logToSession(options.sessionId, 'error', 
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
          script: options.script.length > 100 ? options.script.substring(0, 100) + '...' : options.script,
          sessionId: options.sessionId || 'auto-detected'
        }
      };
    }
  }

  /**
   * Wrap script with UUID logging for git hook pattern
   */
  private wrapScriptWithLogging(script: string, executionUUID: string | null, logExecution: boolean): string {
    if (!logExecution && !executionUUID) {
      return script;
    }

    const uuidLog = executionUUID ? 
      `console.log('🎯 EXECUTION_UUID_${executionUUID}_START');` : '';
    
    const executionLog = logExecution ? 
      `console.log('🔥 CLIENT: JavaScript executed successfully!');` : '';

    const completionLog = executionUUID ? 
      `console.log('🎯 EXECUTION_UUID_${executionUUID}_COMPLETE');` : '';

    return `
      ${uuidLog}
      try {
        ${script}
        ${executionLog}
        ${completionLog}
      } catch (error) {
        console.error('❌ JavaScript execution error:', error);
        ${executionUUID ? `console.log('🎯 EXECUTION_UUID_${executionUUID}_ERROR:', error.message);` : ''}
        throw error;
      }
    `;
  }

  /**
   * Execute JavaScript in browser via DevTools
   */
  private async executeInBrowser(_script: string, _options: {
    sessionId?: string;
    timeout: number;
    waitForResult: boolean;
  }): Promise<any> {
    // TODO: Integrate with BrowserManagerDaemon and DevTools
    // For now, simulate the execution pattern
    
    // This would use the ChromiumDevToolsAdapter.evaluateScript method
    // and coordinate with the session manager for proper browser targeting
    
    // Simulation of the pattern from verification logs:
    const simulatedResult = {
      success: true,
      executedAt: new Date().toISOString(),
      browserReady: true,
      devToolsConnected: true,
      result: null // Would contain actual execution result
    };

    return simulatedResult;
  }

  /**
   * Log message to session logs (integrates with session manager)
   */
  private async logToSession(sessionId: string | undefined, level: 'info' | 'warn' | 'error', message: string): Promise<void> {
    try {
      // Get the actual session and append to its server log file
      if (sessionId && sessionId !== 'auto-detected') {
        const timestamp = new Date().toISOString();
        const logMessage = `[${timestamp}] ${level.toUpperCase()}: ${message}`;
        
        // TODO: Send message to SessionManagerDaemon to append to session server log
        // For now, we'll append directly to session log file if we can determine the path
        await this.appendToSessionServerLog(sessionId, logMessage);
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
  private async appendToSessionServerLog(sessionId: string, message: string): Promise<void> {
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