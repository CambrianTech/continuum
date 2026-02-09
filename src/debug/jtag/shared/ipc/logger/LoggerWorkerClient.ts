/**
 * LoggerWorkerClient - Type-Safe Client for Logger Module in continuum-core
 *
 * This provides a production-ready interface for sending log messages to the
 * LoggerModule (part of continuum-core since Phase 4a). It extends the generic
 * WorkerClient with logger-specific methods and types.
 *
 * USAGE:
 * ```typescript
 * const logger = new LoggerWorkerClient('/tmp/continuum-core.sock');
 * await logger.connect();
 *
 * // Write a log message (type-safe)
 * await logger.writeLog({
 *   category: 'daemons/UserDaemonServer',
 *   level: 'info',
 *   component: 'PersonaUser',
 *   message: 'User logged in successfully',
 *   args: [userId, timestamp]
 * });
 *
 * // Ping the logger module
 * const stats = await logger.ping();
 * ```
 *
 * NOTE: LoggerModule uses command prefix 'log/' (e.g., log/write, log/ping).
 */

import { WorkerClient, WorkerClientConfig } from '../WorkerClient.js';
import {
  WriteLogPayload,
  WriteLogResult,
  FlushLogsPayload,
  FlushLogsResult,
  PingPayload,
  PingResult,
  LogLevel
} from './LoggerMessageTypes.js';

// ============================================================================
// LoggerWorkerClient Class
// ============================================================================

/**
 * Type-safe client for Logger Rust worker.
 */
export class LoggerWorkerClient extends WorkerClient<
  WriteLogPayload | FlushLogsPayload | PingPayload,
  WriteLogResult | FlushLogsResult | PingResult
> {
  constructor(config: WorkerClientConfig | string) {
    // Allow simple socket path string or full config
    const fullConfig: WorkerClientConfig =
      typeof config === 'string'
        ? { socketPath: config }
        : config;

    super(fullConfig);
  }

  // ============================================================================
  // Type-Safe Log Writing Methods
  // ============================================================================

  /**
   * Write a log message to the Rust worker.
   *
   * @param payload - Log message details
   * @param userId - Optional userId context
   * @returns Promise resolving to write result with bytes written
   * @throws {WorkerError} if write fails
   */
  async writeLog(
    payload: WriteLogPayload,
    userId?: string
  ): Promise<WriteLogResult> {
    // LoggerModule uses 'log/write' command (Phase 4a unified runtime)
    const response = await this.send('log/write', payload, userId);
    return response.payload as WriteLogResult;
  }

  /**
   * Convenience method: Write a debug log.
   */
  async debug(
    category: string,
    component: string,
    message: string,
    args?: unknown[]
  ): Promise<WriteLogResult> {
    return this.writeLog({
      category,
      level: 'debug',
      component,
      message,
      args
    });
  }

  /**
   * Convenience method: Write an info log.
   */
  async info(
    category: string,
    component: string,
    message: string,
    args?: unknown[]
  ): Promise<WriteLogResult> {
    return this.writeLog({
      category,
      level: 'info',
      component,
      message,
      args
    });
  }

  /**
   * Convenience method: Write a warning log.
   */
  async warn(
    category: string,
    component: string,
    message: string,
    args?: unknown[]
  ): Promise<WriteLogResult> {
    return this.writeLog({
      category,
      level: 'warn',
      component,
      message,
      args
    });
  }

  /**
   * Convenience method: Write an error log.
   */
  async error(
    category: string,
    component: string,
    message: string,
    args?: unknown[]
  ): Promise<WriteLogResult> {
    return this.writeLog({
      category,
      level: 'error',
      component,
      message,
      args
    });
  }

  // ============================================================================
  // Flush Operations
  // ============================================================================

  /**
   * Flush log buffers to disk.
   *
   * @param category - Optional category to flush (undefined flushes all)
   * @param userId - Optional userId context
   * @returns Promise resolving to flush result with categories flushed
   * @throws {WorkerError} if flush fails
   */
  async flushLogs(
    category?: string,
    userId?: string
  ): Promise<FlushLogsResult> {
    // NOTE: log/flush not yet implemented in LoggerModule - will return error
    const payload: FlushLogsPayload = category ? { category } : {};
    const response = await this.send('log/flush', payload, userId);
    return response.payload as FlushLogsResult;
  }

  /**
   * Convenience method: Flush all logs.
   */
  async flushAll(): Promise<FlushLogsResult> {
    return this.flushLogs();
  }

  // ============================================================================
  // Health Check Operations
  // ============================================================================

  /**
   * Ping the worker to check if it's alive and responsive.
   *
   * This sends a lightweight health check request to the worker and returns
   * statistics about uptime, connections, requests processed, and active categories.
   *
   * @returns Promise resolving to ping result with worker health stats
   * @throws {WorkerError} if worker is frozen or unresponsive
   */
  async ping(): Promise<PingResult> {
    // LoggerModule uses 'log/ping' command (Phase 4a unified runtime)
    const response = await this.send('log/ping', {});
    return response.payload as PingResult;
  }

  // ============================================================================
  // Batch Operations (Future)
  // ============================================================================

  /**
   * Write multiple log messages in batch (future enhancement).
   *
   * NOTE: Currently sends messages individually. A future optimization would
   * be to add a 'write-logs-batch' message type to the Rust worker.
   *
   * @param payloads - Array of log messages to write
   * @returns Promise resolving to array of results
   */
  async writeLogsBatch(
    payloads: WriteLogPayload[]
  ): Promise<WriteLogResult[]> {
    const promises = payloads.map(payload => this.writeLog(payload));
    return Promise.all(promises);
  }
}

// ============================================================================
// Singleton Pattern (Optional)
// ============================================================================

/**
 * Shared singleton instance for application-wide use.
 * Call `LoggerWorkerClient.initialize()` once at startup.
 */
let sharedInstance: LoggerWorkerClient | null = null;

export namespace LoggerWorkerClient {
  /**
   * Initialize the shared logger worker client.
   *
   * @param config - Configuration for worker client
   * @returns The shared instance
   */
  export function initialize(config: WorkerClientConfig | string): LoggerWorkerClient {
    if (sharedInstance) {
      throw new Error('LoggerWorkerClient already initialized');
    }
    sharedInstance = new LoggerWorkerClient(config);
    return sharedInstance;
  }

  /**
   * Get the shared logger worker client instance.
   *
   * @throws {Error} if not initialized
   */
  export function getInstance(): LoggerWorkerClient {
    if (!sharedInstance) {
      throw new Error('LoggerWorkerClient not initialized. Call initialize() first.');
    }
    return sharedInstance;
  }

  /**
   * Check if shared instance is initialized.
   */
  export function isInitialized(): boolean {
    return sharedInstance !== null;
  }

  /**
   * Dispose of the shared instance (for testing).
   */
  export async function dispose(): Promise<void> {
    if (sharedInstance) {
      await sharedInstance.disconnect();
      sharedInstance = null;
    }
  }
}
