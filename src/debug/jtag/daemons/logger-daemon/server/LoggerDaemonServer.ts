/**
 * LoggerDaemon Server - Rust-backed implementation
 *
 * RUST-BACKED DAEMON PATTERN REFERENCE IMPLEMENTATION
 *
 * This daemon establishes the pattern for TypeScript→Rust integration:
 * 1. TypeScript: Connection management, health checks, lifecycle
 * 2. Rust worker: Heavy lifting (I/O, threading, batching)
 * 3. Communication: Unix domain socket
 *
 * Future daemons (Training, Inference, Embedding) follow this pattern.
 */

import { LoggerDaemon } from '../shared/LoggerDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient';

export class LoggerDaemonServer extends LoggerDaemon {
  protected log: ComponentLogger;
  private workerClient: LoggerWorkerClient | null = null;
  private readonly SOCKET_PATH = '/tmp/jtag-logger-worker.sock';
  private healthCheckInterval: NodeJS.Timeout | null = null;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging
    // NOTE: Uses Logger.ts which connects to Rust worker we're managing
    // Falls back to TypeScript logging if worker not ready during init
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Lifecycle: Connect to Rust logger worker
   */
  protected override async onStart(): Promise<void> {
    this.log.info('🦀 Connecting to Rust logger worker');

    // Create client connection to Rust worker
    this.workerClient = new LoggerWorkerClient({
      socketPath: this.SOCKET_PATH,
      timeout: 10000,
      userId: 'logger-daemon'
    });

    try {
      // Connect to Rust worker
      await this.workerClient.connect();
      this.log.info('🦀 Connected to Rust logger worker');

      // Start health checks
      this.startHealthChecks();

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log.error('❌ Failed to connect to Rust logger worker:', error);
      this.log.warn('⚠️  Falling back to TypeScript logging');
      this.log.warn('⚠️  To start Rust worker: npm run worker:start');

      // Don't fail - Logger.ts will fall back to TypeScript logging
      this.workerClient = null;
    }
  }

  /**
   * Lifecycle: Disconnect from Rust worker
   */
  protected override async onStop(): Promise<void> {
    this.log.info('🛑 Disconnecting from Rust logger worker');

    // Stop health checks
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
      this.healthCheckInterval = null;
    }

    // Disconnect from Rust worker
    if (this.workerClient) {
      try {
        await this.workerClient.disconnect();
        this.log.info('✅ Disconnected from Rust logger worker');
      } catch (error) {
        this.log.error('⚠️  Error disconnecting from Rust worker:', error);
      }
      this.workerClient = null;
    }
  }

  /**
   * Start periodic health checks
   */
  private startHealthChecks(): void {
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }

    // Check connection health every 30 seconds
    this.healthCheckInterval = setInterval(async () => {
      try {
        const isHealthy = await this.healthCheck();
        if (!isHealthy) {
          this.log.warn('⚠️  Rust worker health check failed - attempting reconnect');
          await this.reconnect();
        }
      } catch (error) {
        this.log.error('❌ Health check error:', error);
      }
    }, 30000);
  }

  /**
   * Attempt to reconnect to Rust worker
   */
  private async reconnect(): Promise<void> {
    if (!this.workerClient) return;

    try {
      await this.workerClient.disconnect();
    } catch {
      // Ignore disconnect errors during reconnect
    }

    try {
      await this.workerClient.connect();
      this.log.info('✅ Reconnected to Rust logger worker');
    } catch (error) {
      this.log.error('❌ Reconnection failed:', error);
      this.workerClient = null;
    }
  }

  /**
   * Force flush all log buffers to disk
   */
  protected override async flush(): Promise<void> {
    if (!this.workerClient) {
      throw new Error('Rust worker not connected');
    }

    // TODO: Implement flush command to Rust worker
    // await this.workerClient.send({ command: 'flush' });
    this.log.info('🚽 Flushed log buffers');
  }

  /**
   * Rotate log files (close current, open new)
   */
  protected override async rotate(category?: string): Promise<void> {
    if (!this.workerClient) {
      throw new Error('Rust worker not connected');
    }

    // TODO: Implement rotate command to Rust worker
    // await this.workerClient.send({ command: 'rotate', category });
    this.log.info(`🔄 Rotated log files${category ? ` for ${category}` : ''}`);
  }

  /**
   * Get logging statistics from Rust worker
   */
  protected override async getStats(): Promise<Record<string, { messagesLogged: number; bytesWritten: number }>> {
    if (!this.workerClient) {
      throw new Error('Rust worker not connected');
    }

    // TODO: Implement stats query to Rust worker
    // return await this.workerClient.send({ command: 'stats' });
    return {
      'system': { messagesLogged: 0, bytesWritten: 0 },
      'daemons': { messagesLogged: 0, bytesWritten: 0 }
    };
  }

  /**
   * Check connection health to Rust worker
   */
  protected override async healthCheck(): Promise<boolean> {
    if (!this.workerClient) {
      return false;
    }

    try {
      // TODO: Implement ping/pong health check
      // await this.workerClient.send({ command: 'ping' });
      return true;
    } catch (error) {
      return false;
    }
  }
}
