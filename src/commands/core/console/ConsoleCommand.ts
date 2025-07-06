/**
 * Console Command - Bridge browser console logs back to development portal
 * 
 * CRITICAL FOR JTAG: This enables autonomous development by forwarding
 * browser console logs, errors, and health reports back to the AI portal
 */

import { DirectCommand } from '../direct-command/DirectCommand.js';
import { CommandResult, CommandContext, CommandDefinition } from '../base-command/BaseCommand.js';

export class ConsoleCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'console',
      description: 'Forward browser console logs to development portal for JTAG autonomous development',
      category: 'core',
      parameters: {
        action: {
          type: 'string',
          description: 'log, error, warn, info, health_report',
          required: true
        },
        message: {
          type: 'string', 
          description: 'Console message content',
          required: true
        },
        source: {
          type: 'string',
          description: 'Source component (widget, api, etc)',
          required: false
        },
        data: {
          type: 'object',
          description: 'Additional data (health metrics, error details, etc)',
          required: false
        }
      },
      examples: [
        {
          description: 'Forward health report',
          command: 'console --action=health_report --message="System healthy" --data=healthReport'
        },
        {
          description: 'Forward error',
          command: 'console --action=error --message="Widget failed to load" --source=chat-widget'
        }
      ]
    };
  }

  protected static async executeOperation(params: any = {}, _context?: CommandContext): Promise<CommandResult> {
    try {
      const { action, message, source, data } = params;
      
      // DEBUG: Log all console commands received (uncomment for debugging)
      // console.log(`[ConsoleCommand] Received: action=${action}, source=${source}, message=${message?.substring(0, 100)}...`);
      // console.log(`[ConsoleCommand] Context sessionId: ${_context?.sessionId}, params sessionId: ${params.sessionId}`);
      
      if (!action || !message) {
        return this.createErrorResult('Console forwarding requires action and message parameters');
      }

      // Format console message for portal
      const timestamp = new Date().toISOString();
      const consoleEntry = {
        timestamp,
        action,
        message,
        source: source || 'browser',
        data: data || {},
        environment: 'browser'
      };

      // Format console output
      const iconMap: Record<string, string> = {
        'log': '📝',
        'error': '❌', 
        'warn': '⚠️',
        'info': 'ℹ️',
        'health_report': '🏥'
      };
      const icon = iconMap[action as string] || '📝';

      // Log to server console (always)
      console.log(`${icon} PORTAL BRIDGE [${source}]: ${message}`);
      if (data && Object.keys(data).length > 0) {
        console.log(`   Data:`, data);
      }

      // Write to ALL active session browser logs (fix for inconsistent sessionId routing)
      let sessionLogged = false;
      try {
        const fs = await import('fs/promises');
        
        // Format log entry for session file
        const logEntry = `[${timestamp}] ${icon} BROWSER CONSOLE [${source}]: ${message}`;
        
        // Find ALL active session directories and write to them
        const sessionBasePaths = [
          '.continuum/sessions/user/user',
          '.continuum/sessions/user/system'
        ];
        
        let loggedToAnySession = false;
        for (const basePath of sessionBasePaths) {
          try {
            const sessions = await fs.readdir(basePath);
            for (const sessionDir of sessions) {
              const browserLogPath = `${basePath}/${sessionDir}/logs/browser.log`;
              try {
                // Check if browser.log exists
                await fs.access(browserLogPath);
                
                // Write to this session's browser log
                await fs.appendFile(browserLogPath, logEntry + '\n');
                
                // Add data on separate line if present
                if (data && Object.keys(data).length > 0) {
                  await fs.appendFile(browserLogPath, `   Data: ${JSON.stringify(data)}\n`);
                }
                
                loggedToAnySession = true;
              } catch {
                // This session's browser.log doesn't exist or isn't accessible, skip it
              }
            }
          } catch {
            // This base path doesn't exist, skip it
          }
        }
        
        if (loggedToAnySession) {
          sessionLogged = true;
        } else {
          console.warn(`⚠️ No active sessions found - console message not logged to any browser.log files`);
        }
      } catch (error) {
        console.warn(`⚠️ Failed to write to session browser logs: ${error instanceof Error ? error.message : String(error)}`);
      }

      return this.createSuccessResult(
        'Console message forwarded successfully',
        {
          forwarded: true,
          timestamp,
          consoleEntry,
          sessionLogged
        }
      );
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Console forwarding failed: ${errorMessage}`);
    }
  }
}