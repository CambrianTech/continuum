/**
 * Logger - Centralized logging utility with level control and file separation
 *
 * Usage:
 *   import { Logger } from '@system/core/logging/Logger';
 *   const log = Logger.create('SqliteQueryExecutor');           // Auto-routes to data/SqliteQueryExecutor.log
 *   const log = Logger.create('ArchiveDaemonServer');           // Auto-routes to daemons/ArchiveDaemonServer.log
 *   const log = Logger.create('MyComponent', 'custom/path');    // Explicit override
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
 *   LOG_TIMESTAMPS=0 - Disable timestamps (default: 1, timestamps always on)
 *
 *   LOG_TO_CONSOLE=0 - Disable console output (logs only to files)
 *   LOG_TO_CONSOLE=1 - Enable console output (default: 0)
 *
 *   LOG_TO_FILES=0   - Disable file logging
 *   LOG_TO_FILES=1   - Enable file logging (default: 1)
 *
 *   LOG_FILE_MODE=clean   - Start fresh each session (truncate existing logs)
 *   LOG_FILE_MODE=append  - Keep existing logs and add to them
 *   LOG_FILE_MODE=archive - Rotate logs (not implemented, falls back to append)
 *   (default: clean)
 *
 * Category Auto-Inference:
 *   Component names are auto-routed to subdirectories by suffix:
 *   - *DaemonServer, *Daemon → daemons/
 *   - *Adapter              → adapters/
 *   - *Worker               → workers/
 *   - *Command              → commands/
 *   - *Provider             → ai/
 *   - *Executor             → data/
 *   - *Manager, *Service    → core/
 */

