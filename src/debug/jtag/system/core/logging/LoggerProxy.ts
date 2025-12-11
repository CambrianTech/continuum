/**
 * LoggerProxy - IPC Client for LoggerDaemon
 *
 * Replaces Logger.ts functionality by sending log messages to LoggerDaemon process.
 * Maintains identical API surface for backward compatibility.
 *
 * Usage (same as Logger):
 *   import { LoggerProxy as Logger } from '@system/core/logging/LoggerProxy';
 *   const log = Logger.create('SqliteQueryExecutor', 'sql');
 *   log.debug('Query details', { sql, params });
 *
 * Design:
 * - Fire-and-forget: log calls never block the caller
 * - Per-instance settings: each component+category has its own configuration
 * - Local fallback: if daemon unavailable, logs to console (degraded mode)
 * - Queue management: messages buffered until daemon ready
 */

import type {
  LogMessage,
  LogResponse,
  LoggerSettings,
  LogLevel as DaemonLogLevel
} from '../../../daemons/logger-daemon/shared/LoggerDaemonTypes';
import {
  createLoggerSettings,
  DEFAULT_LOGGER_SETTINGS
} from '../../../daemons/logger-daemon/shared/LoggerDaemonTypes';
import { createIPCMessage } from '../process/IPCProtocol';
import type { ManagedProcess } from '../process/ProcessManager';

/**
 * Log levels (matches Logger.ts enum)
 */
export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
  SILENT = 4
}

/**
 * File mode for log writing (matches Logger.ts)
 */
export enum FileMode {
  CLEAN = 'w',
  APPEND = 'a',
  ARCHIVE = 'archive'
}

/**
 * Queued log entry (for local buffering when daemon unavailable)
 */
interface QueuedLogEntry {
  readonly level: DaemonLogLevel;
  readonly component: string;
  readonly category: string;
  readonly message: string;
  readonly args: readonly unknown[];
  readonly timestamp: string;
  readonly settings: LoggerSettings;
}

/**
 * LoggerProxyClass - Manages IPC communication with LoggerDaemon
 *
 * Singleton that maintains connection to daemon process and routes log messages.
 */
class LoggerProxyClass {
  private static instance: LoggerProxyClass;
  private daemonProcess?: ManagedProcess;
  private localQueue: QueuedLogEntry[] = [];
  private readonly MAX_LOCAL_QUEUE = 1000;
  private settingsCache: Map<string, LoggerSettings> = new Map(); // component+category → settings
  private daemonReady = false;
  private minLogLevel: LogLevel = LogLevel.INFO;
  private enableConsoleLogging = false;

  private constructor() {
    // Read configuration from environment
    const envLevel = process.env.LOG_LEVEL?.toUpperCase() || 'INFO';
    const levelMap: Record<string, LogLevel> = {
      'DEBUG': LogLevel.DEBUG,
      'INFO': LogLevel.INFO,
      'WARN': LogLevel.WARN,
      'ERROR': LogLevel.ERROR,
      'SILENT': LogLevel.SILENT
    };
    this.minLogLevel = levelMap[envLevel] || LogLevel.INFO;
    this.enableConsoleLogging = process.env.LOG_TO_CONSOLE === '1';
  }

  static getInstance(): LoggerProxyClass {
    if (!LoggerProxyClass.instance) {
      LoggerProxyClass.instance = new LoggerProxyClass();
    }
    return LoggerProxyClass.instance;
  }

  /**
   * Register LoggerDaemon process
   * Called by JTAGSystem after spawning daemon
   */
  registerDaemon(process: ManagedProcess): void {
    this.daemonProcess = process;
    this.daemonReady = true;

    // Flush local queue to daemon
    this.flushLocalQueue();
  }

  /**
   * Flush locally queued messages to daemon
   */
  private flushLocalQueue(): void {
    if (!this.daemonProcess || !this.daemonReady) {
      return;
    }

    while (this.localQueue.length > 0) {
      const entry = this.localQueue.shift();
      if (entry) {
        this.sendToDaemon(entry);
      }
    }
  }

