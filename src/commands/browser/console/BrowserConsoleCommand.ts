/**
 * Browser Console Command - Interact with browser console via session logging
 * 
 * Layer 7: Console commands that integrate with session console logging
 * Enables reading console output and executing console commands with session tracking
 */

import { BaseCommand } from '../../core/base-command/BaseCommand.js';
import { CommandResult } from '../../../types/CommandTypes.js';

export interface BrowserConsoleOptions {
  action: 'read' | 'clear' | 'execute' | 'monitor';
  sessionId?: string;
  script?: string; // For execute action
  lines?: number; // For read action - how many recent lines
  filter?: 'all' | 'log' | 'warn' | 'error'; // For read action
  follow?: boolean; // For monitor action - stream updates
}

export class BrowserConsoleCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'browser-console',
      description: 'Interact with browser console through session logging system',
      category: 'browser',
      parameters: {
        action: {
          type: 'string' as const,
          description: 'Console action to perform',
          required: true,
          enum: ['read', 'clear', 'execute', 'monitor']
        },
        sessionId: {
          type: 'string' as const,
          description: 'Session ID (auto-detected if not provided)',
          required: false
        },
        script: {
          type: 'string' as const,
          description: 'JavaScript to execute via console (for execute action)',
          required: false
        },
        lines: {
          type: 'number' as const,
          description: 'Number of recent console lines to read',
          required: false,
          default: 50
        },
        filter: {
          type: 'string' as const,
          description: 'Filter console output by level',
          required: false,
          enum: ['all', 'log', 'warn', 'error'],
          default: 'all'
        },
        follow: {
          type: 'boolean' as const,
          description: 'Stream live console updates (for monitor action)',
          required: false,
          default: false
        }
      },
      examples: [
        {
          description: 'Read recent console output from session',
          input: { action: 'read', lines: 20 }
        },
        {
          description: 'Execute console command with session tracking',
          input: { 
            action: 'execute', 
            script: 'console.log("Hello from console command!")'
          }
        },
        {
          description: 'Monitor console for errors only',
          input: { 
            action: 'monitor', 
            filter: 'error',
            follow: true
          }
        }
      ]
    };
  }

  async execute(options: BrowserConsoleOptions): Promise<CommandResult> {
    try {
      const { action, sessionId, lines = 50, filter = 'all' } = options;

      switch (action) {
        case 'read':
          return await this.readConsoleOutput(sessionId, lines, filter);
          
        case 'clear':
          return await this.clearConsole(sessionId);
          
        case 'execute':
          if (!options.script) {
            return {
              success: false,
              error: 'Script parameter required for execute action'
            };
          }
          return await this.executeConsoleScript(sessionId, options.script);
          
        case 'monitor':
          return await this.monitorConsole(sessionId, filter, options.follow);
          
        default:
          return {
            success: false,
            error: `Unknown console action: ${action}`
          };
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Browser console command failed: ${errorMessage}`
      };
    }
  }

  /**
   * Read console output from session browser log
   */
  private async readConsoleOutput(sessionId: string | undefined, lines: number, filter: string): Promise<CommandResult> {
    try {
      // TODO: Integrate with SessionManagerDaemon to read actual browser.log
      // This would read from session.artifacts.logs.client[0]
      
      const consoleOutput = await this.getSessionConsoleLog(sessionId);
      const filteredOutput = this.filterConsoleOutput(consoleOutput, filter);
      const recentOutput = filteredOutput.slice(-lines);

      return {
        success: true,
        data: {
          sessionId: sessionId || 'auto-detected',
          lines: recentOutput.length,
          filter,
          output: recentOutput,
          totalAvailable: filteredOutput.length
        },
        message: `Retrieved ${recentOutput.length} console lines (filter: ${filter})`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to read console output: ${errorMessage}`
      };
    }
  }

  /**
   * Clear browser console via DevTools
   */
  private async clearConsole(sessionId: string | undefined): Promise<CommandResult> {
    try {
      // Generate UUID for tracking the clear operation
      const clearUUID = `clear-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Execute console.clear() via DevTools
      const clearScript = `
        console.clear();
        console.log('🧹 Console cleared [UUID: ${clearUUID}]');
      `;
      
      await this.executeInBrowser(clearScript, sessionId);

      return {
        success: true,
        data: {
          sessionId: sessionId || 'auto-detected',
          clearUUID,
          timestamp: new Date().toISOString()
        },
        message: `Console cleared [${clearUUID}]`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to clear console: ${errorMessage}`
      };
    }
  }

  /**
   * Execute JavaScript via console with session tracking
   */
  private async executeConsoleScript(sessionId: string | undefined, script: string): Promise<CommandResult> {
    try {
      const executionUUID = `console-exec-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Wrap script with UUID tracking (git hook pattern)
      const wrappedScript = `
        console.log('🎯 CONSOLE_EXEC_UUID_${executionUUID}_START');
        try {
          const result = (function() {
            ${script}
          })();
          console.log('🎯 CONSOLE_EXEC_UUID_${executionUUID}_RESULT:', result);
          console.log('🎯 CONSOLE_EXEC_UUID_${executionUUID}_COMPLETE');
          return result;
        } catch (error) {
          console.error('❌ Console execution error:', error);
          console.log('🎯 CONSOLE_EXEC_UUID_${executionUUID}_ERROR:', error.message);
          throw error;
        }
      `;

      const result = await this.executeInBrowser(wrappedScript, sessionId);

      return {
        success: true,
        data: {
          sessionId: sessionId || 'auto-detected',
          executionUUID,
          script: script.length > 100 ? script.substring(0, 100) + '...' : script,
          result,
          timestamp: new Date().toISOString()
        },
        message: `Console script executed [${executionUUID}]`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Console script execution failed: ${errorMessage}`
      };
    }
  }

  /**
   * Monitor console output with optional live streaming
   */
  private async monitorConsole(sessionId: string | undefined, filter: string, follow: boolean): Promise<CommandResult> {
    try {
      // Start monitoring console via session console logger
      const monitorId = `monitor-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // TODO: Integrate with SessionConsoleLogger for live monitoring
      // This would set up a listener for new console messages
      
      const currentOutput = await this.getSessionConsoleLog(sessionId);
      const filteredOutput = this.filterConsoleOutput(currentOutput, filter);

      return {
        success: true,
        data: {
          sessionId: sessionId || 'auto-detected',
          monitorId,
          filter,
          follow,
          currentLines: filteredOutput.length,
          latestEntries: filteredOutput.slice(-10), // Last 10 entries
          monitoring: follow ? 'active' : 'snapshot'
        },
        message: `Console monitoring ${follow ? 'started' : 'snapshot taken'} [${monitorId}]`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Console monitoring failed: ${errorMessage}`
      };
    }
  }

  /**
   * Get session console log content
   */
  private async getSessionConsoleLog(sessionId: string | undefined): Promise<string[]> {
    // TODO: Integrate with SessionManagerDaemon
    // This would read from the actual session browser.log file
    
    // For now, return simulated console output showing the git hook pattern
    return [
      '🌐 [2025-07-01T15:03:12.208Z] CLEAR: console.clear',
      '🌐 [2025-07-01T15:03:12.209Z] LOG: 🔥 CLIENT: JavaScript executed successfully!',
      '🌐 [2025-07-01T15:03:12.210Z] LOG: 🎯 EXECUTION_UUID_abc123_START',
      '🌐 [2025-07-01T15:03:12.211Z] LOG: Hello from session!',
      '🌐 [2025-07-01T15:03:12.212Z] LOG: 🎯 EXECUTION_UUID_abc123_COMPLETE'
    ];
  }

  /**
   * Filter console output by level
   */
  private filterConsoleOutput(output: string[], filter: string): string[] {
    if (filter === 'all') {
      return output;
    }

    const levelFilter = filter.toUpperCase();
    return output.filter(line => line.includes(`] ${levelFilter}:`));
  }

  /**
   * Execute JavaScript in browser via DevTools
   */
  private async executeInBrowser(script: string, sessionId: string | undefined): Promise<any> {
    // TODO: Integrate with ChromiumDevToolsAdapter
    // This would use the same DevTools connection as SessionConsoleLogger
    
    console.log(`🚀 Browser execution [${sessionId || 'auto'}]:`, script.substring(0, 100));
    
    // Simulate execution result
    return {
      success: true,
      executedAt: new Date().toISOString(),
      sessionId: sessionId || 'auto-detected'
    };
  }
}