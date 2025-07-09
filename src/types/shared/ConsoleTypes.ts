/**
 * Shared Console Types
 * 
 * Used by both browser and server for consistent console log forwarding
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
   * Console log entry that gets sent from browser to server
   */
  export interface LogEntry {
    level: Level;
    message: string;
    timestamp: string;
    source?: string;
    data?: any;
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
}