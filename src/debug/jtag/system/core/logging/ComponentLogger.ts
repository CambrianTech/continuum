/**
 * ComponentLogger - Individual logger instance for a component
 *
 * Created by Logger.create(), handles formatting and routing of log messages.
 *
 * Includes timing and inspection utilities for debugging:
 *   log.time('operation')      - Start a timer
 *   log.timeEnd('operation')   - End timer and log duration
 *   log.inspect('label', vars) - Structured variable dump for AI analysis
 *   log.checkpoint('label')    - Mark execution point with stack location
 *   log.timed('label', fn)     - Async wrapper that times an operation
 */

import { inspect as utilInspect } from 'util';
import { performance } from 'perf_hooks';
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
  private timers: Map<string, number> = new Map();

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
              typeof arg === 'object' ? utilInspect(arg, { depth: 2, colors: false, compact: true }) : String(arg)
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

  // ===========================================================================
  // Timing & Inspection Utilities
  // ===========================================================================

  /**
   * Get a clean call stack (strips internal frames)
   */
  private getCallStack(skipFrames: number = 2): string {
    const stack = new Error().stack || '';
    const lines = stack.split('\n').slice(skipFrames + 1); // +1 for "Error" line

    // Filter out node internals and keep relevant frames
    const relevant = lines
      .filter(line => !line.includes('node:internal'))
      .slice(0, 5)  // Keep top 5 frames
      .map(line => line.trim().replace(/^at\s+/, ''))
      .join('\n    ');

    return relevant;
  }

  /**
   * Start a performance timer
   *
   * @example
   * log.time('db-query');
   * const result = await db.query(sql);
   * log.timeEnd('db-query');
   */
  time(label: string): void {
    this.timers.set(label, performance.now());
  }

  /**
   * End a timer and log the duration with call stack
   *
   * @example
   * log.timeEnd('db-query');
   * // Output: ⏱️ db-query: 42.31ms
   * //         @ SqliteAdapter.query (adapters/SqliteAdapter.ts:142)
   */
  timeEnd(label: string): void {
    const start = this.timers.get(label);
    if (start === undefined) {
      this.warn(`Timer '${label}' does not exist`);
      return;
    }

    const duration = performance.now() - start;
    const ms = duration.toFixed(2);
    this.timers.delete(label);

    const stack = this.getCallStack(2);
    this.info(`⏱️ ${label}: ${ms}ms\n    @ ${stack}`);
  }

  /**
   * Structured variable dump for AI analysis
   * Uses debug level so it won't spam production logs
   *
   * @example
   * log.inspect('user-state', { userId, sessionCount, lastActive });
   * // Output: 🔍 INSPECT [user-state]
   * //         @ PersonaUser.process (PersonaUser.ts:245)
   * //         {
   * //           "userId": "abc123",
   * //           "sessionCount": 3
   * //         }
   */
  inspect(label: string, vars: Record<string, unknown>): void {
    if (!this.shouldLog(LogLevel.DEBUG)) {
      return;
    }

    const stack = this.getCallStack(2);
    const formatted = JSON.stringify(vars, null, 2)
      .split('\n')
      .map(line => '    ' + line)
      .join('\n');

    this.debug(`🔍 INSPECT [${label}]\n    @ ${stack}\n${formatted}`);
  }

  /**
   * Checkpoint marker - like a breakpoint but logs instead of stopping
   * Shows exact location in code with optional context
   *
   * @example
   * log.checkpoint('after-auth');
   * // Output: 📍 CHECKPOINT [after-auth]
   * //         @ AuthService.validate (AuthService.ts:87)
   *
   * log.checkpoint('user-loaded', { userId, role });
   * // Output: 📍 CHECKPOINT [user-loaded]
   * //         @ UserDaemon.loadUser (UserDaemon.ts:142)
   * //         { userId: "abc", role: "admin" }
   */
  checkpoint(label: string, context?: Record<string, unknown>): void {
    const stack = this.getCallStack(2);

    let msg = `📍 CHECKPOINT [${label}]\n    @ ${stack}`;

    if (context) {
      const formatted = JSON.stringify(context, null, 2)
        .split('\n')
        .map(line => '    ' + line)
        .join('\n');
      msg += `\n${formatted}`;
    }

    this.info(msg);
  }

  /**
   * Timed async operation wrapper
   * Automatically times the operation and logs duration with stack
   *
   * @example
   * const user = await log.timed('fetch-user', async () => {
   *   return await userService.getFullProfile(userId);
   * });
   * // Output: ⏱️ fetch-user: 127.45ms
   * //         @ UserController.getUser (UserController.ts:54)
   */
  async timed<T>(label: string, fn: () => Promise<T>): Promise<T> {
    const start = performance.now();
    const stack = this.getCallStack(2);

    try {
      return await fn();
    } finally {
      const duration = performance.now() - start;
      const ms = duration.toFixed(2);
      this.info(`⏱️ ${label}: ${ms}ms\n    @ ${stack}`);
    }
  }

  /**
   * Synchronous version of timed()
   */
  timedSync<T>(label: string, fn: () => T): T {
    const start = performance.now();
    const stack = this.getCallStack(2);

    try {
      return fn();
    } finally {
      const duration = performance.now() - start;
      const ms = duration.toFixed(2);
      this.info(`⏱️ ${label}: ${ms}ms\n    @ ${stack}`);
    }
  }

  /**
   * JTAG - Sophisticated trace statement for debugging
   * Named after the hardware debugging interface (Joint Test Action Group)
   *
   * Captures full context: label, variables, call stack, and timestamp.
   * Designed for AI-readable output during complex debugging sessions.
   *
   * @example
   * log.jtag('persona-decision', {
   *   userId: persona.id,
   *   messageCount: inbox.length,
   *   energy: state.energy,
   *   decision: 'respond'
   * });
   *
   * // Output:
   * // 🔬 JTAG [persona-decision]
   * //     @ PersonaUser.processInbox (PersonaUser.ts:312)
   * //       PersonaInbox.peek (PersonaInbox.ts:45)
   * //       ...
   * //     {
   * //       "userId": "helper-ai",
   * //       "messageCount": 3,
   * //       "energy": 0.85,
   * //       "decision": "respond"
   * //     }
   */
  jtag(label: string, context?: Record<string, unknown>): void {
    const stack = this.getCallStack(2);

    let msg = `🔬 JTAG [${label}]\n    @ ${stack}`;

    if (context) {
      const formatted = JSON.stringify(context, null, 2)
        .split('\n')
        .map(line => '    ' + line)
        .join('\n');
      msg += `\n${formatted}`;
    }

    // Always log at info level - JTAG traces are important
    this.info(msg);
  }
}
