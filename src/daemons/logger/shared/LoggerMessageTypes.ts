/**
 * Logger Message Types - Typed messages for async logging daemon
 * Uses DaemonMessage<T> generic pattern for type safety
 */

import { DaemonMessage } from '../../base/DaemonProtocol';
import { LogLevel, LogEntry } from './ConsoleTypes';

// Browser-compatible UUID generation
function generateUUID(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID();
  } else {
    // Fallback for browser environments
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  }
}

// Re-export unified types for convenience
export { ConsoleUtils } from './ConsoleTypes';
export type { LogLevel, LogEntry, ProbeData, OriginalConsole } from './ConsoleTypes';

export interface LoggerMessage {
  type: 'log' | 'flush' | 'rotate' | 'configure';
  payload: LogEntry | FlushRequest | RotateRequest | ConfigureRequest;
}

export interface FlushRequest {
  sessionId?: string; // Flush specific session or all sessions
  force?: boolean;    // Force immediate flush
}

export interface RotateRequest {
  sessionId?: string; // Rotate specific session logs or all
  maxSize?: number;   // Max file size before rotation
}

export interface ConfigureRequest {
  logLevel?: LogLevel;
  enableBatching?: boolean;
  batchSize?: number;
  flushInterval?: number;
}

// Typed daemon messages for logger
export type LoggerDaemonMessage = DaemonMessage<LoggerMessage>;

// Factory functions for creating typed messages
export class LoggerMessageFactory {
  static createLogMessage(logEntry: LogEntry): LoggerMessage;
  static createLogMessage(
    from: string,
    to: string,
    logEntry: LogEntry,
    priority?: 'low' | 'normal' | 'high' | 'critical'
  ): LoggerDaemonMessage;
  static createLogMessage(
    logEntryOrFrom: LogEntry | string,
    to?: string,
    logEntry?: LogEntry,
    priority: 'low' | 'normal' | 'high' | 'critical' = 'normal'
  ): LoggerMessage | LoggerDaemonMessage {
    // Simple overload for direct LoggerMessage
    if (typeof logEntryOrFrom === 'object' && !to) {
      return {
        type: 'log',
        payload: logEntryOrFrom
      };
    }
    
    // Full DaemonMessage overload
    return {
      id: generateUUID(),
      from: logEntryOrFrom as string,
      to: to!,
      type: 'log',
      data: {
        type: 'log',
        payload: logEntry!
      },
      timestamp: new Date(),
      priority
    };
  }

  static createFlushMessage(
    from: string,
    to: string,
    flushRequest: FlushRequest = {}
  ): LoggerDaemonMessage {
    return {
      id: generateUUID(),
      from,
      to,
      type: 'flush',
      data: {
        type: 'flush',
        payload: flushRequest
      },
      timestamp: new Date(),
      priority: 'high'
    };
  }

  static createRotateMessage(
    from: string,
    to: string,
    rotateRequest: RotateRequest = {}
  ): LoggerDaemonMessage {
    return {
      id: generateUUID(),
      from,
      to,
      type: 'rotate',
      data: {
        type: 'rotate',
        payload: rotateRequest
      },
      timestamp: new Date(),
      priority: 'normal'
    };
  }

  static createConfigureMessage(
    from: string,
    to: string,
    configureRequest: ConfigureRequest
  ): LoggerDaemonMessage {
    return {
      id: generateUUID(),
      from,
      to,
      type: 'configure',
      data: {
        type: 'configure',
        payload: configureRequest
      },
      timestamp: new Date(),
      priority: 'high'
    };
  }
}