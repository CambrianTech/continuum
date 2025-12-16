/**
 * LoggerDaemonServer - Rust-backed daemon for high-performance logging
 *
 * RUST-BACKED DAEMON PATTERN:
 * This daemon establishes the pattern for TypeScript→Rust integration:
 *
 * 1. TypeScript (this file):
 *    - DaemonBase lifecycle integration
 *    - Connection management to Rust worker
 *    - Health checks and restart logic
 *    - Thin wrapper with minimal logic
 *
 * 2. Rust worker (workers/logger/):
 *    - Multi-threaded processing
 *    - Batching and buffering
 *    - File I/O and flushing
 *    - All performance-critical code
 *
 * 3. Communication:
 *    - Unix domain socket (/tmp/jtag-logger-worker.sock)
 *    - JSON messages over socket
 *    - Non-blocking async I/O
 *
 * Benefits:
 * - TypeScript: Easy to write, good for orchestration
 * - Rust: Fast, memory-safe, handles heavy lifting
 * - Clean separation of concerns
 * - Easy to test each side independently
 */

import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { LoggerDaemon } from '../shared/LoggerDaemon';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient';

/**
 * LoggerDaemonServer - manages Rust logger worker connection
 */
export class LoggerDaemonServer extends LoggerDaemon {
  protected log: ComponentLogger;
  private workerClient: LoggerWorkerClient | null = null;
  private readonly SOCKET_PATH = '/tmp/jtag-logger-worker.sock';
  private healthCheckInterval: NodeJS.Timeout | null = null;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging
    // NOTE: This uses Logger.ts which connects to the Rust worker we're managing
    // During initialization, Logger.ts will fall back to TypeScript logging if worker not ready yet
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Initialize daemon - connect to Rust worker
   *
   * NOTE: We assume the Rust worker is already started by npm run worker:start.
   * This daemon just manages the connection, not the process itself.
   * Future: Could add process management here to auto-start/restart worker.
   */
  protected async initialize(): Promise<void> {
    this.log.info('🦀 Initializing Rust logger worker connection');

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

      // Don't fail initialization - Logger.ts will fall back to TypeScript logging
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
    this.healthCheckInterval = setInterval(() => {
      if (this.workerClient) {
        // Future: implement ping/pong health check
        // For now, connection errors will be caught when logs are sent
      }
    }, 30000);
  }

  /**
   * Handle incoming messages
   *
   * Currently, LoggerDaemon doesn't handle direct commands.
   * Logging happens via Logger.ts which connects to Rust worker directly.
   * This could be extended to support commands like:
   * - logger/flush - force flush all buffers
   * - logger/rotate - rotate log files
   * - logger/stats - get logging statistics
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const sessionId = message.payload.sessionId;

    this.log.debug('LoggerDaemon received message');

    // For now, no commands supported - logging happens via Logger.ts
    return createBaseResponse(false, this.context, sessionId, {
      message: 'LoggerDaemon does not handle commands yet. Use Logger.ts for logging.'
    });
  }

  /**
   * Cleanup daemon - disconnect from Rust worker
   */
  async cleanup(): Promise<void> {
    this.log.info('🛑 Shutting down LoggerDaemon');

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

    this.log.info('✅ LoggerDaemon shutdown complete');
  }
}