import * as fs from 'fs';
import * as path from 'path';
import { SystemPaths } from '../config/SystemPaths';
import { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient';

// Import from modular files
import { LogLevel, FileMode, createLoggerConfig, parseFileMode } from './LoggerTypes';
import type { LoggerConfig, LogCategory } from './LoggerTypes';
import { inferCategory } from './CategoryInference';
import { ComponentLogger, type ParentLogger } from './ComponentLogger';

// Re-export types for consumers
export { LogLevel, FileMode } from './LoggerTypes';
export type { ComponentLogger } from './ComponentLogger';

// ============================================================================
// Rust Worker Toggle
// ============================================================================
/**
 * Enable Rust logger worker for high-performance logging.
 *
 * When true:
 * - Logs are sent to Rust worker over Unix domain socket
 * - Worker handles batching, flushing, and file I/O
 * - Falls back to TypeScript logging if worker unavailable
 *
 * When false:
 * - Standard TypeScript logging (direct file I/O)
 * - Current behavior, fully tested and reliable
 *
 * Default: true (Rust logger enabled)
 */
const USE_RUST_LOGGER = true;

interface LogQueueEntry {
  message: string;
  stream: fs.WriteStream;
}

/**
 * Main Logger class - singleton that manages file streams and routing
 * Implements ParentLogger interface for ComponentLogger consumption
 */
class LoggerClass implements ParentLogger {
  private static instance: LoggerClass;
  private config: LoggerConfig;
  private fileStreams: Map<string, fs.WriteStream>;  // file path -> stream
  private logQueues: Map<string, LogQueueEntry[]>;    // file path -> queue
  private logTimers: Map<string, NodeJS.Timeout>;     // file path -> flush timer
  private cleanedFiles: Set<string>;                   // track files cleaned with mode='w' (CLEAN)
  private defaultFileMode: FileMode;                   // default mode for log files (from LOG_FILE_MODE env var)
  private readonly FLUSH_INTERVAL_MS = 100;           // Flush every 100ms
  private readonly MAX_QUEUE_SIZE = 1000;             // Max buffered messages

  // ParentLogger interface - public for ComponentLogger access
  public workerClient: LoggerWorkerClient | null = null;
  public useRustLogger: boolean = USE_RUST_LOGGER;
  public logDir: string;

  private constructor() {
    this.config = createLoggerConfig();
    this.defaultFileMode = parseFileMode(process.env.LOG_FILE_MODE);

    this.fileStreams = new Map();
    this.logQueues = new Map();
    this.logTimers = new Map();
    this.cleanedFiles = new Set();
    this.logDir = SystemPaths.logs.system;

    // Initialize Rust worker connection (if enabled)
    if (this.useRustLogger) {
      const socketPath = '/tmp/jtag-logger-worker.sock';
      this.workerClient = new LoggerWorkerClient({
        socketPath,
        timeout: 10000,
        userId: 'logger-daemon'
      });

      // Connect in background (non-blocking)
      this.workerClient.connect()
        .then(() => {
          if (this.config.enableConsoleLogging) {
            console.log('🦀 [Logger] Connected to Rust logger worker');
          }
        })
        .catch((err) => {
          console.error('⚠️⚠️⚠️  [Logger] RUST WORKER CONNECTION FAILED - FALLING BACK TO TYPESCRIPT LOGGING ⚠️⚠️⚠️');
          console.error('⚠️  [Logger] Socket: /tmp/jtag-logger-worker.sock');
          console.error('⚠️  [Logger] Error:', err.message);
          console.error('⚠️  [Logger] To start Rust worker: npm run worker:start');
          this.workerClient = null;
        });
    }

    // Register shutdown handlers
    process.on('exit', () => this.shutdown());
    process.on('SIGINT', () => {
      this.shutdown();
      process.exit(0);
    });
    process.on('SIGTERM', () => {
      this.shutdown();
      process.exit(0);
    });
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
      throw new Error(`File logging is disabled (LOG_TO_FILES=0) but Logger.create() was called with category '${category}'. Either enable file logging or don't use categories.`);
    }

    const logFile = path.join(this.logDir, `${category}.log`);

    if (this.fileStreams.has(logFile)) {
      return this.fileStreams.get(logFile)!;
    }

    // Create log file directory (including any subdirectories in category)
    const logFileDir = path.dirname(logFile);
    if (!fs.existsSync(logFileDir)) {
      fs.mkdirSync(logFileDir, { recursive: true, mode: 0o755 });
    }

    // Use configured file mode
    let flags = 'a';
    if (this.defaultFileMode === FileMode.CLEAN && !this.cleanedFiles.has(logFile)) {
      flags = 'w';
      this.cleanedFiles.add(logFile);
    } else if (this.defaultFileMode === FileMode.APPEND) {
      flags = 'a';
    } else if (this.defaultFileMode === FileMode.ARCHIVE) {
      flags = 'a'; // ARCHIVE not implemented yet
    }

    const stream = fs.createWriteStream(logFile, { flags, mode: 0o644 });

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
      return;
    }

    const timer = setInterval(() => {
      this.flushQueue(logFile);
    }, this.FLUSH_INTERVAL_MS);

    this.logTimers.set(logFile, timer);
  }

  /**
   * Queue a log message for async writing (ParentLogger interface)
   * Fire-and-forget - never blocks the caller
   * Lazily creates file streams if needed (handles Rust worker fallback)
   */
  public queueMessage(logFile: string, message: string): void {
    let queue = this.logQueues.get(logFile);
    let stream = this.fileStreams.get(logFile);

    // Lazy initialization if stream doesn't exist (Rust worker fallback case)
    if (!queue || !stream) {
      // Extract category from logFile path
      const category = logFile
        .replace(this.logDir, '')
        .replace(/^\//, '')
        .replace(/\.log$/, '');

      this.getFileStream(category);
      queue = this.logQueues.get(logFile);
      stream = this.fileStreams.get(logFile);
    }

    if (!queue || !stream) {
      // Still no stream after lazy init - something is wrong
      console.error(`[Logger] Failed to initialize stream for '${logFile}'`);
      return;
    }

    queue.push({ message, stream });

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

    if (!queue || !stream) {
      if (this.config.enableConsoleLogging) {
        console.error(`[Logger BUG] Flush timer running but queue/stream missing for '${logFile}'`);
      }
      return;
    }

    if (queue.length === 0) {
      return;
    }

    const batch = queue.map(entry => entry.message).join('');
    stream.write(batch);
    queue.length = 0;
  }

  /**
   * Create a logger instance for a specific component
   *
   * @param component - Component name (e.g., 'ArchiveDaemonServer')
   * @param category - Optional override (if not provided, inferred from component name)
   * @param logRoot - Optional log root directory (overrides default system log dir)
   *                  Use for persona logs that go to persona home directories
   *
   * Examples:
   *   Logger.create('ArchiveDaemonServer')     → daemons/ArchiveDaemonServer.log
   *   Logger.create('SqliteStorageAdapter')   → adapters/SqliteStorageAdapter.log
   *   Logger.create('DataWorker')             → workers/DataWorker.log
   *   Logger.create('MyComponent', 'custom')  → custom.log (explicit override)
   *   Logger.create('Hippocampus', 'logs/hippocampus', '/path/to/persona/home')
   *                                           → /path/to/persona/home/logs/hippocampus.log
   */
  create(component: string, category?: LogCategory, logRoot?: string): ComponentLogger {
    // Auto-infer category from component name if not provided
    const resolvedCategory = category ?? inferCategory(component);

    // Use custom logRoot if provided (for persona logs), otherwise use system logDir
    const effectiveLogDir = logRoot || this.logDir;

    // When using Rust logger, DO NOT open TypeScript file streams (they truncate files!)
    if (resolvedCategory && !this.useRustLogger) {
      this.getFileStream(resolvedCategory);  // Sets up queue/stream for TypeScript logging
    }

    const logFile = resolvedCategory ? path.join(effectiveLogDir, `${resolvedCategory}.log`) : undefined;
    return new ComponentLogger(component, this.config, logFile, this);
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
    for (const logFile of this.logQueues.keys()) {
      this.flushQueue(logFile);
      const timer = this.logTimers.get(logFile);
      if (timer) {
        clearInterval(timer);
      }
    }

    for (const stream of this.fileStreams.values()) {
      stream.end();
    }

    this.fileStreams.clear();
    this.logQueues.clear();
    this.logTimers.clear();

    if (this.workerClient) {
      this.workerClient.disconnect();
    }
  }
}

/**
 * Singleton logger instance
 */
export const Logger = LoggerClass.getInstance();
