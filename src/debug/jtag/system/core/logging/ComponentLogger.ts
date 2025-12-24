/**
 * ComponentLogger - Individual logger instance for a component
 *
 * Created by Logger.create(), handles formatting and routing of log messages.
 */

import { inspect } from 'util';
import type { LoggerConfig, LogCategory } from './LoggerTypes';
import { LogLevel } from './LoggerTypes';
import type { LogLevel as WorkerLogLevel } from '../../../shared/ipc/logger/LoggerMessageTypes';

/** Interface for the parent logger (to avoid circular imports) */
export interface ParentLogger {
  queueMessage(logFile: string, message: string): void;
  workerClient: {
    writeLog(entry: {
      category: string;
      level: WorkerLogLevel;
      component: string;
      message: string;
      args?: any[];
    }): Promise<unknown>;  // Returns WriteLogResult but we don't use it
  } | null;
  useRustLogger: boolean;
  logDir: string;
}

export class ComponentLogger {
  constructor(
    private component: string,
    private config: LoggerConfig,
    private logFilePath?: string,
    private parentLogger?: ParentLogger
  ) {}

  private shouldLog(level: LogLevel): boolean {
    return level >= this.config.level;
  }

  private formatMessage(level: string, emoji: string, message: string, ...args: any[]): void {
    const levelEnum = LogLevel[level.toUpperCase() as keyof typeof LogLevel];
    if (!this.shouldLog(levelEnum)) {
      return;
    }

    const timestamp = this.config.enableTimestamps
      ? `[${new Date().toISOString()}] `
      : '';

    // Console output (if enabled)
    if (this.config.enableConsoleLogging) {
      const prefix = `${timestamp}${emoji} ${this.component}:`;
      if (args.length === 0) {
        console.log(prefix, message);
      } else {
        console.log(prefix, message, ...args);
      }
    }

    // File output - route to Rust worker OR TypeScript queue
    if (this.parentLogger && this.logFilePath) {
      if (this.parentLogger.useRustLogger && this.parentLogger.workerClient) {
        this.sendToWorker(level as WorkerLogLevel, message, args, timestamp);
      } else {
        const formattedArgs = args.length > 0
          ? ' ' + args.map(arg =>
              typeof arg === 'object' ? inspect(arg, { depth: 2, colors: false, compact: true }) : String(arg)
            ).join(' ')
          : '';

        const logLine = `${timestamp}[${level}] ${this.component}: ${message}${formattedArgs}\n`;
        this.parentLogger.queueMessage(this.logFilePath, logLine);
      }
    }
  }

  private sendToWorker(level: WorkerLogLevel, message: string, args: any[], _timestamp: string): void {
    if (!this.parentLogger || !this.logFilePath) {
      return;
    }

    // Extract category from logFilePath
    const category = this.logFilePath
      .replace(this.parentLogger.logDir, '')
      .replace(/^\//, '')
      .replace(/\.log$/, '');

    if (this.parentLogger.workerClient) {
      this.parentLogger.workerClient.writeLog({
        category,
        level,
        component: this.component,
        message,
        args: args.length > 0 ? args : undefined
      }).catch((err) => {
        console.error(`[Logger] Rust worker write failed for ${this.component}:`, err.message);
      });
    }
  }

  debug(message: string, ...args: any[]): void {
    this.formatMessage('debug', '🔍', message, ...args);
  }

  info(message: string, ...args: any[]): void {
    this.formatMessage('info', 'ℹ️', message, ...args);
  }

  warn(message: string, ...args: any[]): void {
    this.formatMessage('warn', '⚠️', message, ...args);
  }

  error(message: string, ...args: any[]): void {
    this.formatMessage('error', '❌', message, ...args);
  }

  /**
   * Conditional debug logging - only executes expensive operations if debug is enabled
   */
  debugIf(messageFn: () => [string, ...any[]]): void {
    if (this.shouldLog(LogLevel.DEBUG)) {
      const [message, ...args] = messageFn();
      this.debug(message, ...args);
    }
  }

  /**
   * Write raw pre-formatted message to log file
   */
  writeRaw(message: string): void {
    if (this.logFilePath && this.parentLogger) {
      this.parentLogger.queueMessage(this.logFilePath, message);
    }
  }

  /**
   * Get the file path for this logger
   */
  getLogFilePath(): string | undefined {
    return this.logFilePath;
  }
}
