/**
 * LoggerDaemonServer - Log Aggregation and File Management
 *
 * Handles all log writing operations for the system.
 * Each component+category gets its own log file and settings.
 *
 * Features:
 * - Per-instance file management (component:category → file path)
 * - Buffered writing with periodic flush
 * - File rotation support (size-based)
 * - Queue management (drop logs if queue full)
 * - Statistics tracking (logs written, dropped, queue metrics)
 */

import * as fs from 'fs';
import * as path from 'path';
import type {
  LogMessage,
  LoggerSettings,
  LogLevel,
  FileMode
} from '../shared/LoggerDaemonTypes';
import { DEFAULT_LOGGER_SETTINGS } from '../shared/LoggerDaemonTypes';

/**
 * Log entry in write queue
 */
interface QueuedLog {
  readonly filePath: string;
  readonly formattedMessage: string;
  readonly timestamp: string;
  readonly level: LogLevel;
  readonly component: string;
  readonly category: string;
}

/**
 * Per-instance logger state
 */
interface LoggerInstance {
  readonly component: string;
  readonly category: string;
  readonly filePath: string;
  settings: LoggerSettings;
  writeStream?: fs.WriteStream;
  lastFlush: number;
  logsWritten: number;
  logsDropped: number;
  currentFileSize: number;
}

/**
 * Daemon statistics
 */
export interface DaemonStats {
  readonly totalLogs: number;
  readonly byLevel: Record<LogLevel, number>;
  readonly queueSize: number;
  readonly peakQueueSize: number;
  readonly droppedLogs: number;
  readonly instanceCount: number;
  readonly instances: Array<{
    readonly component: string;
    readonly category: string;
    readonly logsWritten: number;
    readonly logsDropped: number;
    readonly settings: LoggerSettings;
  }>;
}

/**
 * LoggerDaemonServer - Main log aggregation implementation
 *
 * Manages all log writing for the system in a centralized daemon process.
 */
export class LoggerDaemonServer {
  private instances = new Map<string, LoggerInstance>(); // component:category → instance
  private writeQueue: QueuedLog[] = [];
  private flushTimer?: NodeJS.Timeout;
  private readonly logBaseDir: string;
  private readonly maxGlobalQueueSize = 10000;
  private peakQueueSize = 0;
  private totalLogsReceived = 0;
  private totalLogsWritten = 0;
  private totalLogsDropped = 0;
  private logsByLevel: Record<LogLevel, number> = {
    debug: 0,
    info: 0,
    warn: 0,
    error: 0
  };

  constructor(logBaseDir: string = '.continuum/jtag/logs/system') {
    this.logBaseDir = logBaseDir;
    this.ensureLogDirectory();
  }

  /**
   * Initialize the daemon
   */
  async initialize(): Promise<void> {
    console.log(`[LoggerDaemonServer] Initializing (logDir: ${this.logBaseDir})`);

    // Start periodic flush timer (every 1 second)
    this.flushTimer = setInterval(() => {
      this.flushQueue();
    }, 1000);

    console.log(`[LoggerDaemonServer] Ready`);
  }

  /**
   * Handle incoming log message
   */
  async handleLogMessage(message: LogMessage): Promise<void> {
    this.totalLogsReceived++;
    this.logsByLevel[message.data.level]++;

    // Get or create logger instance
    const instance = this.getOrCreateInstance(
      message.data.component,
      message.data.category,
      message.data.settings
    );

    // Check if log level should be written
    if (!this.shouldLog(message.data.level, instance.settings.level)) {
      return;
    }

    // Check global queue size
    if (this.writeQueue.length >= this.maxGlobalQueueSize) {
      instance.logsDropped++;
      this.totalLogsDropped++;
      console.warn(
        `[LoggerDaemonServer] Queue full (${this.writeQueue.length}), dropping log from ${message.data.component}`
      );
      return;
    }

    // Format message
    const formattedMessage = this.formatLogMessage(
      message.data.level,
      message.data.component,
      message.data.message,
      message.data.args,
      message.data.timestamp
    );

    // Add to queue
    const queuedLog: QueuedLog = {
      filePath: instance.filePath,
      formattedMessage,
      timestamp: message.data.timestamp,
      level: message.data.level,
      component: message.data.component,
      category: message.data.category
    };

    this.writeQueue.push(queuedLog);

    // Update peak queue size
    if (this.writeQueue.length > this.peakQueueSize) {
      this.peakQueueSize = this.writeQueue.length;
    }

    // Flush immediately if queue is large
    if (this.writeQueue.length >= 100) {
      await this.flushQueue();
    }
  }

