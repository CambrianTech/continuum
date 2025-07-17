/**
 * Async Logger - Convenient interface for process-based logging
 * Drop-in replacement for UniversalLogger with async queue processing
 */

import { ContinuumContext } from '../types/shared/core/ContinuumTypes';
import { loggerClient } from '../daemons/logger/LoggerClient';
import { LogLevel } from '../daemons/logger/LoggerMessageTypes';

export class AsyncLogger {
  /**
   * Main logging method - fire and forget
   */
  static async log(
    context: ContinuumContext,
    level: LogLevel,
    message: string,
    source: string = 'unknown'
  ): Promise<void> {
    try {
      await loggerClient.log(context, level, message, source);
    } catch (error) {
      // Fallback to console if logger fails
      console.error('AsyncLogger failed:', error);
      console.log(`[${level.toUpperCase()}] ${message}`);
    }
  }

  /**
   * Convenience methods
   */
  static async debug(context: ContinuumContext, message: string, source: string = 'unknown'): Promise<void> {
    await AsyncLogger.log(context, 'debug', message, source);
  }

  static async info(context: ContinuumContext, message: string, source: string = 'unknown'): Promise<void> {
    await AsyncLogger.log(context, 'info', message, source);
  }

  static async warn(context: ContinuumContext, message: string, source: string = 'unknown'): Promise<void> {
    await AsyncLogger.log(context, 'warn', message, source);
  }

  static async error(context: ContinuumContext, message: string, source: string = 'unknown'): Promise<void> {
    await AsyncLogger.log(context, 'error', message, source);
  }

  /**
   * Initialize the async logger
   */
  static async initialize(context?: ContinuumContext): Promise<void> {
    await loggerClient.initialize(context);
  }

  /**
   * Flush all pending logs
   */
  static async flush(sessionId?: string): Promise<void> {
    await loggerClient.flush(sessionId);
  }

  /**
   * Shutdown the logger
   */
  static async shutdown(): Promise<void> {
    await loggerClient.shutdown();
  }
}

// Super convenient global functions
export const asyncLog = AsyncLogger.log;
export const asyncDebug = AsyncLogger.debug;
export const asyncInfo = AsyncLogger.info;
export const asyncWarn = AsyncLogger.warn;
export const asyncError = AsyncLogger.error;