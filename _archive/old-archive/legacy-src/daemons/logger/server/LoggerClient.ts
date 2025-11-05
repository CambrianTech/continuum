/**
 * Logger Client - Easy access interface for process-based LoggerDaemon
 * Provides simple API while abstracting the underlying process communication
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { LoggerMessageFactory, LogLevel, LogEntry } from '../shared/LoggerMessageTypes';
import { LoggerDaemon } from './LoggerDaemon';

export class LoggerClient {
  private static instance: LoggerClient;
  private loggerDaemon: LoggerDaemon | null = null;

  private constructor() {}

  /**
   * Get singleton instance
   */
  static getInstance(): LoggerClient {
    if (!LoggerClient.instance) {
      LoggerClient.instance = new LoggerClient();
    }
    return LoggerClient.instance;
  }

  /**
   * Initialize logger daemon
   */
  async initialize(context?: ContinuumContext): Promise<void> {
    if (this.loggerDaemon) {
      return; // Already initialized
    }

    this.loggerDaemon = new LoggerDaemon(context);
    await this.loggerDaemon.start();
  }

  /**
   * Simple logging interface - fire and forget
   */
  async log(
    context: ContinuumContext,
    level: LogLevel,
    message: string,
    source: string = 'unknown',
    metadata?: Record<string, unknown>
  ): Promise<void> {
    if (!this.loggerDaemon) {
      throw new Error('Logger daemon not initialized');
    }

    const logEntry: LogEntry = {
      level,
      message,
      timestamp: Date.now(),
      sessionId: context.sessionId,
      source,
      context,
      ...(metadata && { data: metadata })
    };

    const logMessage = LoggerMessageFactory.createLogMessage(
      source,
      'logger',
      logEntry,
      this.getPriorityForLevel(level)
    );

    // Fire and forget - don't wait for processing
    await this.loggerDaemon.enqueueMessage(logMessage);
  }

  /**
   * Convenience methods for different log levels
   */
  async debug(context: ContinuumContext, message: string, source: string = 'unknown', metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'debug', message, source, metadata);
  }

  async info(context: ContinuumContext, message: string, source: string = 'unknown', metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'info', message, source, metadata);
  }

  async warn(context: ContinuumContext, message: string, source: string = 'unknown', metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'warn', message, source, metadata);
  }

  async error(context: ContinuumContext, message: string, source: string = 'unknown', metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'error', message, source, metadata);
  }

  /**
   * Force flush all buffers
   */
  async flush(sessionId?: string): Promise<void> {
    if (!this.loggerDaemon) {
      throw new Error('Logger daemon not initialized');
    }

    const flushMessage = LoggerMessageFactory.createFlushMessage(
      'client',
      'logger',
      { ...(sessionId && { sessionId }), force: true }
    );

    await this.loggerDaemon.enqueueMessage(flushMessage);
  }

  /**
   * Configure logger settings
   */
  async configure(config: {
    logLevel?: LogLevel;
    enableBatching?: boolean;
    batchSize?: number;
    flushInterval?: number;
  }): Promise<void> {
    if (!this.loggerDaemon) {
      throw new Error('Logger daemon not initialized');
    }

    const configMessage = LoggerMessageFactory.createConfigureMessage(
      'client',
      'logger',
      config
    );

    await this.loggerDaemon.enqueueMessage(configMessage);
  }

  /**
   * Get queue status for monitoring
   */
  getStatus(): {
    size: number;
    isProcessing: boolean;
    maxSize: number;
  } | null {
    if (!this.loggerDaemon) {
      return null;
    }
    return this.loggerDaemon.getQueueStatus();
  }

  /**
   * Shutdown logger daemon
   */
  async shutdown(): Promise<void> {
    if (this.loggerDaemon) {
      await this.loggerDaemon.stop();
      this.loggerDaemon = null;
    }
  }

  /**
   * Convert log level to message priority
   */
  private getPriorityForLevel(level: LogLevel): 'low' | 'normal' | 'high' | 'critical' {
    switch (level) {
      case 'debug':
        return 'low';
      case 'info':
        return 'normal';
      case 'warn':
        return 'high';
      case 'error':
        return 'critical';
      default:
        return 'normal';
    }
  }
}

// Export singleton instance for easy access
export const loggerClient = LoggerClient.getInstance();