  /**
   * Get or create logger instance for component+category
   */
  private getOrCreateInstance(
    component: string,
    category: string,
    settings: LoggerSettings
  ): LoggerInstance {
    const key = `${component}:${category}`;

    let instance = this.instances.get(key);
    if (instance) {
      // Update settings if changed
      instance.settings = settings;
      return instance;
    }

    // Create new instance
    const filePath = this.getLogFilePath(category);
    instance = {
      component,
      category,
      filePath,
      settings,
      lastFlush: Date.now(),
      logsWritten: 0,
      logsDropped: 0,
      currentFileSize: this.getFileSize(filePath)
    };

    this.instances.set(key, instance);
    console.log(`[LoggerDaemonServer] Created logger instance: ${key} → ${filePath}`);

    // Handle file mode
    this.handleFileMode(instance);

    return instance;
  }

  /**
   * Handle file mode (overwrite, append, rotate)
   */
  private handleFileMode(instance: LoggerInstance): void {
    const { filePath, settings } = instance;

    if (settings.fileMode === 'overwrite') {
      // Clear existing file
      if (fs.existsSync(filePath)) {
        fs.unlinkSync(filePath);
        console.log(`[LoggerDaemonServer] Cleared log file: ${filePath}`);
      }
    } else if (settings.fileMode === 'rotate') {
      // Check if rotation needed
      if (settings.maxFileSizeBytes && instance.currentFileSize > settings.maxFileSizeBytes) {
        this.rotateLogFile(instance);
      }
    }
    // 'append' mode: do nothing, just append to existing file
  }

  /**
   * Rotate log file (move to .1, .2, .3, etc.)
   */
  private rotateLogFile(instance: LoggerInstance): void {
    const { filePath, settings } = instance;
    const maxRotated = settings.maxRotatedFiles || 5;

    // Delete oldest file if exists
    const oldestFile = `${filePath}.${maxRotated}`;
    if (fs.existsSync(oldestFile)) {
      fs.unlinkSync(oldestFile);
    }

    // Shift files: .3 → .4, .2 → .3, .1 → .2
    for (let i = maxRotated - 1; i >= 1; i--) {
      const fromFile = `${filePath}.${i}`;
      const toFile = `${filePath}.${i + 1}`;
      if (fs.existsSync(fromFile)) {
        fs.renameSync(fromFile, toFile);
      }
    }

    // Move current file to .1
    if (fs.existsSync(filePath)) {
      fs.renameSync(filePath, `${filePath}.1`);
    }

    // Reset file size
    instance.currentFileSize = 0;
    console.log(`[LoggerDaemonServer] Rotated log file: ${filePath}`);
  }

  /**
   * Format log message with timestamp, level, component
   */
  private formatLogMessage(
    level: LogLevel,
    component: string,
    message: string,
    args: readonly unknown[],
    timestamp: string
  ): string {
    const levelEmoji = this.getLevelEmoji(level);
    const levelStr = level.toUpperCase().padEnd(5);

    // Format arguments
    let argsStr = '';
    if (args.length > 0) {
      argsStr = ' ' + args.map(arg => {
        if (typeof arg === 'object') {
          try {
            return JSON.stringify(arg);
          } catch {
            return String(arg);
          }
        }
        return String(arg);
      }).join(' ');
    }

    return `[${timestamp}] ${levelEmoji} ${levelStr} [${component}] ${message}${argsStr}\n`;
  }

  /**
   * Get emoji for log level
   */
  private getLevelEmoji(level: LogLevel): string {
    switch (level) {
      case 'debug': return '🔍';
      case 'info': return 'ℹ️';
      case 'warn': return '⚠️';
      case 'error': return '❌';
      default: return '📝';
    }
  }

