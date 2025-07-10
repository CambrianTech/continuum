/**
 * Console Command - Bridge browser console logs back to development portal
 * 
 * CRITICAL FOR JTAG: This enables autonomous development by forwarding
 * browser console logs, errors, and health reports back to the AI portal
 */

import { DirectCommand } from '../direct-command/DirectCommand';
import { CommandResult, WebSocketCommandContext, CommandDefinition } from '../base-command/BaseCommand';
import { BrowserConsoleLogForwarding } from '../../../types/shared/WebSocketCommunication';

export class ConsoleCommand extends DirectCommand {
  /**
   * Normalize any client input format into our well-defined shared type
   * Server-side only - ensures type safety regardless of what client sends
   */
  private static normalizeToSharedType(params: any): BrowserConsoleLogForwarding {
    // If already in shared type format, use as-is
    if (params.consoleLogLevel && params.consoleMessage) {
      return params as BrowserConsoleLogForwarding;
    }
    
    // Legacy format - convert to shared type
    const { action, message, source, data, level, args, arguments: consoleArguments, timestamp, ...rest } = params;
    
    return {
      consoleLogLevel: (action || level || 'log') as 'debug' | 'log' | 'info' | 'warn' | 'error',
      consoleMessage: message || '',
      consoleArguments: Array.isArray(args || consoleArguments) ? (args || consoleArguments).map((arg: any) => {
        // Handle enhanced Console.ConsoleArgument format from shared types
        if (arg && typeof arg === 'object' && arg.type === 'object' && arg.stringRepresentation) {
          // This is a properly serialized ObjectArgument from Console.MessageUtils
          return {
            argumentType: 'object' as const,
            argumentValue: arg.stringRepresentation,
            originalValue: arg.value
          };
        }
        
        // Handle simple types
        const argType = typeof arg;
        const mappedType = argType === 'bigint' || argType === 'symbol' ? 'string' : 
                          arg === null ? 'null' : argType;
        return {
          argumentType: mappedType as 'string' | 'number' | 'boolean' | 'object' | 'function' | 'undefined' | 'null',
          argumentValue: String(arg),
          originalValue: arg
        };
      }) : [],
      browserContext: {
        sourceFileName: data?.fileName || rest.fileName,
        sourceLineNumber: data?.lineNumber || rest.lineNumber,
        sourceColumnNumber: data?.columnNumber || rest.columnNumber,
        functionName: data?.functionName || rest.functionName,
        stackTrace: data?.stackTrace || rest.stackTrace || '',
        currentUrl: data?.url || rest.url || '',
        userAgent: data?.userAgent || rest.userAgent || '',
        viewportWidth: data?.viewport?.width || rest.viewportWidth || 0,
        viewportHeight: data?.viewport?.height || rest.viewportHeight || 0,
        timestamp: timestamp || data?.timestamp || new Date().toISOString()
      }
    };
  }

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
      // Convert whatever the client sends into our well-defined shared type
      // This ensures server-side type safety regardless of client format
      const logData: BrowserConsoleLogForwarding = this.normalizeToSharedType(params);
      
      if (!logData.consoleLogLevel || !logData.consoleMessage) {
        console.log(`🎯 MISSING PARAMS: level=${logData.consoleLogLevel}, message=${logData.consoleMessage}`);
        return this.createErrorResult('Console forwarding requires consoleLogLevel and consoleMessage parameters');
      }

      // Add server-side timestamp and create structured log entry
      const serverTimestamp = new Date().toISOString();
      
      // Decode base64 JavaScript if present (for probe level)
      const enhancedLogData = { ...logData };
      if (logData.consoleLogLevel === 'probe' as any && logData.consoleArguments?.length > 0) {
        const probeArg = logData.consoleArguments[0];
        if (probeArg?.originalValue && typeof probeArg.originalValue === 'object' && 'executeJSBase64' in probeArg.originalValue) {
          try {
            const executeJSBase64 = (probeArg.originalValue as any).executeJSBase64;
            const decodedJS = Buffer.from(executeJSBase64, 'base64').toString('utf-8');
            // Add decoded JS to server logs for debugging (but keep base64 in stored logs)
            console.log(`🔬 PROBE JS Code: ${decodedJS}`);
          } catch (error) {
            console.warn(`⚠️ Failed to decode probe JavaScript: ${error}`);
          }
        }
      }
      
      const logEntry = {
        ...enhancedLogData,
        serverTimestamp,
        sessionId: context.sessionId
      };
      
      const timePrefix = `[${new Date().toLocaleTimeString()}]`;

      // Log to server console (always) with timestamp
      console.log(`[${timePrefix} console.${logData.consoleLogLevel}]: ${logData.consoleMessage}`);
      if (logData.consoleArguments && logData.consoleArguments.length > 0) {
        console.log(`   Args:`, logData.consoleArguments);
      }

