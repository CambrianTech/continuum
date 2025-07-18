/**
 * Client Logger Daemon - Simple browser-side console forwarder
 * Handles console interception and forwarding to server via WebSocket
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { 
  LoggerMessage, 
  LogEntry, 
  LogLevel,
  LoggerMessageFactory
} from '../shared/LoggerMessageTypes';

/**
 * Browser-specific console override interface
 */
interface OriginalConsole {
  log: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  info: (...args: unknown[]) => void;
  debug: (...args: unknown[]) => void;
  trace: (...args: unknown[]) => void;
}

/**
 * WebSocket-based transport for client-server communication
 */
interface WebSocketTransport {
  send(message: LoggerMessage): Promise<void>;
  isConnected(): boolean;
}

export class ClientLoggerDaemon {
  public readonly context: ContinuumContext;
  private consoleForwarding = false;
  private originalConsole: OriginalConsole = {} as OriginalConsole;
  private webSocketTransport: WebSocketTransport | null = null;

  constructor(context: ContinuumContext) {
    this.context = context;
    this.enableConsoleForwarding();
  }

  /**
   * Set WebSocket transport for server communication
   */
  setWebSocketTransport(transport: WebSocketTransport): void {
    this.webSocketTransport = transport;
  }

  /**
   * Process a log message (used by ClientLoggerClient)
   */
  async processLogMessage(loggerMessage: LoggerMessage): Promise<void> {
    // Forward to server via WebSocket if available
    if (this.webSocketTransport && this.webSocketTransport.isConnected()) {
      await this.webSocketTransport.send(loggerMessage);
    }

    // Always log locally in browser as well
    this.logToBrowserConsole(loggerMessage);
  }

  /**
   * Enable console forwarding
   */
  private enableConsoleForwarding(): void {
    if (this.consoleForwarding) {
      console.warn('⚠️ Console forwarding is already enabled');
      return;
    }

    // Store original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      debug: console.debug ? console.debug.bind(console) : console.log.bind(console),
      trace: console.trace.bind(console)
    };

    // Override console methods
    console.log = (...args: unknown[]): void => {
      this.originalConsole.log(...args);
      this.forwardConsole('log', args);
    };

    console.warn = (...args: unknown[]): void => {
      this.originalConsole.warn(...args);
      this.forwardConsole('warn', args);
    };

    console.error = (...args: unknown[]): void => {
      this.originalConsole.error(...args);
      this.forwardConsole('error', args);
    };

    console.info = (...args: unknown[]): void => {
      this.originalConsole.info(...args);
      this.forwardConsole('info', args);
    };

    console.debug = (...args: unknown[]): void => {
      this.originalConsole.debug(...args);
      this.forwardConsole('debug', args);
    };

    console.trace = (...args: unknown[]): void => {
      this.originalConsole.trace(...args);
      this.forwardConsole('trace', args);
    };

    this.consoleForwarding = true;
  }

  /**
   * Forward console call to processing
   */
  private forwardConsole(level: LogLevel, args: unknown[]): void {
    try {
      const entry: LogEntry = {
        level,
        message: args.map(arg => this.serializeArg(arg)).join(' '),
        timestamp: Date.now(),
        sessionId: this.context.sessionId,
        source: 'browser-console',
        context: this.context,
        data: args.length > 0 ? { args } : undefined
      };

      const logMessage = LoggerMessageFactory.createLogMessage(entry);
      
      // Process the message (fire and forget)
      this.processLogMessage(logMessage).catch(error => {
        this.originalConsole.error('Failed to forward console message:', error);
      });
    } catch (error) {
      this.originalConsole.error('Failed to forward console message:', error);
    }
  }

  /**
   * Serialize arguments for logging
   */
  private serializeArg(arg: unknown): string {
    if (arg === null) return 'null';
    if (arg === undefined) return 'undefined';
    if (typeof arg === 'string') return arg;
    if (typeof arg === 'number' || typeof arg === 'boolean') return String(arg);
    if (typeof arg === 'function') return `[Function: ${arg.name || 'anonymous'}]`;
    if (arg instanceof Error) return `${arg.name}: ${arg.message}`;
    
    try {
      return JSON.stringify(arg, null, 2);
    } catch {
      return String(arg);
    }
  }

  /**
   * Log to browser console (fallback/local logging)
   */
  private logToBrowserConsole(loggerMessage: LoggerMessage): void {
    if (loggerMessage.type !== 'log') return;

    const entry = loggerMessage.payload as LogEntry;
    const method = this.originalConsole[entry.level as keyof OriginalConsole] || this.originalConsole.log;
    
    const prefix = `[${entry.source}]`;
    method(prefix, entry.message, entry.data || '');
  }

  /**
   * Clean up and restore original console methods
   */
  cleanup(): void {
    if (this.consoleForwarding) {
      console.log = this.originalConsole.log;
      console.warn = this.originalConsole.warn;
      console.error = this.originalConsole.error;
      console.info = this.originalConsole.info;
      console.debug = this.originalConsole.debug;
      console.trace = this.originalConsole.trace;
      this.consoleForwarding = false;
    }
  }
}