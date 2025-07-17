/**
 * Console Command - Bridge browser console logs back to development portal
 * 
 * CRITICAL FOR JTAG: This enables autonomous development by forwarding
 * browser console logs, errors, and health reports back to the AI portal
 */

import { DirectCommand } from '../direct-command/DirectCommand';
import { CommandResult, CommandDefinition } from '../base-command/BaseCommand';
import { Console } from '../../../types/shared/ConsoleTypes';
import { UniversalLogger } from '../../../logging/UniversalLogger';

export class ConsoleCommand extends DirectCommand {
  /**
   * Get original console methods to avoid infinite loops with UniversalLogger override
   */
  private static getOriginalConsole() {
    return (UniversalLogger as any).originalConsole || console;
  }

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

  protected static async executeOperation(params: any = {}): Promise<CommandResult> {
    try {
      // Convert whatever the client sends into our well-defined shared type
      // This ensures server-side type safety regardless of client format
      const logData: Console.LogEntry = this.normalizeToSharedType(params);
      
      if (!logData.level || !logData.message) {
        this.getOriginalConsole().log(`🎯 MISSING PARAMS: level=${logData.level}, message=${logData.message}`);
        return this.createErrorResult('Console forwarding requires level and message parameters');
      }

      // Add server-side timestamp and create structured log entry with spread operator
      const serverTimestamp = new Date().toISOString();
      
      // Decode base64 JavaScript if present (for probe level)
      if (logData.level === Console.Level.PROBE && logData.arguments?.length > 0) {
        const probeArg = logData.arguments[0];
        if (probeArg && typeof probeArg === 'object' && 'executeJSBase64' in probeArg) {
          try {
            const executeJSBase64 = (probeArg as any).executeJSBase64;
            const decodedJS = Buffer.from(executeJSBase64, 'base64').toString('utf-8');
            // Add decoded JS to server logs for debugging (but keep base64 in stored logs)
            this.getOriginalConsole().log(`🔬 PROBE JS Code: ${decodedJS}`);
          } catch (error) {
            this.getOriginalConsole().warn(`⚠️ Failed to decode probe JavaScript: ${error}`);
          }
        }
      }
      
      
      const timePrefix = `[${new Date().toLocaleTimeString()}]`;

      // Log to server console (always) with timestamp and full metadata
      this.getOriginalConsole().log(`[${timePrefix} console.${logData.level}]: ${logData.message}`);
      if (logData.arguments && logData.arguments.length > 0) {
        this.getOriginalConsole().log(`   Args:`, logData.arguments);
      }
      
      // Log enhanced metadata including stack trace if available
      if (logData.metadata?.stackTrace) {
        this.getOriginalConsole().log(`   Stack Trace:`, logData.metadata.stackTrace);
      }
      if (logData.metadata?.url) {
        this.getOriginalConsole().log(`   URL:`, logData.metadata.url);
      }

      // No longer write to session-specific files - UniversalLogger handles this
      const sessionLogged = true; // UniversalLogger will handle session-specific logging

      // ADDITIONAL: Use UniversalLogger to create browser.log.json (comprehensive JSON file)
      // This ADDS to existing browser logging, doesn't replace it
      try {
        // Format message for UniversalLogger
        let formattedMessage = logData.message;
        
        // Add arguments if present
        if (logData.arguments && logData.arguments.length > 0) {
          const formattedArgs = Console.MessageUtils.argumentsToString(logData.arguments);
          formattedMessage += ` ${formattedArgs}`;
        }
        
        // Add stack trace for errors
        if (logData.level === Console.Level.ERROR && logData.metadata?.stackTrace) {
          formattedMessage += `\n   Stack: ${logData.metadata.stackTrace}`;
        }
        
        // Map Console.Level to UniversalLogger level
        const universalLogLevel = logData.level === Console.Level.LOG ? 'info' : 
                                 logData.level === Console.Level.WARN ? 'warn' :
                                 logData.level === Console.Level.ERROR ? 'error' :
                                 logData.level === Console.Level.DEBUG ? 'debug' :
                                 'info';
        
        // Call UniversalLogger to create browser.log.json (using working pattern from SessionManagerDaemon)
        UniversalLogger.log(
          'browser',
          logData.source || 'ConsoleCommand',
          formattedMessage,
          universalLogLevel as 'info' | 'warn' | 'error' | 'debug'
        );
        
        
        this.getOriginalConsole().log(`✅ Also logged to UniversalLogger: browser.${universalLogLevel}`);
        
      } catch (universalError) {
        this.getOriginalConsole().warn(`⚠️ Failed to log to UniversalLogger: ${universalError instanceof Error ? universalError.message : String(universalError)}`);
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