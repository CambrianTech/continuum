/**
 * Console Command - Bridge browser console logs back to development portal
 * 
 * CRITICAL FOR JTAG: This enables autonomous development by forwarding
 * browser console logs, errors, and health reports back to the AI portal
 */

import { DirectCommand } from '../direct-command/DirectCommand';
import { CommandResult, WebSocketCommandContext, CommandDefinition } from '../base-command/BaseCommand';
import { Console } from '../../../types/shared/ConsoleTypes';

export class ConsoleCommand extends DirectCommand {
  /**
   * Normalize any client input format into our well-defined shared type
   * Server-side only - ensures type safety regardless of what client sends
   */
  private static normalizeToSharedType(params: any): Console.LogEntry {
    // If already in Console.LogEntry format, use as-is
    if (params.level && params.message && params.arguments) {
      return params as Console.LogEntry;
    }
    
    // Handle legacy BrowserConsoleLogForwarding format
    if (params.consoleLogLevel && params.consoleMessage) {
      return {
        level: params.consoleLogLevel as Console.Level,
        message: params.consoleMessage,
        arguments: params.consoleArguments || [],
        timestamp: params.browserContext?.timestamp || new Date().toISOString(),
        sessionId: params.sessionId,
        metadata: {
          ...params.browserContext,
          url: params.browserContext?.currentUrl,
          stackTrace: params.browserContext?.stackTrace || '',
          fileName: params.browserContext?.sourceFileName,
          lineNumber: params.browserContext?.sourceLineNumber,
          columnNumber: params.browserContext?.sourceColumnNumber,
          viewportWidth: params.browserContext?.viewportWidth || 0,
          viewportHeight: params.browserContext?.viewportHeight || 0
        }
      };
    }
    
    // Handle very legacy format - convert to shared type using spread operators
    const { action, message, source, data, level, args, arguments: consoleArguments, timestamp, ...rest } = params;
    
    return {
      level: (action || level || 'log') as Console.Level,
      message: message || '',
      arguments: Array.isArray(args || consoleArguments) ? (args || consoleArguments) : [],
      timestamp: timestamp || data?.timestamp || new Date().toISOString(),
      source,
      metadata: {
        stackTrace: data?.stackTrace || rest.stackTrace || '',
        url: data?.url || rest.url || '',
        userAgent: data?.userAgent || rest.userAgent || '',
        fileName: data?.fileName || rest.fileName,
        lineNumber: data?.lineNumber || rest.lineNumber,
        columnNumber: data?.columnNumber || rest.columnNumber,
        viewportWidth: data?.viewport?.width || rest.viewportWidth || 0,
        viewportHeight: data?.viewport?.height || rest.viewportHeight || 0,
        ...rest
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
      const logData: Console.LogEntry = this.normalizeToSharedType(params);
      
      if (!logData.level || !logData.message) {
        console.log(`🎯 MISSING PARAMS: level=${logData.level}, message=${logData.message}`);
        return this.createErrorResult('Console forwarding requires level and message parameters');
      }

      // Add server-side timestamp and create structured log entry with spread operator
      const serverTimestamp = new Date().toISOString();
      
      // Decode base64 JavaScript if present (for probe level)
      const enhancedLogData = { ...logData };
      if (logData.level === Console.Level.PROBE && logData.arguments?.length > 0) {
        const probeArg = logData.arguments[0];
        if (probeArg && typeof probeArg === 'object' && 'executeJSBase64' in probeArg) {
          try {
            const executeJSBase64 = (probeArg as any).executeJSBase64;
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
        sessionId: context.sessionId || logData.sessionId
      };
      
      const timePrefix = `[${new Date().toLocaleTimeString()}]`;

      // Log to server console (always) with timestamp and full metadata
      console.log(`[${timePrefix} console.${logData.level}]: ${logData.message}`);
      if (logData.arguments && logData.arguments.length > 0) {
        console.log(`   Args:`, logData.arguments);
      }
      
      // Log enhanced metadata including stack trace if available
      if (logData.metadata?.stackTrace) {
        console.log(`   Stack Trace:`, logData.metadata.stackTrace);
      }
      if (logData.metadata?.url) {
        console.log(`   URL:`, logData.metadata.url);
      }

      // Write to session-specific log files with level-based JSON format
      let sessionLogged = false;
      try {
        // sessionId is now guaranteed to be non-null by TypeScript
        let sessionId = context.sessionId;
        
        // If no sessionId provided, use the current/default shared development session
        if (!sessionId) {
          // Try to find the most recent development session using async operations
          try {
            const fs = await import('fs/promises');
            const { join } = await import('path');
            
            const sessionsPath = '.continuum/sessions/user/shared';
            
            // Use async operations to avoid blocking
            const allEntries = await fs.readdir(sessionsPath);
            const sessionDirs: string[] = [];
            
            // Check each entry asynchronously
            for (const entry of allEntries) {
              try {
                const fullPath = join(sessionsPath, entry);
                const stat = await fs.stat(fullPath);
                if (stat.isDirectory() && entry.startsWith('development-shared-')) {
                  sessionDirs.push(entry);
                }
              } catch (statError) {
                // Skip entries we can't access
                continue;
              }
            }
            
            if (sessionDirs.length > 0) {
              // Sort by modification time to get the most recent
              const sessionStats = await Promise.all(
                sessionDirs.map(async (dir) => {
                  try {
                    const fullPath = join(sessionsPath, dir);
                    const stat = await fs.stat(fullPath);
                    return { dir, mtime: stat.mtimeMs };
                  } catch (error) {
                    return { dir, mtime: 0 };
                  }
                })
              );
              
              const sortedSessions = sessionStats
                .sort((a, b) => b.mtime - a.mtime)
                .map(s => s.dir);
              
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
            const levelLogPath = join(sessionLogsDir, `browser.${logData.level}.json`);
            const allLogsPath = join(sessionLogsDir, 'browser.log');
            
            // Handle probe logs specially for better readability
            let jsonLogEntry: string;
            let humanReadableEntry: string;
            
            if (logData.level === Console.Level.PROBE) {
              // For probe logs, format for human readability with decoded JS
              // Probe data is in message as JSON string or first argument
              let probeData: any = null;
              try {
                probeData = JSON.parse(logData.message);
              } catch (error) {
                // Fallback: check arguments for probe data object
                probeData = logData.arguments?.[0];
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
                humanReadableEntry = `[${serverTimestamp}] 🛸 PROBE: ${logData.message}\n`;
              }
            } else {
              // Standard log format for non-probe logs with enhanced metadata
              jsonLogEntry = JSON.stringify(logEntry) + '\n';
              
              humanReadableEntry = `[${serverTimestamp}] ${logData.level.toUpperCase()}: ${logData.message}`;
              
              // Add arguments if present using Console.MessageUtils for proper formatting
              if (logData.arguments && logData.arguments.length > 0) {
                const formattedArgs = Console.MessageUtils.argumentsToString(logData.arguments);
                humanReadableEntry += ` ${formattedArgs}`;
              }
              
              // Add stack trace for errors to human-readable log
              if (logData.level === Console.Level.ERROR && logData.metadata?.stackTrace) {
                humanReadableEntry += `\n   Stack: ${logData.metadata.stackTrace}`;
              }
              
              humanReadableEntry += '\n';
            }
            
            // Write to both formats: level-specific JSON and human-readable for browser.log
            await Promise.all([
              fs.appendFile(levelLogPath, jsonLogEntry),        // browser.probe.json (readable JSON) or browser.warn.json (compact JSON)
              fs.appendFile(allLogsPath, humanReadableEntry)    // browser.log (human-readable)
            ]);
            
            console.log(`✅ Wrote to browser logs: ${sessionId} (${logData.level})`);
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