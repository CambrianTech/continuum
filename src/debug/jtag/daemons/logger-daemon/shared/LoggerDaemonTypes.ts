/**
 * LoggerDaemonTypes - Logger-Specific IPC Messages
 *
 * Extends generic IPCProtocol with logger-specific message types.
 * Implements per-instance settings (file mode, flush interval, queue size, etc.)
 */

import type { IPCMessage, IPCResponse } from '../../../system/core/process/IPCProtocol';

/**
 * Log levels supported by the logger
 */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/**
 * File mode for log writing
 */
export type FileMode = 'append' | 'overwrite' | 'rotate';

/**
 * Per-instance logger settings
 * CRITICAL: Each logger instance (component+category) has its OWN settings
 *
 * Example: Logger.create('ProcessManager', 'process/manager') gets its own settings
 */
export interface LoggerSettings {
  /** Minimum log level to write (debug, info, warn, error) */
  readonly level: LogLevel;

  /** File write mode (append, overwrite, rotate) */
  readonly fileMode: FileMode;

  /** How often to flush buffer to disk (ms) */
  readonly flushIntervalMs: number;

  /** Maximum queue size before dropping logs */
  readonly maxQueueSize: number;

  /** Whether this logger instance is enabled */
  readonly enabled: boolean;

  /** Maximum file size before rotation (bytes, only if fileMode=rotate) */
  readonly maxFileSizeBytes?: number;

  /** Maximum number of rotated files to keep */
  readonly maxRotatedFiles?: number;
}

/**
 * Log message sent from Logger.ts → LoggerDaemon
 */
export interface LogMessage extends IPCMessage {
  readonly type: 'log';
  readonly data: {
    /** Log level */
    readonly level: LogLevel;

    /** Component name (e.g., 'ProcessManager') */
    readonly component: string;

    /** Category (e.g., 'process/manager') */
    readonly category: string;

    /** Primary message */
    readonly message: string;

    /** Additional arguments (objects, errors, etc.) */
    readonly args: readonly unknown[];

    /** ISO timestamp */
    readonly timestamp: string;

    /** Per-instance settings (travels with EVERY message) */
    readonly settings: LoggerSettings;
  };
}

/**
 * Response to log message
 */
export interface LogResponse extends IPCResponse {
  readonly type: 'log-response';
  readonly data?: {
    /** Whether log was written to file */
    readonly written: boolean;

    /** Number of logs currently queued */
    readonly queueSize: number;

    /** Whether queue is full (logs being dropped) */
    readonly queueFull: boolean;
  };
}

/**
 * Configure settings for a specific logger instance
 * (Allows runtime reconfiguration without restarting daemon)
 */
export interface ConfigureLoggerMessage extends IPCMessage {
  readonly type: 'configure-logger';
  readonly data: {
    /** Component name to configure */
    readonly component: string;

    /** Category to configure */
    readonly category: string;

    /** New settings for this logger instance */
    readonly settings: Partial<LoggerSettings>;
  };
}

/**
 * Response to configure-logger
 */
export interface ConfigureLoggerResponse extends IPCResponse {
  readonly type: 'configure-logger-response';
  readonly data?: {
    /** Updated settings for this logger instance */
    readonly settings: LoggerSettings;
  };
}

/**
 * Request logger statistics
 */
export interface LogStatsMessage extends IPCMessage {
  readonly type: 'log-stats';
  readonly data?: {
    /** Optional: filter by component */
    readonly component?: string;

    /** Optional: filter by category */
    readonly category?: string;
  };
}

/**
 * Logger statistics response
 */
export interface LogStatsResponse extends IPCResponse {
  readonly type: 'log-stats-response';
  readonly data: {
    /** Total logs written */
    readonly totalLogs: number;

    /** Logs by level */
    readonly byLevel: Record<LogLevel, number>;

    /** Logs currently queued */
    readonly queueSize: number;

    /** Peak queue size */
    readonly peakQueueSize: number;

    /** Number of dropped logs (queue full) */
    readonly droppedLogs: number;

    /** Per-instance statistics */
    readonly byInstance: Array<{
      readonly component: string;
      readonly category: string;
      readonly count: number;
      readonly settings: LoggerSettings;
    }>;
  };
}

/**
 * Request graceful flush of all queued logs
 */
export interface FlushMessage extends IPCMessage {
  readonly type: 'flush';
}

/**
 * Response to flush request
 */
export interface FlushResponse extends IPCResponse {
  readonly type: 'flush-response';
  readonly data?: {
    /** Number of logs flushed */
    readonly flushed: number;
  };
}

/**
 * Default settings for new logger instances
 */
export const DEFAULT_LOGGER_SETTINGS: LoggerSettings = {
  level: 'info',
  fileMode: 'append',
  flushIntervalMs: 1000,        // Flush every second
  maxQueueSize: 1000,           // Drop after 1000 queued
  enabled: true,
  maxFileSizeBytes: 10 * 1024 * 1024,  // 10MB before rotation
  maxRotatedFiles: 5            // Keep 5 rotated files
};

/**
 * Helper to create default settings with overrides
 */
export function createLoggerSettings(
  overrides?: Partial<LoggerSettings>
): LoggerSettings {
  return { ...DEFAULT_LOGGER_SETTINGS, ...overrides };
}

/**
 * Type guard for LogMessage
 */
export function isLogMessage(message: IPCMessage): message is LogMessage {
  return (
    message.type === 'log' &&
    typeof message.data === 'object' &&
    message.data !== null &&
    'level' in message.data &&
    'component' in message.data &&
    'category' in message.data &&
    'message' in message.data
  );
}

/**
 * Type guard for ConfigureLoggerMessage
 */
export function isConfigureLoggerMessage(
  message: IPCMessage
): message is ConfigureLoggerMessage {
  return (
    message.type === 'configure-logger' &&
    typeof message.data === 'object' &&
    message.data !== null &&
    'component' in message.data &&
    'category' in message.data &&
    'settings' in message.data
  );
}

/**
 * Type guard for LogStatsMessage
 */
export function isLogStatsMessage(
  message: IPCMessage
): message is LogStatsMessage {
  return message.type === 'log-stats';
}

/**
 * Type guard for FlushMessage
 */
export function isFlushMessage(message: IPCMessage): message is FlushMessage {
  return message.type === 'flush';
}
