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

      // Write to specific session browser log only (session affinity)
      let sessionLogged = false;
      try {
        // Get sessionId from context (passed from WebSocketDaemon connection mapping)
        // Context structure: _context can have sessionId directly or nested in context
        const sessionId = _context?.sessionId || _context?.context?.sessionId;
        
        if (!sessionId) {
          console.warn(`⚠️ No sessionId in context - cannot determine which session to log to`);
          return this.createSuccessResult('Console message forwarded to server console only', {
            forwarded: true,
            timestamp,
            consoleEntry,
            sessionLogged: false,
            warning: 'No session context - logged to server console only'
          });
        }

        const fs = await import('fs/promises');
        
        // Format log entry for session file
        const logEntry = `[${timestamp}] ${icon} BROWSER CONSOLE [${source}]: ${message}`;
        
        // Try to find the specific session's browser log
        const sessionBasePaths = [
          '.continuum/sessions/user/user',
          '.continuum/sessions/user/system'
        ];
        
        for (const basePath of sessionBasePaths) {
          const browserLogPath = `${basePath}/${sessionId}/logs/browser.log`;
          try {
            // Check if this specific session's browser.log exists
            await fs.access(browserLogPath);
            
            // Write to this session's browser log only
            await fs.appendFile(browserLogPath, logEntry + '\n');
            
            // Add data on separate line if present
            if (data && Object.keys(data).length > 0) {
              await fs.appendFile(browserLogPath, `   Data: ${JSON.stringify(data)}\n`);
            }
            
            sessionLogged = true;
            break; // Found the session, stop looking
          } catch {
            // This session path doesn't exist, try next base path
          }
        }
        
        if (!sessionLogged) {
          console.warn(`⚠️ Session ${sessionId} browser.log not found - console message not logged to session file`);
        }
        
      } catch (error) {
        console.warn(`⚠️ Failed to write to session browser log: ${error instanceof Error ? error.message : String(error)}`);
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