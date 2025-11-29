/**
 * Logger - Centralized logging utility with level control and file separation
 *
 * Usage:
 *   import { Logger } from '@system/core/logging/Logger';
 *   const log = Logger.create('SqliteQueryExecutor', 'sql');      // Logs to sql.log
 *   const mindLog = Logger.create('PersonaInbox', 'persona-mind'); // Logs to persona-mind.log
 *
 *   log.debug('Query details', { sql, params }); // Only logs if LOG_LEVEL=debug
 *   log.info('Operation completed');             // Always logs
 *   log.warn('Unusual condition');
 *   log.error('Error occurred', error);
 *
 * Control via environment variable:
 *   LOG_LEVEL=error  - Only errors
 *   LOG_LEVEL=warn   - Warnings and errors (default for production)
 *   LOG_LEVEL=info   - Info, warnings, errors (default for development)
 *   LOG_LEVEL=debug  - Everything (verbose, for debugging only)
 *
 * Alpha launch default: LOG_LEVEL=warn (minimal noise)
 *
 * Log File Categories:
 *   - 'sql': All database operations (queries, writes, schema)
 *   - 'persona-mind': PersonaUser cognitive processes (inbox, state, coordination)
 *   - 'genome': LoRA genome operations (paging, training, adapters)
 *   - 'system': System-level operations (initialization, daemon startup)
 *   - 'tools': Tool execution (parsing, execution, results, failures)
 *   - undefined: Console only (default, no file writing)
 */

import * as fs from 'fs';
import * as path from 'path';
import { DATABASE_PATHS } from '../../data/config/DatabaseConfig';

export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
  SILENT = 4
}

interface LoggerConfig {
  level: LogLevel;
  enableColors: boolean;
  enableTimestamps: boolean;
  enableFileLogging: boolean;
}

type LogCategory = 'sql' | 'persona-mind' | 'genome' | 'system' | 'tools';

class LoggerClass {
  private static instance: LoggerClass;
  private config: LoggerConfig;
  private fileStreams: Map<LogCategory, fs.WriteStream>;
  private logDir: string;

  private constructor() {
    // Default: INFO for development, WARN for production
    const envLevel = process.env.LOG_LEVEL?.toUpperCase() || 'INFO';
    const levelMap: Record<string, LogLevel> = {
      'DEBUG': LogLevel.DEBUG,
      'INFO': LogLevel.INFO,
      'WARN': LogLevel.WARN,
      'ERROR': LogLevel.ERROR,
      'SILENT': LogLevel.SILENT
    };

    this.config = {
      level: levelMap[envLevel] || LogLevel.INFO,
      enableColors: process.env.NO_COLOR !== '1',
      enableTimestamps: process.env.LOG_TIMESTAMPS === '1',
      enableFileLogging: process.env.LOG_TO_FILES === '1'
    };

    this.fileStreams = new Map();
    this.logDir = path.join(process.cwd(), DATABASE_PATHS.LOGS_DIR);
  }

  static getInstance(): LoggerClass {
    if (!LoggerClass.instance) {
      LoggerClass.instance = new LoggerClass();
    }
    return LoggerClass.instance;
  }

  /**
   * Get or create file stream for a log category
   */
  private getFileStream(category: LogCategory): fs.WriteStream {
    if (!this.config.enableFileLogging) {
      return null as any;
    }

    if (this.fileStreams.has(category)) {
      return this.fileStreams.get(category)!;
    }

    // Create log directory if it doesn't exist
    if (!fs.existsSync(this.logDir)) {
      fs.mkdirSync(this.logDir, { recursive: true, mode: 0o755 });
    }

    const logFile = path.join(this.logDir, `${category}.log`);
    const stream = fs.createWriteStream(logFile, { flags: 'a', mode: 0o644 });

    this.fileStreams.set(category, stream);
    return stream;
  }

  /**
   * Create a logger instance for a specific component
   *
   * @param component - Component name (e.g., 'SqliteQueryExecutor')
   * @param category - Optional log category for file separation
   */
  create(component: string, category?: LogCategory): ComponentLogger {
    const fileStream = category ? this.getFileStream(category) : undefined;
    return new ComponentLogger(component, this.config, fileStream);
  }

  /**
   * Get current log level
   */
  getLevel(): LogLevel {
    return this.config.level;
  }

  /**
   * Set log level programmatically
   */
  setLevel(level: LogLevel): void {
    this.config.level = level;
  }

  /**
   * Close all file streams (call on shutdown)
   */
  shutdown(): void {
    for (const stream of this.fileStreams.values()) {
      stream.end();
    }
    this.fileStreams.clear();
  }
}

class ComponentLogger {
  constructor(
    private component: string,
    private config: LoggerConfig,
    private fileStream?: fs.WriteStream
  ) {}

  private shouldLog(level: LogLevel): boolean {
    return level >= this.config.level;
  }

  private formatMessage(level: string, emoji: string, message: string, ...args: any[]): void {
    if (!this.shouldLog(LogLevel[level as keyof typeof LogLevel])) {
      return;
    }

    const timestamp = this.config.enableTimestamps
      ? `[${new Date().toISOString()}] `
      : '';

    const prefix = `${timestamp}${emoji} ${this.component}:`;

    // Console output
    if (args.length === 0) {
      console.log(prefix, message);
    } else {
      console.log(prefix, message, ...args);
    }

    // File output (if category specified)
    if (this.fileStream) {
      const formattedArgs = args.length > 0
        ? ' ' + args.map(arg =>
            typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
          ).join(' ')
        : '';

      const logLine = `${timestamp}[${level}] ${this.component}: ${message}${formattedArgs}\n`;
      this.fileStream.write(logLine);
    }
  }

  debug(message: string, ...args: any[]): void {
    this.formatMessage('DEBUG', '🔍', message, ...args);
  }

  info(message: string, ...args: any[]): void {
    this.formatMessage('INFO', 'ℹ️', message, ...args);
  }

  warn(message: string, ...args: any[]): void {
    this.formatMessage('WARN', '⚠️', message, ...args);
  }

  error(message: string, ...args: any[]): void {
    this.formatMessage('ERROR', '❌', message, ...args);
  }

  /**
   * Conditional debug logging - only executes expensive operations if debug is enabled
   *
   * Example:
   *   log.debugIf(() => ['Complex query', buildExpensiveDebugObject()]);
   */
  debugIf(messageFn: () => [string, ...any[]]): void {
    if (this.shouldLog(LogLevel.DEBUG)) {
      const [message, ...args] = messageFn();
      this.debug(message, ...args);
    }
  }
}

/**
 * Singleton logger instance
 */
export const Logger = LoggerClass.getInstance();

/**
 * Type export for component loggers
 */
export type { ComponentLogger };
