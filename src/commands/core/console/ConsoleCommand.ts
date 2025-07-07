/**
 * Console Command - Bridge browser console logs back to development portal
 * 
 * CRITICAL FOR JTAG: This enables autonomous development by forwarding
 * browser console logs, errors, and health reports back to the AI portal
 */

import { DirectCommand } from '../direct-command/DirectCommand';
import { CommandResult, WebSocketCommandContext, CommandDefinition } from '../base-command/BaseCommand';

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

  protected static async executeOperation(params: any = {}, context: WebSocketCommandContext): Promise<CommandResult> {
    try {
      // BROWSER LOGS FIX: Write to server log immediately to verify execution
      console.log(`🎯 BROWSER LOG FIX: ConsoleCommand.executeOperation CALLED!`);
      console.log(`🎯 Parameters received:`, params);
      console.log(`🎯 Context sessionId:`, context.sessionId);
      
      const { action, message, source, data } = params;
      
      if (!action || !message) {
        console.log(`🎯 MISSING PARAMS: action=${action}, message=${message}`);
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
        // DEBUG: Log actual context structure to understand what we're receiving
        console.log(`🔍 CONSOLE_COMMAND_DEBUG: Full context received:`, JSON.stringify(context, null, 2));
        console.log(`🔍 CONSOLE_COMMAND_DEBUG: params:`, JSON.stringify(params, null, 2));
        
        // sessionId is now guaranteed to be non-null by TypeScript
        let sessionId = context.sessionId;
        
        // If no sessionId provided, use the current/default shared development session
        if (!sessionId) {
          // Try to find the most recent development session
          try {
            const { readdirSync, statSync } = await import('fs');
            const { join } = await import('path');
            
            const sessionsPath = '.continuum/sessions/user/shared';
            const sessionDirs = readdirSync(sessionsPath).filter(dir => {
              const fullPath = join(sessionsPath, dir);
              return statSync(fullPath).isDirectory() && dir.startsWith('development-shared-');
            });
            
            if (sessionDirs.length > 0) {
              // Sort by modification time to get the most recent
              const sortedSessions = sessionDirs.sort((a, b) => {
                const aTime = statSync(join(sessionsPath, a)).mtimeMs;
                const bTime = statSync(join(sessionsPath, b)).mtimeMs;
                return bTime - aTime; // Most recent first
              });
              
              sessionId = sortedSessions[0];
              console.log(`📝 No sessionId in context - using most recent shared session: ${sessionId}`);
            }
          } catch (error) {
            console.log(`📝 Could not find existing sessions: ${error}`);
          }
          
          // If still no session, we can't log to session files
          if (!sessionId) {
            console.log(`📝 No active session found - logging to server console only`);
            return this.createSuccessResult('Console message forwarded to server console', {
              forwarded: true,
              timestamp,
              consoleEntry,
              sessionLogged: false
            });
          }
        }

        const fs = await import('fs/promises');
        
        // Format log entry for session file
        const logEntry = `[${timestamp}] ${icon} BROWSER CONSOLE [${source}]: ${message}`;
        
        // Try to find the specific session's browser log
        // Sessions are organized as: .continuum/sessions/user/shared/{sessionId}/logs/browser.log
        const sessionBasePaths = [
          '.continuum/sessions/user/shared',
          '.continuum/sessions/user/development', 
          '.continuum/sessions/shared/development',
          '.continuum/sessions/shared/shared'
        ];
        
        for (const basePath of sessionBasePaths) {
          const browserLogPath = `${basePath}/${sessionId}/logs/browser.log`;
          try {
            console.log(`🔍 CONSOLE_COMMAND_DEBUG: Trying browser log path: ${browserLogPath}`);
            
            // Check if this specific session's browser.log exists
            await fs.access(browserLogPath);
            
            console.log(`✅ CONSOLE_COMMAND_DEBUG: Found browser log at: ${browserLogPath}`);
            
            // Write to this session's browser log only
            await fs.appendFile(browserLogPath, logEntry + '\n');
            
            // Add data on separate line if present
            if (data && Object.keys(data).length > 0) {
              await fs.appendFile(browserLogPath, `   Data: ${JSON.stringify(data)}\n`);
            }
            
            console.log(`✅ CONSOLE_COMMAND_DEBUG: Successfully wrote to browser log: ${sessionId}`);
            sessionLogged = true;
            break; // Found the session, stop looking
          } catch (accessError) {
            console.log(`🔍 CONSOLE_COMMAND_DEBUG: Browser log not found at: ${browserLogPath} - ${accessError}`);
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