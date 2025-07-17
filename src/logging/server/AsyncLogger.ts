/**
 * Server Async Logger - Server-side implementation with daemon integration
 * Moved from root logging directory to follow universal module pattern
 */

import { ContinuumContext } from '../../types/shared/core/ContinuumTypes';
import { LoggerInterface, LogLevel } from '../shared/LoggingTypes';
import { loggerClient } from '../../daemons/logger/LoggerClient';

export class ServerAsyncLogger implements LoggerInterface {
  private initialized = false;

  constructor() {
    // Future versions will accept LoggingConfig
  }

  async initialize(): Promise<void> {
    if (!this.initialized) {
      await loggerClient.initialize();
      this.initialized = true;
    }
  }

  async log(
    context: ContinuumContext,
    level: LogLevel,
    message: string,
    source: string = 'unknown',
    metadata?: Record<string, unknown>
  ): Promise<void> {
    await this.initialize();
    await loggerClient.log(context, level, message, source, metadata);
  }

  async debug(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'debug', message, source, metadata);
  }

  async info(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'info', message, source, metadata);
  }

  async warn(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'warn', message, source, metadata);
  }

  async error(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void> {
    await this.log(context, 'error', message, source, metadata);
  }

  async flush(sessionId?: string): Promise<void> {
    await loggerClient.flush(sessionId);
  }

  async shutdown(): Promise<void> {
    await loggerClient.shutdown();
  }
}