  /**
   * Send log message to daemon (or queue locally)
   * Fire-and-forget - never blocks caller
   */
  private sendLogMessage(
    level: DaemonLogLevel,
    component: string,
    category: string,
    message: string,
    args: unknown[],
    settings: LoggerSettings
  ): void {
    const entry: QueuedLogEntry = {
      level,
      component,
      category,
      message,
      args: Object.freeze([...args]),
      timestamp: new Date().toISOString(),
      settings
    };

    // If daemon not ready, queue locally
    if (!this.daemonReady || !this.daemonProcess) {
      if (this.localQueue.length < this.MAX_LOCAL_QUEUE) {
        this.localQueue.push(entry);
      } else {
        // Queue full, drop message (degraded mode)
        if (this.enableConsoleLogging) {
          console.warn(`[LoggerProxy] Local queue full, dropping log: ${component}: ${message}`);
        }
      }
      return;
    }

    // Send to daemon
    this.sendToDaemon(entry);
  }

  /**
   * Send queued entry to daemon via IPC
   */
  private sendToDaemon(entry: QueuedLogEntry): void {
    if (!this.daemonProcess) {
      return;
    }

    const logMessage: LogMessage = {
      type: 'log',
      messageId: `log-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      timestamp: entry.timestamp,
      data: {
        level: entry.level,
        component: entry.component,
        category: entry.category,
        message: entry.message,
        args: entry.args,
        timestamp: entry.timestamp,
        settings: entry.settings
      }
    };

    // Fire-and-forget: send IPC message without waiting for response
    try {
      this.daemonProcess.send(logMessage);
    } catch (error) {
      // Daemon send failed, fallback to console
      if (this.enableConsoleLogging) {
        console.error(`[LoggerProxy] Failed to send to daemon:`, error);
        console.log(`[${entry.component}] ${entry.message}`, ...entry.args);
      }
    }
  }

  /**
   * Get or create settings for a logger instance
   */
  private getSettings(component: string, category: string, overrides?: Partial<LoggerSettings>): LoggerSettings {
    const key = `${component}:${category}`;

    if (this.settingsCache.has(key) && !overrides) {
      return this.settingsCache.get(key)!;
    }

    // Create settings with overrides
    const settings = createLoggerSettings({
      level: this.mapLogLevel(this.minLogLevel),
      ...overrides
    });

    this.settingsCache.set(key, settings);
    return settings;
  }

  /**
   * Map LogLevel enum to DaemonLogLevel string
   */
  private mapLogLevel(level: LogLevel): DaemonLogLevel {
    switch (level) {
      case LogLevel.DEBUG: return 'debug';
      case LogLevel.INFO: return 'info';
      case LogLevel.WARN: return 'warn';
      case LogLevel.ERROR: return 'error';
      case LogLevel.SILENT: return 'error'; // Silent = only errors
      default: return 'info';
    }
  }

  /**
   * Create a logger instance for a specific component
   *
   * @param component - Component name (e.g., 'SqliteQueryExecutor')
   * @param category - Optional log category for file separation
   */
  create(component: string, category?: string): ComponentLoggerProxy {
    const actualCategory = category || component;
    const settings = this.getSettings(component, actualCategory);
    return new ComponentLoggerProxy(component, actualCategory, settings, this);
  }

  /**
   * Create a logger with custom file path
   * (Maintains API compatibility, but file path is ignored - daemon manages files)
   */
  createWithFile(component: string, logFilePath: string, mode: FileMode): ComponentLoggerProxy {
    // Extract category from file path (e.g., '.../logs/persona-mind.log' → 'persona-mind')
    const category = logFilePath.split('/').pop()?.replace('.log', '') || component;
    const settings = this.getSettings(component, category, {
      fileMode: this.mapFileMode(mode)
    });
    return new ComponentLoggerProxy(component, category, settings, this);
  }

  /**
   * Map FileMode enum to daemon FileMode string
   */
  private mapFileMode(mode: FileMode): 'append' | 'overwrite' | 'rotate' {
    switch (mode) {
      case FileMode.CLEAN: return 'overwrite';
      case FileMode.APPEND: return 'append';
      case FileMode.ARCHIVE: return 'rotate';
      default: return 'append';
    }
  }

  /**
   * Get current log level
   */
  getLevel(): LogLevel {
    return this.minLogLevel;
  }

  /**
   * Set log level programmatically
   */
  setLevel(level: LogLevel): void {
    this.minLogLevel = level;
    // Update all cached settings
    const entries = Array.from(this.settingsCache.entries());
    for (const [key, settings] of entries) {
      this.settingsCache.set(key, {
        ...settings,
        level: this.mapLogLevel(level)
      });
    }
  }

  /**
   * Shutdown proxy (flush local queue, disconnect from daemon)
   */
  shutdown(): void {
    // Flush any remaining queued messages
    this.flushLocalQueue();

    // Clear state
    this.daemonProcess = undefined;
    this.daemonReady = false;
    this.localQueue = [];
    this.settingsCache.clear();
  }

  /**
   * Internal: Send log message (called by ComponentLoggerProxy)
   */
  _sendLog(
    level: DaemonLogLevel,
    component: string,
    category: string,
    message: string,
    args: unknown[],
    settings: LoggerSettings
  ): void {
    this.sendLogMessage(level, component, category, message, args, settings);
  }

  /**
   * Internal: Check if level should be logged
   */
  _shouldLog(level: LogLevel): boolean {
    return level >= this.minLogLevel;
  }

  /**
   * Internal: Get console logging flag
   */
  _isConsoleEnabled(): boolean {
    return this.enableConsoleLogging;
  }
}

/**
 * ComponentLoggerProxy - Per-component logger instance
 *
 * Maintains same API as ComponentLogger from Logger.ts
 */
class ComponentLoggerProxy {
  constructor(
    private component: string,
    private category: string,
    private settings: LoggerSettings,
    private proxy: LoggerProxyClass
  ) {}

  private shouldLog(level: LogLevel): boolean {
    return this.proxy._shouldLog(level);
  }

  private formatMessage(level: DaemonLogLevel, emoji: string, message: string, ...args: unknown[]): void {
    const numericLevel = this.levelToNumeric(level);
    if (!this.shouldLog(numericLevel)) {
      return;
    }

    // Console output (if enabled) - for immediate feedback
    if (this.proxy._isConsoleEnabled()) {
      const timestamp = `[${new Date().toISOString()}] `;
      const prefix = `${timestamp}${emoji} ${this.component}:`;
      if (args.length === 0) {
        console.log(prefix, message);
      } else {
        console.log(prefix, message, ...args);
      }
    }

    // Send to daemon - fire-and-forget
    this.proxy._sendLog(level, this.component, this.category, message, args, this.settings);
  }

  private levelToNumeric(level: DaemonLogLevel): LogLevel {
    switch (level) {
      case 'debug': return LogLevel.DEBUG;
      case 'info': return LogLevel.INFO;
      case 'warn': return LogLevel.WARN;
      case 'error': return LogLevel.ERROR;
      default: return LogLevel.INFO;
    }
  }

  debug(message: string, ...args: unknown[]): void {
    this.formatMessage('debug', '🔍', message, ...args);
  }

  info(message: string, ...args: unknown[]): void {
    this.formatMessage('info', 'ℹ️', message, ...args);
  }

  warn(message: string, ...args: unknown[]): void {
    this.formatMessage('warn', '⚠️', message, ...args);
  }

  error(message: string, ...args: unknown[]): void {
    this.formatMessage('error', '❌', message, ...args);
  }

  /**
   * Conditional debug logging
   */
  debugIf(messageFn: () => [string, ...unknown[]]): void {
    if (this.shouldLog(LogLevel.DEBUG)) {
      const [message, ...args] = messageFn();
      this.debug(message, ...args);
    }
  }

  /**
   * Write raw pre-formatted message
   * (For PersonaLogger compatibility)
   */
  writeRaw(message: string): void {
    // Send raw message as info level
    this.proxy._sendLog('info', this.component, this.category, message, [], this.settings);
  }

  /**
   * Get log file path
   * (Returns category-based path for compatibility)
   */
  getLogFilePath(): string {
    return `.continuum/jtag/logs/system/${this.category}.log`;
  }
}

/**
 * Singleton logger proxy instance
 */
export const LoggerProxy = LoggerProxyClass.getInstance();

/**
 * Type export for component loggers
 */
export type { ComponentLoggerProxy as ComponentLogger };
