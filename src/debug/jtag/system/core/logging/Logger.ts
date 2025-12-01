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
 * Control via environment variables:
 *   LOG_LEVEL=error  - Only errors
 *   LOG_LEVEL=warn   - Warnings and errors (default for production)
 *   LOG_LEVEL=info   - Info, warnings, errors (default for development)
 *   LOG_LEVEL=debug  - Everything (verbose, for debugging only)
 *
 *   LOG_TO_CONSOLE=0 - Disable console output (logs only to files)
 *   LOG_TO_CONSOLE=1 - Enable console output (default)
 *
 *   LOG_TO_FILES=0   - Disable file logging
 *   LOG_TO_FILES=1   - Enable file logging (default)
 *
 * Alpha launch default: LOG_LEVEL=warn, LOG_TO_CONSOLE=0 (clean console, files only)
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
import { SystemPaths } from '../config/SystemPaths';

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
  enableConsoleLogging: boolean;
}

export enum FileMode {
  CLEAN = 'w',     // Start fresh on restart (default)
  APPEND = 'a',    // Keep existing logs
  ARCHIVE = 'archive'  // Archive and rotate logs (not implemented yet)
}

type LogCategory = 'sql' | 'persona-mind' | 'genome' | 'system' | 'tools';

interface LogQueueEntry {
  message: string;
  stream: fs.WriteStream;
}

class LoggerClass {
  private static instance: LoggerClass;
  private config: LoggerConfig;
  private fileStreams: Map<string, fs.WriteStream>;  // file path -> stream
  private logQueues: Map<string, LogQueueEntry[]>;    // file path -> queue
  private logTimers: Map<string, NodeJS.Timeout>;     // file path -> flush timer
  private logDir: string;
  private readonly FLUSH_INTERVAL_MS = 100;           // Flush every 100ms
  private readonly MAX_QUEUE_SIZE = 1000;             // Max buffered messages

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
      enableFileLogging: process.env.LOG_TO_FILES !== '0',  // Enabled by default, disable with LOG_TO_FILES=0
      enableConsoleLogging: process.env.LOG_TO_CONSOLE === '1'  // Disabled by default, disable with LOG_TO_CONSOLE=0
    };

    this.fileStreams = new Map();
    this.logQueues = new Map();
    this.logTimers = new Map();
    // Use SystemPaths for correct log directory (.continuum/jtag/system/logs)
    this.logDir = SystemPaths.logs.system;
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

    const logFile = path.join(this.logDir, `${category}.log`);

    if (this.fileStreams.has(logFile)) {
      return this.fileStreams.get(logFile)!;
    }

    // Create log directory if it doesn't exist
    if (!fs.existsSync(this.logDir)) {
      fs.mkdirSync(this.logDir, { recursive: true, mode: 0o755 });
    }

    // System logs ALWAYS use 'a' (append) - never truncate during runtime
    const stream = fs.createWriteStream(logFile, { flags: 'a', mode: 0o644 });

    this.fileStreams.set(logFile, stream);
    this.logQueues.set(logFile, []);
    this.startFlushTimer(logFile);

    return stream;
  }

  /**
   * Start periodic flush timer for a log file
   */
  private startFlushTimer(logFile: string): void {
    if (this.logTimers.has(logFile)) {
      return; // Timer already running
    }

    const timer = setInterval(() => {
      this.flushQueue(logFile);
    }, this.FLUSH_INTERVAL_MS);

    this.logTimers.set(logFile, timer);
  }

  /**
   * Queue a log message for async writing
   * Fire-and-forget - never blocks the caller
   */
  private queueMessage(logFile: string, message: string): void {
    const queue = this.logQueues.get(logFile);
    const stream = this.fileStreams.get(logFile);

    if (!queue || !stream) {
      return; // Logging disabled or stream not initialized
    }

    queue.push({ message, stream });

    // Immediate flush if queue is getting full
    if (queue.length >= this.MAX_QUEUE_SIZE) {
      this.flushQueue(logFile);
    }
  }

  /**
   * Flush queued messages to file (batched write)
   */
  private flushQueue(logFile: string): void {
    const queue = this.logQueues.get(logFile);
    const stream = this.fileStreams.get(logFile);

    if (!queue || !stream || queue.length === 0) {
      return;
    }

    // Batch all messages into single write
    const batch = queue.map(entry => entry.message).join('');
    stream.write(batch);

    // Clear queue
    queue.length = 0;
  }

  /**
   * Create a logger instance for a specific component
   *
   * @param component - Component name (e.g., 'SqliteQueryExecutor')
   * @param category - Optional log category for file separation
   */
  create(component: string, category?: LogCategory): ComponentLogger {
    const fileStream = category ? this.getFileStream(category) : undefined;
    const logFile = category ? path.join(this.logDir, `${category}.log`) : undefined;
    return new ComponentLogger(component, this.config, fileStream, logFile, this);
  }

  /**
   * Create a logger with custom file path (for persona logs)
   *
   * @param component - Component name (e.g., 'PersonaMind')
   * @param logFilePath - Full path to log file (e.g., '.continuum/personas/helper-ai/logs/mind.log')
   * @param mode - File mode (CLEAN, APPEND, or ARCHIVE) - NO DEFAULT, caller must specify
   */
  createWithFile(component: string, logFilePath: string, mode: FileMode): ComponentLogger {
    // Handle ARCHIVE mode (not implemented yet)
    if (mode === FileMode.ARCHIVE) {
      console.warn('⚠️ [Logger] ARCHIVE mode not implemented yet, falling back to APPEND');
      mode = FileMode.APPEND;
    }

    // Check if stream already exists
    if (this.fileStreams.has(logFilePath)) {
      const stream = this.fileStreams.get(logFilePath)!;
      return new ComponentLogger(component, this.config, stream, logFilePath, this);
    }

    // Create custom file stream with specified mode
    const logDir = path.dirname(logFilePath);
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true, mode: 0o755 });
    }

    const stream = fs.createWriteStream(logFilePath, { flags: mode, mode: 0o644 });
    this.fileStreams.set(logFilePath, stream);
    this.logQueues.set(logFilePath, []);
    this.startFlushTimer(logFilePath);

    return new ComponentLogger(component, this.config, stream, logFilePath, this);
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
    // Flush all queues before closing streams
    for (const logFile of this.logQueues.keys()) {
      this.flushQueue(logFile);
      const timer = this.logTimers.get(logFile);
      if (timer) {
        clearInterval(timer);
      }
    }

    // Close all streams
    for (const stream of this.fileStreams.values()) {
      stream.end();
    }

    this.fileStreams.clear();
    this.logQueues.clear();
    this.logTimers.clear();
  }
}

class ComponentLogger {
  constructor(
    private component: string,
    private config: LoggerConfig,
    private fileStream?: fs.WriteStream,
    private logFilePath?: string,
    private logger?: LoggerClass
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

    // Console output (if enabled)
    if (this.config.enableConsoleLogging) {
      const prefix = `${timestamp}${emoji} ${this.component}:`;
      if (args.length === 0) {
        console.log(prefix, message);
      } else {
        console.log(prefix, message, ...args);
      }
    }

    // File output (if category specified) - FIRE-AND-FORGET via queue
    if (this.fileStream && this.logFilePath && this.logger) {
      const formattedArgs = args.length > 0
        ? ' ' + args.map(arg =>
            typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
          ).join(' ')
        : '';

      const logLine = `${timestamp}[${level}] ${this.component}: ${message}${formattedArgs}\n`;
      // Queue the message - never blocks!
      (this.logger as any).queueMessage(this.logFilePath, logLine);
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
