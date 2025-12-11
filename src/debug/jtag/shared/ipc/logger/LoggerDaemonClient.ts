/**
 * LoggerDaemonClient - IPC client for sending logs to LoggerDaemonProcess
 *
 * Sends log messages via IPC to the LoggerDaemon child process, which:
 * 1. Writes headers automatically on first log for each category
 * 2. Forwards to multi-threaded Rust worker
 * 3. Rust worker writes to category-based log files
 */

import type { ManagedProcess } from '../../../system/core/process/ProcessManager';
import type { LogMessage, LoggerSettings } from '../../../daemons/logger-daemon/shared/LoggerDaemonTypes';

const DEFAULT_SETTINGS: LoggerSettings = {
  level: 'debug',
  fileMode: 'append',
  flushIntervalMs: 100,
  maxQueueSize: 1000,
  enabled: true
};

export class LoggerDaemonClient {
  private process: ManagedProcess | null = null;
  private messageId = 0;

  /**
   * Set the logger daemon process handle
   */
  setProcess(process: ManagedProcess): void {
    this.process = process;
  }

  /**
   * Send log message to daemon via IPC
   */
  async sendLog(
    component: string,
    category: string,
    level: 'debug' | 'info' | 'warn' | 'error',
    message: string,
    args: unknown[]
  ): Promise<void> {
    if (!this.process) {
      throw new Error('LoggerDaemonClient: process not set');
    }

    const logMessage: LogMessage = {
      type: 'log',
      messageId: `log-${++this.messageId}`,
      data: {
        level,
        component,
        category,
        message,
        args,
        timestamp: new Date().toISOString(),
        settings: DEFAULT_SETTINGS
      }
    };

    // Send to daemon process, await response
    console.log(`[LoggerDaemonClient] Sending IPC message: ${component}:${category}`);
    try {
      await this.process.send(logMessage);
      console.log(`[LoggerDaemonClient] IPC send successful`);
    } catch (err) {
      console.error(`[LoggerDaemonClient] IPC send FAILED:`, err);
      throw err;
    }
  }

  /**
   * Check if client is connected to daemon
   */
  isConnected(): boolean {
    return this.process !== null;
  }
}
