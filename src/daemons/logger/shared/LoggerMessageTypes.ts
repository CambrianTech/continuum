/**
 * Logger Message Types - Typed messages for async logging daemon
 * Uses DaemonMessage<T> generic pattern for type safety
 */

import { DaemonMessage } from '../../base/DaemonProtocol';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';

export type LogLevel = 'log' | 'debug' | 'info' | 'warn' | 'error' | 'trace';

export interface LogEntry {
  level: LogLevel;
  message: string;
  timestamp: number;
  sessionId: string;
  source: string;
  context: ContinuumContext;
  data?: Record<string, unknown> | undefined;
}

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
      id: crypto.randomUUID(),
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
      id: crypto.randomUUID(),
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
      id: crypto.randomUUID(),
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
      id: crypto.randomUUID(),
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