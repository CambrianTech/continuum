/**
 * Client Logger Client - Simple interface for browser-side logging
 * Follows the same pattern as server LoggerClient but for browser context
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { LoggerMessageFactory, LogLevel, LogEntry } from '../shared/LoggerMessageTypes';
import { ClientLoggerDaemon } from './ClientLoggerDaemon';

export class ClientLoggerClient {
  private static instance: ClientLoggerClient;
  private clientLoggerDaemon: ClientLoggerDaemon | null = null;

  private constructor() {}

  static getInstance(): ClientLoggerClient {
    if (!ClientLoggerClient.instance) {
      ClientLoggerClient.instance = new ClientLoggerClient();
    }
    return ClientLoggerClient.instance;
  }

  /**
   * Initialize daemon with context
   */
  initialize(context: ContinuumContext): void {
    if (this.clientLoggerDaemon) {
      return; // Already initialized
    }
    
    this.clientLoggerDaemon = new ClientLoggerDaemon(context);
  }

  /**
   * Set WebSocket transport for server communication
   */
  setWebSocketTransport(transport: any): void {
    if (!this.clientLoggerDaemon) {
      throw new Error('ClientLoggerClient not initialized');
    }
    this.clientLoggerDaemon.setWebSocketTransport(transport);
  }

  /**
   * Log a message directly (bypasses console overrides)
   */
  async log(level: LogLevel, message: string, data?: Record<string, unknown>): Promise<void> {
    if (!this.clientLoggerDaemon) {
      console.warn('ClientLoggerClient not initialized, using console fallback');
      (console as any)[level](message, data);
      return;
    }

    const entry: LogEntry = {
      level,
      message,
      timestamp: Date.now(),
      sessionId: this.clientLoggerDaemon.context.sessionId,
      source: 'client-direct',
      context: this.clientLoggerDaemon.context,
      data
    };

    const logMessage = LoggerMessageFactory.createLogMessage(entry);
    
    // Simple direct processing - no complex daemon message wrapping
    try {
      await this.clientLoggerDaemon.processLogMessage(logMessage);
    } catch (error) {
      console.error('Failed to process log message:', error);
    }
  }

  /**
   * Convenience methods for different log levels
   */
  async info(message: string, data?: Record<string, unknown>): Promise<void> {
    return this.log('info', message, data);
  }

  async warn(message: string, data?: Record<string, unknown>): Promise<void> {
    return this.log('warn', message, data);
  }

  async error(message: string, data?: Record<string, unknown>): Promise<void> {
    return this.log('error', message, data);
  }

  async debug(message: string, data?: Record<string, unknown>): Promise<void> {
    return this.log('debug', message, data);
  }

  /**
   * Check if daemon is initialized
   */
  isInitialized(): boolean {
    return this.clientLoggerDaemon !== null;
  }

  /**
   * Clean up
   */
  destroy(): void {
    if (this.clientLoggerDaemon) {
      this.clientLoggerDaemon.cleanup();
      this.clientLoggerDaemon = null;
    }
  }
}

// Export singleton instance
export const clientLoggerClient = ClientLoggerClient.getInstance();