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
          input: { script: 'console.log("Hello from session!")' }
        },
        {
          description: 'Execute with custom session and no UUID',
          input: { 
            script: 'document.title = "Test Page"',
            sessionId: 'my-session-123',
            generateUUID: false
          }
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
        await this.logToSession(sessionId, 'info', 
          `🚀 JavaScript execution started ${executionUUID ? `[UUID: ${executionUUID}]` : ''}`
        );
      }

      // Execute JavaScript via browser manager
      const executionResult = await this.executeInBrowser(wrappedScript, { 
        sessionId, 
        timeout, 
        waitForResult 
      });

      // Log execution completion
      if (logExecution) {
        await this.logToSession(sessionId, 'info', 
          `✅ JavaScript execution completed ${executionUUID ? `[UUID: ${executionUUID}]` : ''}`
        );
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
        await this.logToSession(options.sessionId, 'error', 
          `❌ JavaScript execution failed: ${errorMessage}`
        );
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
  private async executeInBrowser(script: string, options: {
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
      // TODO: Integrate with SessionManagerDaemon logging
      // This would send a message to session manager to write to session logs
      
      const timestamp = new Date().toISOString();
      const logMessage = `[${timestamp}] ${level.toUpperCase()}: ${message}`;
      
      // For now, log to console (Layer 8 will implement proper session integration)
      console.log(`📝 Session Log [${sessionId || 'auto'}]: ${logMessage}`);
      
    } catch (error) {
      console.warn('⚠️ Failed to log to session:', error);
    }
  }
}