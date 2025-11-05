/**
 * Browser Console Command - Interact with browser console via session logging
 * 
 * Layer 7: Console commands that integrate with session console logging
 * Enables reading console output and executing console commands with session tracking
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand.js';
import { CommandDefinition, CommandResult, ContinuumContext } from '../../core/base-command/BaseCommand.js';

export interface BrowserConsoleOptions {
  action: 'read' | 'clear' | 'execute' | 'monitor';
  sessionId?: string;
  script?: string; // For execute action
  lines?: number; // For read action - how many recent lines
  filter?: 'all' | 'log' | 'warn' | 'error'; // For read action
  follow?: boolean; // For monitor action - stream updates
}

export class BrowserConsoleCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'console',
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
          command: 'console --action=read --lines=20'
        },
        {
          description: 'Execute console command with session tracking',
          command: 'console --action=execute --script="console.log(\"Hello from console command!\")"'
        },
        {
          description: 'Monitor console for errors only',
          command: 'console --action=monitor --filter=error --follow=true'
        }
      ]
    };
  }

  protected static async executeOperation(params: BrowserConsoleOptions, _context: ContinuumContext): Promise<CommandResult> {
    // Apply defaults and validate required parameters
    const options = this.applyDefaults<BrowserConsoleOptions>(params, {
      lines: 50,
      filter: 'all'
    });

    this.validateRequiredParams(options, ['action']);

    const { action, sessionId, lines, filter } = options;

    switch (action) {
      case 'read':
        return await BrowserConsoleCommand.readConsoleOutput(sessionId, lines || 50, filter || 'all');
        
      case 'clear':
        return await BrowserConsoleCommand.clearConsole(sessionId);
        
      case 'execute':
        if (!options.script) {
          return this.createErrorResult('Script parameter required for execute action');
        }
        return await BrowserConsoleCommand.executeConsoleScript(sessionId, options.script);
        
      case 'monitor':
        return await BrowserConsoleCommand.monitorConsole(sessionId, filter || 'all', options.follow || false);
        
      default:
        return this.createErrorResult(`Unknown console action: ${action}`);
    }
  }

  /**
   * Read console output from session browser log
   */
  private static async readConsoleOutput(sessionId: string | undefined, lines: number, filter: string): Promise<CommandResult> {
    try {
      // TODO: Integrate with SessionManagerDaemon to read actual browser.log
      // This would read from session.artifacts.logs.client[0]
      
      const consoleOutput = await BrowserConsoleCommand.getSessionConsoleLog(sessionId);
      const filteredOutput = BrowserConsoleCommand.filterConsoleOutput(consoleOutput, filter);
      const recentOutput = filteredOutput.slice(-lines);

      return this.createSuccessResult(
        `Retrieved ${recentOutput.length} console lines (filter: ${filter})`,
        {
          sessionId: sessionId || 'auto-detected',
          lines: recentOutput.length,
          filter,
          output: recentOutput,
          totalAvailable: filteredOutput.length
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to read console output: ${errorMessage}`);
    }
  }

  /**
   * Clear browser console via DevTools
   */
  private static async clearConsole(sessionId: string | undefined): Promise<CommandResult> {
    try {
      // Generate UUID for tracking the clear operation
      const clearUUID = `clear-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Execute console.clear() via DevTools
      const clearScript = `
        console.clear();
        console.log('🧹 Console cleared [UUID: ${clearUUID}]');
      `;
      
      await BrowserConsoleCommand.executeInBrowser(clearScript, sessionId);

      return this.createSuccessResult(
        `Console cleared [${clearUUID}]`,
        {
          sessionId: sessionId || 'auto-detected',
          clearUUID,
          timestamp: new Date().toISOString()
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to clear console: ${errorMessage}`);
    }
  }

  /**
   * Execute JavaScript via console with session tracking
   */
  private static async executeConsoleScript(sessionId: string | undefined, script: string): Promise<CommandResult> {
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

      const result = await BrowserConsoleCommand.executeInBrowser(wrappedScript, sessionId);

      return this.createSuccessResult(
        `Console script executed [${executionUUID}]`,
        {
          sessionId: sessionId || 'auto-detected',
          executionUUID,
          script: script.length > 100 ? script.substring(0, 100) + '...' : script,
          result,
          timestamp: new Date().toISOString()
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Console script execution failed: ${errorMessage}`);
    }
  }

  /**
   * Monitor console output with optional live streaming
   */
  private static async monitorConsole(sessionId: string | undefined, filter: string, follow: boolean): Promise<CommandResult> {
    try {
      // Start monitoring console via session console logger
      const monitorId = `monitor-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // TODO: Integrate with SessionConsoleLogger for live monitoring
      // This would set up a listener for new console messages
      
      const currentOutput = await BrowserConsoleCommand.getSessionConsoleLog(sessionId);
      const filteredOutput = BrowserConsoleCommand.filterConsoleOutput(currentOutput, filter);

      return this.createSuccessResult(
        `Console monitoring ${follow ? 'started' : 'snapshot taken'} [${monitorId}]`,
        {
          sessionId: sessionId || 'auto-detected',
          monitorId,
          filter,
          follow,
          currentLines: filteredOutput.length,
          latestEntries: filteredOutput.slice(-10), // Last 10 entries
          monitoring: follow ? 'active' : 'snapshot'
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Console monitoring failed: ${errorMessage}`);
    }
  }

  /**
   * Get session console log content
   */
  private static async getSessionConsoleLog(_sessionId: string | undefined): Promise<string[]> {
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
  private static filterConsoleOutput(output: string[], filter: string): string[] {
    if (filter === 'all') {
      return output;
    }

    const levelFilter = filter.toUpperCase();
    return output.filter(line => line.includes(`] ${levelFilter}:`));
  }

  /**
   * Execute JavaScript in browser via DevTools
   */
  private static async executeInBrowser(script: string, sessionId: string | undefined): Promise<any> {
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