/**
 * Shared Console Types
 * 
 * Used by both browser and server for consistent console log forwarding
 * Enhanced with proper object serialization to prevent [object Object] issues
 */

export namespace Console {
  /**
   * Console log levels
   */
  export enum Level {
    ERROR = 'error',
    WARN = 'warn', 
    INFO = 'info',
    LOG = 'log',
    DEBUG = 'debug',
    TRACE = 'trace',
    TABLE = 'table',
    GROUP = 'group',
    GROUP_END = 'groupEnd',
    PROBE = 'probe'
  }

  /**
   * AI Probe interface for diagnostic logging
   */
  export interface ProbeData {
    /** Human-readable probe description */
    message: string;
    /** Structured data for analysis */
    data?: Record<string, unknown>;
    /** Optional JavaScript code to execute and capture result */
    executeJS?: string;
    /** Base64 encoded JavaScript for wire transmission (internal use) */
    executeJSBase64?: string;
    /** Probe category (ai-diagnostic, performance, error-trace, etc.) */
    category?: string;
    /** Tags for filtering and organization */
    tags?: string[];
  }

  /**
   * Properly typed console arguments that can be JSON serialized
   */
  export type ConsoleArgument = 
    | string 
    | number 
    | boolean 
    | null 
    | undefined
    | ObjectArgument
    | ConsoleArgument[];

  /**
   * Object arguments are JSON serialized for wire transfer
   */
  export interface ObjectArgument {
    type: 'object';
    value: Record<string, unknown>;
    stringRepresentation: string;
    originalType: string;
  }

  /**
   * Console log entry that gets sent from browser to server
   */
  export interface LogEntry {
    level: Level;
    message: string;
    arguments: ConsoleArgument[];
    timestamp: string;
    source?: string;
    sessionId?: string;
    metadata?: {
      url?: string;
      userAgent?: string;
      stackTrace?: string;
      lineNumber?: number;
      columnNumber?: number;
      fileName?: string;
      viewportWidth?: number;
      viewportHeight?: number;
      timestamp?: string;
    };
  }

  /**
   * Server response to console log forwarding
   */
  export interface LogResponse {
    success: boolean;
    timestamp: string;
    sessionLogged: boolean;
    error?: string;
  }

  /**
   * Utility class for console message processing
   */
  export class MessageUtils {
    
    /**
     * Properly serialize a console argument for wire transfer
     */
    static serializeArgument(arg: unknown): ConsoleArgument {
      if (arg === null) return null;
      if (arg === undefined) return undefined;
      
      const argType = typeof arg;
      
      if (argType === 'string' || argType === 'number' || argType === 'boolean') {
        return arg as ConsoleArgument;
      }
      
      if (Array.isArray(arg)) {
        return arg.map(item => this.serializeArgument(item));
      }
      
      if (argType === 'object') {
        try {
          // Special handling for Error objects to capture stack trace
          if (arg instanceof Error) {
            return {
              type: 'object',
              value: {
                name: arg.name,
                message: arg.message,
                stack: arg.stack,
                cause: (arg as any).cause // Error.cause is ES2022, use any for compatibility
              },
              stringRepresentation: `${arg.name}: ${arg.message}\n${arg.stack || 'No stack trace available'}`,
              originalType: '[object Error]'
            };
          }
          
          const jsonString = JSON.stringify(arg, null, 2);
          return {
            type: 'object',
            value: arg as Record<string, unknown>,
            stringRepresentation: jsonString,
            originalType: Object.prototype.toString.call(arg)
          };
        } catch (error) {
          // Fallback for non-serializable objects (circular references, etc.)
          return {
            type: 'object',
            value: {},
            stringRepresentation: `[${Object.prototype.toString.call(arg)}]`,
            originalType: Object.prototype.toString.call(arg)
          };
        }
      }
      
      // Fallback for functions, symbols, etc.
      return String(arg);
    }
    
    /**
     * Convert console arguments to a readable string with proper object formatting
     */
    static argumentsToString(args: ConsoleArgument[]): string {
      return args.map(arg => {
        if (arg === null) return 'null';
        if (arg === undefined) return 'undefined';
        
        if (typeof arg === 'object' && arg !== null) {
          if (Array.isArray(arg)) {
            return `[${arg.map(item => this.argumentsToString([item])).join(', ')}]`;
          }
          
          if ('type' in arg && arg.type === 'object') {
            return arg.stringRepresentation;
          }
        }
        
        return String(arg);
      }).join(' ');
    }
    
    /**
     * Create a properly formatted console log entry
     */
    static createLogEntry(
      level: Level,
      args: unknown[],
      metadata?: Partial<LogEntry['metadata']>
    ): LogEntry {
      const serializedArgs = args.map(arg => this.serializeArgument(arg));
      
      // First argument is typically the main message
      const mainMessage = serializedArgs.length > 0 ? this.argumentsToString([serializedArgs[0]]) : '';
      const additionalArgs = serializedArgs.slice(1);
      
      // Use provided stack trace if available, otherwise capture it
      const stackTrace = metadata?.stackTrace || this.captureStackTrace();
      const browserContext = this.captureBrowserContext();
      
      return {
        level,
        message: mainMessage,
        arguments: additionalArgs,
        timestamp: new Date().toISOString(),
        metadata: {
          stackTrace,
          ...browserContext,
          ...metadata
        }
      };
    }
    
    /**
     * Capture actual stack trace from current execution context
     */
    static captureStackTrace(): string {
      try {
        const error = new Error();
        if (error.stack) {
          // Remove our own utility functions from the stack trace
          const lines = error.stack.split('\n');
          const filteredLines = lines.filter(line => 
            !line.includes('MessageUtils.captureStackTrace') &&
            !line.includes('MessageUtils.createLogEntry') &&
            !line.includes('ConsoleForwarder.forwardConsole') &&
            !line.includes('ClientConsoleManager.forwardConsole') &&
            !line.includes('ClientConsoleManager.') &&
            !line.includes('ClientLoggerDaemon.') &&
            !line.includes('console.log') &&
            !line.includes('console.warn') &&
            !line.includes('console.error') &&
            !line.includes('console.info') &&
            !line.includes('console.debug') &&
            !line.includes('console.trace')
          );
          return filteredLines.join('\n');
        }
      } catch (e) {
        // Fallback if stack capture fails
      }
      return '';
    }
    
    /**
     * Capture comprehensive browser context
     */
    static captureBrowserContext(): Partial<LogEntry['metadata']> {
      if (typeof window === 'undefined') {
        return {};
      }
      
      const context: Partial<LogEntry['metadata']> = {
        url: window.location.href,
        userAgent: navigator.userAgent,
        fileName: window.location.pathname,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
        timestamp: new Date().toISOString()
      };
      
      return context;
    }
  }
}