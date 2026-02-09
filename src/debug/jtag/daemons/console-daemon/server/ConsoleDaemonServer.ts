/**
 * Console Daemon - Server Implementation
 *
 * Server-specific console daemon that delegates to Rust logger worker for efficient I/O.
 * NO file I/O in TypeScript - all writes go through Unix socket to Rust.
 */

import { ConsoleDaemon } from '../shared/ConsoleDaemon';
import type { ConsolePayload } from '../shared/ConsoleDaemon';
import { shouldDualScope } from '../../../system/core/types/SystemScopes';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient';
import type { LogLevel as WorkerLogLevel } from '../../../shared/ipc/logger/LoggerMessageTypes';

export class ConsoleDaemonServer extends ConsoleDaemon {
  // LoggerModule is now part of continuum-core (Phase 4a)
  private readonly SOCKET_PATH = '/tmp/continuum-core.sock';
  private loggerClient: LoggerWorkerClient | null = null;
  private connectionAttempted = false;
  private connectionFailed = false;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Initialize and connect to Rust logger worker
   */
  protected override async initialize(): Promise<void> {
    await super.initialize();

    // Try to connect to Rust logger worker
    await this.connectToRustWorker();
  }

  private async connectToRustWorker(): Promise<void> {
    if (this.connectionAttempted) return;
    this.connectionAttempted = true;

    try {
      this.loggerClient = new LoggerWorkerClient({
        socketPath: this.SOCKET_PATH,
        timeout: 5000,
        userId: 'console-daemon'
      });

      await this.loggerClient.connect();
      this.originalConsole.log(`🦀 ${this.toString()}: Connected to Rust logger worker`);
    } catch (error) {
      // Rust worker not running - fall back to no-op (logs still go to console)
      this.connectionFailed = true;
      this.loggerClient = null;
      this.originalConsole.warn(`⚠️ ${this.toString()}: Rust logger worker not available - console logs will not be persisted to files`);
    }
  }

  /**
   * Map console log level to worker log level ('log' -> 'info')
   */
  private mapLogLevel(level: string): WorkerLogLevel {
    return level === 'log' ? 'info' : level as WorkerLogLevel;
  }

  /**
   * Process console payload - send to Rust logger worker
   * FAST: No file I/O in TypeScript, just IPC to Rust
   */
  protected async processConsolePayload(consolePayload: ConsolePayload): Promise<void> {
    // Skip if no Rust worker connection
    if (!this.loggerClient || this.connectionFailed) {
      return; // Logs still went to console, just not persisted
    }

    const workerLevel = this.mapLogLevel(consolePayload.level);

    try {
      // Send to Rust worker for efficient batched file I/O
      await this.loggerClient.writeLog({
        category: `system/${consolePayload.context.environment}`,
        level: workerLevel,
        component: consolePayload.component,
        message: consolePayload.message,
        args: consolePayload.data ? [consolePayload.data] : undefined
      });

      // Session logs (if applicable)
      if (consolePayload.sessionId !== this.context.uuid && shouldDualScope(consolePayload.sessionId)) {
        await this.loggerClient.writeLog({
          category: `sessions/${consolePayload.sessionId}`,
          level: workerLevel,
          component: consolePayload.component,
          message: consolePayload.message,
          args: consolePayload.data ? [consolePayload.data] : undefined
        });
      }
    } catch (error) {
      // Don't spam console on every write failure - just note connection lost
      if (!this.connectionFailed) {
        this.connectionFailed = true;
        this.originalConsole.warn(`⚠️ ${this.toString()}: Lost connection to Rust logger worker`);
      }
    }
  }

  private async notifyErrorMonitoring(consolePayload: ConsolePayload): Promise<void> {
    // Future: Send to error monitoring service
    // For now, errors are already logged via Rust worker
  }
}
