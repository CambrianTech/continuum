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
    GROUP_END = 'groupEnd'
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
      
      return {
        level,
        message: mainMessage,
        arguments: additionalArgs,
        timestamp: new Date().toISOString(),
        metadata: {
          url: typeof window !== 'undefined' ? window.location.href : '',
          userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : '',
          stackTrace: '',
          ...metadata
        }
      };
    }
  }
}