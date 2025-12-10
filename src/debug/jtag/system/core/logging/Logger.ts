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
 * Recommended defaults:
 *   Development: LOG_LEVEL=info, LOG_TO_CONSOLE=0, LOG_FILE_MODE=clean
 *   Production: LOG_LEVEL=warn, LOG_TO_CONSOLE=0, LOG_FILE_MODE=clean
 *
 * Log File Categories:
 *   - 'sql': All database operations (queries, writes, schema)
 *   - 'persona-mind': PersonaUser cognitive processes (inbox, state, coordination)
 *   - 'genome': LoRA genome operations (paging, training, adapters)
 *   - 'system': System-level operations (initialization, daemon startup)
 *   - 'tools': Tool execution (parsing, execution, results, failures)
 *   - undefined: Console only (default, no file writing)
 */

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
 * Default: true (Rust logger enabled for testing)
 */
const USE_RUST_LOGGER = true;

import * as fs from 'fs';
import * as path from 'path';
import { inspect } from 'util';
import { spawn, ChildProcess } from 'child_process';
import { SystemPaths } from '../config/SystemPaths';
import { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient.js';
import type { LogLevel as WorkerLogLevel } from '../../../shared/ipc/logger/LoggerMessageTypes.js';

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

type LogCategory = string;

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
  private cleanedFiles: Set<string>;                   // track files cleaned with mode='w' (CLEAN)
  private defaultFileMode: FileMode;                   // default mode for log files (from LOG_FILE_MODE env var)
  private logDir: string;
  private readonly FLUSH_INTERVAL_MS = 100;           // Flush every 100ms
  private readonly MAX_QUEUE_SIZE = 1000;             // Max buffered messages
  private workerProcess: ChildProcess | null = null;  // Rust worker process (when USE_RUST_LOGGER enabled)
  public workerClient: LoggerWorkerClient | null = null;  // Rust worker client (when USE_RUST_LOGGER enabled) - public so ComponentLogger can access
  public useRustLogger: boolean = USE_RUST_LOGGER;   // Toggle for Rust logger - public so ComponentLogger can access

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

    // Read file mode from environment (default: CLEAN)
    const envFileMode = process.env.LOG_FILE_MODE?.toLowerCase() || 'clean';
    const fileModeMap: Record<string, FileMode> = {
      'clean': FileMode.CLEAN,
      'append': FileMode.APPEND,
      'archive': FileMode.ARCHIVE
    };
    this.defaultFileMode = fileModeMap[envFileMode] || FileMode.CLEAN;

    this.config = {
      level: levelMap[envLevel] || LogLevel.INFO,
      enableColors: process.env.NO_COLOR !== '1',
      enableTimestamps: process.env.LOG_TIMESTAMPS !== '0',  // ALWAYS ON by default (timestamps essential for debugging)
      enableFileLogging: process.env.LOG_TO_FILES !== '0',  // Enabled by default, disable with LOG_TO_FILES=0
      enableConsoleLogging: process.env.LOG_TO_CONSOLE === '1'  // Disabled by default, disable with LOG_TO_CONSOLE=0
    };

    this.fileStreams = new Map();
    this.logQueues = new Map();
    this.logTimers = new Map();
    this.cleanedFiles = new Set();
    // Use SystemPaths for correct log directory (.continuum/jtag/system/logs)
    this.logDir = SystemPaths.logs.system;

    // Initialize Rust worker client (if enabled)
    if (this.useRustLogger) {
      this.initializeWorkerClient();
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

  /**
   * Initialize Rust logger worker client asynchronously.
   * Runs in background, falls back to TypeScript logging if connection fails.
   */
  private initializeWorkerClient(): void {
    // Use /tmp for socket (filesystem limitation: APFS external drives don't support sockets in project dirs)
    const socketPath = '/tmp/jtag-logger-worker.sock';

    // Start the Rust worker process first
    const workerStarted = this.startWorkerProcess(socketPath);
    if (!workerStarted) {
      if (this.config.enableConsoleLogging) {
        console.warn('⚠️  [Logger] Failed to start Rust worker, using TypeScript logging');
      }
      return;
    }

    // Give worker time to start listening on socket (non-blocking)
    setTimeout(() => {
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
          if (this.config.enableConsoleLogging) {
            console.warn('⚠️  [Logger] Failed to connect to Rust worker, using TypeScript logging:', err.message);
          }
          // Fall back to TypeScript logging
          this.workerClient = null;
        });
    }, 1000);  // Wait 1 second for worker to start
  }

  /**
   * Start the Rust logger worker process.
   * Returns true if successfully started, false otherwise.
   */
  private startWorkerProcess(socketPath: string): boolean {
    // Path to Rust binary (relative to project root, not compiled JS location)
    const workerBinary = path.join(process.cwd(), 'workers/logger/target/release/logger-worker');

    // Check if binary exists
    if (!fs.existsSync(workerBinary)) {
      if (this.config.enableConsoleLogging) {
        console.error('❌ [Logger] Rust worker binary not found at:', workerBinary);
        console.error('   Build it with: cd workers/logger && cargo build --release');
      }
      return false;
    }

    // Ensure workers directory exists
    const workersDir = path.dirname(socketPath);
    if (!fs.existsSync(workersDir)) {
      try {
        fs.mkdirSync(workersDir, { recursive: true });
      } catch (err) {
        if (this.config.enableConsoleLogging) {
          console.error('❌ [Logger] Failed to create workers directory:', err);
        }
        return false;
      }
    }

    // Remove old socket if it exists
    if (fs.existsSync(socketPath)) {
      try {
        fs.unlinkSync(socketPath);
      } catch (err) {
        if (this.config.enableConsoleLogging) {
          console.warn('⚠️  [Logger] Failed to remove old socket file:', err);
        }
      }
    }

    // Spawn the worker process
    try {
      this.workerProcess = spawn(workerBinary, [socketPath], {
        detached: false,  // Keep attached so it dies with parent
        stdio: 'ignore'   // Don't pipe stdout/stderr
      });

      // Monitor for crashes
      this.workerProcess.on('exit', (code, signal) => {
        if (code !== null && code !== 0) {
          if (this.config.enableConsoleLogging) {
            console.error(`❌ [Logger] Rust worker exited with code ${code}`);
          }
        }
        this.workerProcess = null;
        this.workerClient = null;
      });

      this.workerProcess.on('error', (err) => {
        if (this.config.enableConsoleLogging) {
          console.error('❌ [Logger] Rust worker error:', err);
        }
        this.workerProcess = null;
        this.workerClient = null;
      });

      if (this.config.enableConsoleLogging) {
        console.log('🦀 [Logger] Started Rust worker process, PID:', this.workerProcess.pid);
      }

      return true;
    } catch (error) {
      if (this.config.enableConsoleLogging) {
        console.error('❌ [Logger] Failed to spawn Rust worker:', error);
      }
      return false;
    }
  }

  static getInstance(): LoggerClass {
    if (!LoggerClass.instance) {
      LoggerClass.instance = new LoggerClass();
    }
    return LoggerClass.instance;
  }

  /**
   * Stop the Rust logger worker process gracefully.
   * Sends SIGTERM, waits, then SIGKILL if needed.
   */
  private stopWorkerProcess(): void {
    if (!this.workerProcess) {
      return;
    }

    const pid = this.workerProcess.pid;
    if (!pid) {
      return;
    }

    try {
      // Try graceful shutdown first (SIGTERM)
      this.workerProcess.kill('SIGTERM');

      // Wait up to 5 seconds for graceful shutdown
      const startTime = Date.now();
      const maxWait = 5000;

      while (Date.now() - startTime < maxWait) {
        try {
          // Check if process still exists (throws if dead)
          process.kill(pid, 0);
          // Still alive, wait a bit more
          const waitMs = 100;
          const start = Date.now();
          while (Date.now() - start < waitMs) { /* busy wait */ }
        } catch {
          // Process is dead, cleanup successful
          if (this.config.enableConsoleLogging) {
            console.log('🦀 [Logger] Rust worker shut down gracefully');
          }
          this.workerProcess = null;
          this.workerClient = null;
          return;
        }
      }

      // Still alive after 5 seconds, force kill
      this.workerProcess.kill('SIGKILL');
      if (this.config.enableConsoleLogging) {
        console.warn('⚠️  [Logger] Rust worker required SIGKILL');
      }
    } catch (err) {
      if (this.config.enableConsoleLogging) {
        console.error('❌ [Logger] Error stopping Rust worker:', err);
      }
    } finally {
      this.workerProcess = null;
      this.workerClient = null;
    }
  }

  /**
   * Restart the Rust logger worker process.
   * Useful when worker crashes or becomes unresponsive.
   */
  restartWorkerProcess(): void {
    if (this.config.enableConsoleLogging) {
      console.log('🔄 [Logger] Restarting Rust worker...');
    }

    this.stopWorkerProcess();

    const socketPath = '/tmp/logger-worker.sock';
    const started = this.startWorkerProcess(socketPath);

    if (started && this.workerClient) {
      // Reconnect the client
      this.initializeWorkerClient();
    }
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
    // E.g., category='daemons/AIProviderDaemonServer' creates .../logs/daemons/ directory
    const logFileDir = path.dirname(logFile);
    if (!fs.existsSync(logFileDir)) {
      fs.mkdirSync(logFileDir, { recursive: true, mode: 0o755 });
    }

    // Use configured file mode (from LOG_FILE_MODE env var)
    // First call for this file: use defaultFileMode, subsequent calls: append
    let flags = 'a'; // Default to append
    if (this.defaultFileMode === FileMode.CLEAN && !this.cleanedFiles.has(logFile)) {
      flags = 'w'; // First time: truncate (clean)
      this.cleanedFiles.add(logFile);
    } else if (this.defaultFileMode === FileMode.APPEND) {
      flags = 'a'; // Always append
    } else if (this.defaultFileMode === FileMode.ARCHIVE) {
      // ARCHIVE not implemented yet, fall back to append
      flags = 'a';
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
  public queueMessage(logFile: string, message: string): void {
    const queue = this.logQueues.get(logFile);
    const stream = this.fileStreams.get(logFile);

    if (!queue || !stream) {
      throw new Error(`Cannot queue log message - queue or stream not initialized for file '${logFile}'. Queue exists: ${!!queue}, Stream exists: ${!!stream}. File streams must be created via getFileStream() before queueMessage() can be called.`);
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

    // Defensive: If timer is running but queue/stream don't exist, that's a bug
    // Log error but don't crash the timer - system can continue operating
    if (!queue || !stream) {
      if (this.config.enableConsoleLogging) {
        console.error(`[Logger BUG] Flush timer running but queue/stream missing for '${logFile}'. Queue exists: ${!!queue}, Stream exists: ${!!stream}`);
      }
      return;
    }

    // Nothing to flush (legitimate - called every 100ms by timer)
    if (queue.length === 0) {
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
      if (this.config.enableConsoleLogging) {
        console.warn('⚠️ [Logger] ARCHIVE mode not implemented yet, falling back to APPEND');
      }
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

    // If mode is CLEAN and file was already cleaned this session, switch to APPEND
    // This prevents multiple CLEAN calls from truncating the same file repeatedly
    let effectiveMode = mode;
    if (mode === FileMode.CLEAN) {
      if (this.cleanedFiles.has(logFilePath)) {
        effectiveMode = FileMode.APPEND; // Already cleaned, just append from now on
        if (this.config.enableConsoleLogging) {
          console.log(`📝 [Logger] CLEAN→APPEND (already cleaned): ${path.basename(logFilePath)}`);
        }
      } else {
        this.cleanedFiles.add(logFilePath); // Mark as cleaned
        if (this.config.enableConsoleLogging) {
          console.log(`🧹 [Logger] CLEAN mode (truncating): ${path.basename(logFilePath)}`);
        }
      }
    }

    const stream = fs.createWriteStream(logFilePath, { flags: effectiveMode, mode: 0o644 });
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

    // Stop Rust worker process (if running) using graceful shutdown
    // stopWorkerProcess() handles: SIGTERM → wait → SIGKILL fallback + client cleanup
    this.stopWorkerProcess();
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

    // File output - route to Rust worker OR TypeScript queue
    if (this.logger && this.logFilePath) {
      // Try Rust worker first (if enabled and connected)
      if (this.logger.useRustLogger && this.logger.workerClient?.isConnected()) {
        this.sendToWorker(level as WorkerLogLevel, message, args);
      }
      // Fall back to TypeScript logging (file stream)
      else if (this.fileStream) {
        const formattedArgs = args.length > 0
          ? ' ' + args.map(arg =>
              typeof arg === 'object' ? inspect(arg, { depth: 2, colors: false, compact: true }) : String(arg)
            ).join(' ')
          : '';

        const logLine = `${timestamp}[${level}] ${this.component}: ${message}${formattedArgs}\n`;
        this.logger.queueMessage(this.logFilePath, logLine);
      }
    }
  }

  /**
   * Send log message to Rust worker (fire-and-forget).
   * Errors are silently ignored - logging must never block or throw.
   */
  private sendToWorker(level: WorkerLogLevel, message: string, args: any[]): void {
    if (!this.logger || !this.logger.workerClient || !this.logFilePath) {
      return;
    }

    // Extract category from logFilePath
    // E.g., '/path/to/logs/sql.log' -> 'sql'
    //       '/path/to/logs/daemons/UserDaemonServer.log' -> 'daemons/UserDaemonServer'
    const category = this.logFilePath
      .replace(this.logger['logDir'], '')  // Remove base path
      .replace(/^\//, '')                   // Remove leading slash
      .replace(/\.log$/, '');               // Remove .log extension

    // Fire-and-forget (don't await, don't catch errors)
    this.logger.workerClient.writeLog({
      category,
      level,
      component: this.component,
      message,
      args: args.length > 0 ? args : undefined
    }).catch(() => {
      // Silently fall back to TypeScript logging on worker error
      // Don't log the error - would cause infinite recursion
    });
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

  /**
   * Write raw pre-formatted message to log file
   * Used by PersonaLogger which already formats its messages
   * NOTE: Does NOT check enableFileLogging - persona logs are always written
   */
  writeRaw(message: string): void {
    if (this.fileStream && this.logFilePath && this.logger) {
      this.logger.queueMessage(this.logFilePath, message);
    }
  }

  /**
   * Get the file path for this logger (for testing/debugging)
   */
  getLogFilePath(): string | undefined {
    return this.logFilePath;
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
