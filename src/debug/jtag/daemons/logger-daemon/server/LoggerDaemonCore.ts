/**
 * LoggerDaemonCore - Log Aggregation and File Management Engine
 *
 * Pure log aggregation implementation (no daemon infrastructure).
 * Used by LoggerDaemonProcess (child process entry point).
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
import { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient';

// DEBUG LOG FILE - ALWAYS FINDABLE
const DEBUG_LOG = '/tmp/logger-daemon-debug.log';
function debugLog(msg: string): void {
  const timestamp = new Date().toISOString();
  fs.appendFileSync(DEBUG_LOG, `[${timestamp}] ${msg}\n`);
}

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
 * LoggerDaemonCore - Main log aggregation implementation
 *
 * Manages all log writing for the system in a centralized daemon process.
 */
export class LoggerDaemonCore {
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
  private workerClient: LoggerWorkerClient;
  private headersWritten = new Set<string>(); // Track categories that have headers

  constructor(logBaseDir: string = '.continuum/jtag/logs/system') {
    debugLog('CONSTRUCTOR START');
    this.logBaseDir = logBaseDir;
    this.ensureLogDirectory();

    // Create Rust worker client
    const socketPath = '/tmp/jtag-logger-worker.sock';
    debugLog(`Creating worker client for ${socketPath}`);
    this.workerClient = new LoggerWorkerClient(socketPath);
    debugLog('CONSTRUCTOR END');
  }

  /**
   * Initialize the daemon
   */
  async initialize(): Promise<void> {
    debugLog('INITIALIZE START');

    try {
      debugLog('Calling workerClient.connect()...');
      await this.workerClient.connect();
      debugLog('✅ CONNECTED to Rust worker successfully');

      // VERIFICATION TEST: Send test log to verify Rust worker receives it
      debugLog('Sending verification test log to Rust worker...');
      const testResult = await this.workerClient.writeLog({
        category: 'daemon-verification',
        level: 'info',
        component: 'LoggerDaemonCore',
        message: `Daemon initialized successfully at ${new Date().toISOString()}`,
        args: ['PID:' + process.pid, 'Socket:/tmp/jtag-logger-worker.sock']
      });
      debugLog(`✅ VERIFICATION TEST PASSED - Rust worker wrote ${testResult.bytesWritten} bytes`);
      console.log(`🎯 LoggerDaemonCore: Verification test passed - ${testResult.bytesWritten} bytes written to Rust worker`);

    } catch (error) {
      const err = error as Error;
      debugLog(`❌ CONNECTION FAILED: ${err.message}`);
      debugLog(`Error stack: ${err.stack}`);
      throw error;
    }
  }

  /**
   * Write header to log file if this is the first message for this category
   * Header is generated dynamically from the component name
   */
  private async writeHeaderIfNeeded(category: string, component: string): Promise<void> {
    // Skip if header already written for this category
    if (this.headersWritten.has(category)) {
      return;
    }

    const timestamp = new Date().toISOString();
    const sessionId = `session-${Date.now()}`;

    const header = [
      '================================================================================',
      `COMPONENT: ${component}`,
      `CATEGORY: ${category}`,
      `SESSION: ${sessionId}`,
      `STARTED: ${timestamp}`,
      `PID: ${process.pid}`,
      '================================================================================',
      '',
      'LOG FORMAT:',
      '  [RUST] [timestamp] [LEVEL] Component: message [args]',
      '',
      'LOG LEVELS:',
      '  DEBUG - Detailed diagnostic information',
      '  INFO  - General informational messages',
      '  WARN  - Warning messages (potential issues)',
      '  ERROR - Error messages (failures)',
      '',
      '================================================================================',
      'LOG ENTRIES BEGIN BELOW',
      '================================================================================',
      ''
    ].join('\n');

    await this.workerClient.writeLog({
      category,
      level: 'info',
      component,
      message: header,
      args: []
    });

    this.headersWritten.add(category);
    debugLog(`📋 Wrote header for category: ${category} (component: ${component})`);
  }

  /**
   * Handle incoming log message - forward to Rust worker
   * Writes header automatically on first message for each category
   */
  async handleLogMessage(message: LogMessage): Promise<void> {
    debugLog(`HANDLE LOG: ${message.data.component}:${message.data.category}`);
    this.totalLogsReceived++;
    this.logsByLevel[message.data.level]++;

    // Write header if this is the first message for this category
    await this.writeHeaderIfNeeded(message.data.category, message.data.component);

    // Send to Rust worker - NO FALLBACK, THROW ON ERROR
    debugLog('Sending to Rust worker...');
    await this.workerClient.writeLog({
      category: message.data.category,
      level: message.data.level,
      component: message.data.component,
      message: message.data.message,
      args: [...message.data.args]
    });
    debugLog('✅ Sent successfully');
    this.totalLogsWritten++;
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
    console.log(`[LoggerDaemonServer] Shutting down...`);

    // Stop flush timer
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = undefined;
    }

    // Disconnect from Rust worker
    try {
      await this.workerClient.disconnect();
      console.log(`[LoggerDaemonServer] Disconnected from Rust worker`);
    } catch (error) {
      console.error(`[LoggerDaemonServer] Error disconnecting from Rust worker:`, error);
    }

    console.log(`[LoggerDaemonServer] Shutdown complete`);
  }
}