  /**
   * Check if log level should be written
   */
  private shouldLog(messageLevel: LogLevel, settingsLevel: LogLevel): boolean {
    const levels: LogLevel[] = ['debug', 'info', 'warn', 'error'];
    const messageIndex = levels.indexOf(messageLevel);
    const settingsIndex = levels.indexOf(settingsLevel);
    return messageIndex >= settingsIndex;
  }

  /**
   * Flush write queue to disk
   */
  private async flushQueue(): Promise<number> {
    if (this.writeQueue.length === 0) {
      return 0;
    }

    const toFlush = [...this.writeQueue];
    this.writeQueue = [];

    // Group logs by file path
    const byFile = new Map<string, QueuedLog[]>();
    for (const log of toFlush) {
      const existing = byFile.get(log.filePath) || [];
      existing.push(log);
      byFile.set(log.filePath, existing);
    }

    // Write to each file
    let flushedCount = 0;
    for (const [filePath, logs] of byFile.entries()) {
      try {
        const content = logs.map(log => log.formattedMessage).join('');
        fs.appendFileSync(filePath, content, 'utf8');

        flushedCount += logs.length;
        this.totalLogsWritten += logs.length;

        // Update instance stats
        const firstLog = logs[0];
        const key = `${firstLog.component}:${firstLog.category}`;
        const instance = this.instances.get(key);
        if (instance) {
          instance.logsWritten += logs.length;
          instance.lastFlush = Date.now();
          instance.currentFileSize += content.length;

          // Check rotation
          if (
            instance.settings.fileMode === 'rotate' &&
            instance.settings.maxFileSizeBytes &&
            instance.currentFileSize > instance.settings.maxFileSizeBytes
          ) {
            this.rotateLogFile(instance);
          }
        }
      } catch (error) {
        console.error(`[LoggerDaemonServer] Failed to write logs to ${filePath}:`, error);
      }
    }

    return flushedCount;
  }

  /**
   * Get log file path for category
   */
  private getLogFilePath(category: string): string {
    return path.join(this.logBaseDir, `${category}.log`);
  }

  /**
   * Get file size in bytes
   */
  private getFileSize(filePath: string): number {
    try {
      if (fs.existsSync(filePath)) {
        return fs.statSync(filePath).size;
      }
    } catch {
      // File doesn't exist or not readable
    }
    return 0;
  }

  /**
   * Ensure log directory exists
   */
  private ensureLogDirectory(): void {
    if (!fs.existsSync(this.logBaseDir)) {
      fs.mkdirSync(this.logBaseDir, { recursive: true });
      console.log(`[LoggerDaemonServer] Created log directory: ${this.logBaseDir}`);
    }
  }

  /**
   * Get daemon statistics
   */
  getStats(): DaemonStats {
    const instances = Array.from(this.instances.values()).map(instance => ({
      component: instance.component,
      category: instance.category,
      logsWritten: instance.logsWritten,
      logsDropped: instance.logsDropped,
      settings: instance.settings
    }));

    return {
      totalLogs: this.totalLogsWritten,
      byLevel: { ...this.logsByLevel },
      queueSize: this.writeQueue.length,
      peakQueueSize: this.peakQueueSize,
      droppedLogs: this.totalLogsDropped,
      instanceCount: this.instances.size,
      instances
    };
  }

  /**
   * Graceful shutdown
   */
  async shutdown(): Promise<void> {
    console.log(`[LoggerDaemonServer] Shutting down (flushing ${this.writeQueue.length} queued logs)...`);

    // Stop flush timer
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = undefined;
    }

    // Flush remaining queue
    const flushed = await this.flushQueue();
    console.log(`[LoggerDaemonServer] Flushed ${flushed} logs`);

    // Close all write streams
    for (const instance of this.instances.values()) {
      if (instance.writeStream) {
        instance.writeStream.end();
      }
    }

    console.log(`[LoggerDaemonServer] Shutdown complete`);
  }
}
