/**
 * Client Logger Daemon - Simple browser-side console forwarder
 * Handles console interception and forwarding to server via WebSocket
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { 
  LoggerMessage
} from '../shared/LoggerMessageTypes';

/**
 * WebSocket-based transport for client-server communication
 */
interface WebSocketTransport {
  send(message: LoggerMessage): Promise<void>;
  isConnected(): boolean;
}

export class ClientLoggerDaemon {
  public readonly context: ContinuumContext;
  private webSocketTransport: WebSocketTransport | null = null;

  constructor(context: ContinuumContext) {
    this.context = context;
    // Console forwarding is now handled by ClientConsoleManager
    // This daemon only processes log messages received from the manager
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


  // Serialization methods removed - now using unified ConsoleUtils

  /**
   * Log to browser console (fallback/local logging)
   * DISABLED: Let server-side forwarding handle all console output with proper stack traces
   */
  private logToBrowserConsole(_loggerMessage: LoggerMessage): void {
    // Disabled to prevent duplicate output and allow server-side stack traces to be the primary output
    // The ClientConsoleManager already handles both console.* calls and forwarding
    return;
  }

  /**
   * Clean up (console methods are now managed by ClientConsoleManager)
   */
  cleanup(): void {
    // Console cleanup is now handled by ClientConsoleManager
    // This daemon only needs to cleanup WebSocket transport
    this.webSocketTransport = null;
  }
}