      // Write to session-specific log files with level-based JSON format
      let sessionLogged = false;
      try {
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
              serverTimestamp,
              logData,
              sessionLogged: false
            });
          }
        }

        const fs = await import('fs/promises');
        const { join } = await import('path');
        
        // Try to find the specific session's logs directory
        const sessionBasePaths = [
          '.continuum/sessions/user/shared',
          '.continuum/sessions/user/development', 
          '.continuum/sessions/shared/development',
          '.continuum/sessions/shared/shared'
        ];
        
        for (const basePath of sessionBasePaths) {
          const sessionLogsDir = join(basePath, sessionId, 'logs');
          try {
            // Check if this session's logs directory exists
            await fs.access(sessionLogsDir);
            
            // Write to level-specific JSON log files following naming convention
            const levelLogPath = join(sessionLogsDir, `browser.${logData.consoleLogLevel}.json`);
            const allLogsPath = join(sessionLogsDir, 'browser.log');
            
            // Handle probe logs specially for better readability
            let jsonLogEntry: string;
            let humanReadableEntry: string;
            
            if (logData.consoleLogLevel === 'probe' as any) {
              // For probe logs, format for human readability with decoded JS
              // Probe data is in consoleMessage as JSON string
              let probeData: any = null;
              try {
                probeData = JSON.parse(logData.consoleMessage);
              } catch (error) {
                // Fallback: check consoleArguments for legacy format
                probeData = logData.consoleArguments?.[0]?.originalValue;
              }
              
              if (probeData && typeof probeData === 'object') {
                const probe = probeData as any;
                
                // Create readable probe entry
                const readableProbe = {
                  message: probe.message,
                  category: probe.category,
                  tags: probe.tags,
                  data: probe.data,
                  executeJS: probe.executeJSBase64 ? 
                    Buffer.from(probe.executeJSBase64, 'base64').toString('utf-8') : 
                    probe.executeJS,
                  timestamp: serverTimestamp,
                  sessionId: context.sessionId
                };
                
                // Formatted JSON for probe file (pretty-printed)
                jsonLogEntry = JSON.stringify(readableProbe, null, 2) + '\n';
                
                // Human-readable format for browser.log
                humanReadableEntry = `[${serverTimestamp}] 🛸 PROBE: ${probe.message}`;
                if (probe.category) {
                  humanReadableEntry += ` (${probe.category})`;
                }
                humanReadableEntry += '\n';
                
                // Add JS execution if present
                if (readableProbe.executeJS) {
                  humanReadableEntry += `  JS: ${readableProbe.executeJS}\n`;
                }
                
                // Add execution result if present
                if (probe.data?.jsExecutionResult) {
                  humanReadableEntry += `  Result: ${probe.data.jsExecutionResult}\n`;
                }
                
                humanReadableEntry += '\n';
              } else {
                // Fallback for malformed probe data
                jsonLogEntry = JSON.stringify(logEntry, null, 2) + '\n';
                humanReadableEntry = `[${serverTimestamp}] 🛸 PROBE: ${logData.consoleMessage}\n`;
              }
            } else {
              // Standard log format for non-probe logs
              jsonLogEntry = JSON.stringify(logEntry) + '\n';
              
              humanReadableEntry = `[${serverTimestamp}] ${logData.consoleLogLevel.toUpperCase()}: ${logData.consoleMessage}`;
              
              // Add arguments if present, with JSON formatting for objects
              if (logData.consoleArguments && logData.consoleArguments.length > 0) {
                const formattedArgs = logData.consoleArguments.map(arg => {
                  if (arg.argumentType === 'object' && arg.argumentValue) {
                    return arg.argumentValue; // This is already JSON formatted
                  } else {
                    return arg.argumentValue;
                  }
                }).join(' ');
                humanReadableEntry += ` ${formattedArgs}`;
              }
              humanReadableEntry += '\n';
            }
            
            // Write to both formats: level-specific JSON and human-readable for browser.log
            await Promise.all([
              fs.appendFile(levelLogPath, jsonLogEntry),        // browser.probe.json (readable JSON) or browser.warn.json (compact JSON)
              fs.appendFile(allLogsPath, humanReadableEntry)    // browser.log (human-readable)
            ]);
            
            console.log(`✅ Wrote to browser logs: ${sessionId} (${logData.consoleLogLevel})`);
            sessionLogged = true;
            break; // Found the session, stop looking
          } catch (accessError) {
            // This session path doesn't exist, try next base path
            continue;
          }
        }
        
        if (!sessionLogged) {
          console.warn(`⚠️ Session ${sessionId} logs directory not found - console message not logged to session file`);
        }
        
      } catch (error) {
        console.warn(`⚠️ Failed to write to session browser log: ${error instanceof Error ? error.message : String(error)}`);
      }

      return this.createSuccessResult(
        'Console message forwarded successfully',
        {
          forwarded: true,
          serverTimestamp,
          logData,
          sessionLogged
        }
      );
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Console forwarding failed: ${errorMessage}`);
    }
  }
}