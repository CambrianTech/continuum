/**
 * Logger Worker - Worker-Specific Message Types
 *
 * This demonstrates the pattern: IPC layer is generic, workers own their schemas.
 * These types are used as payload types with WorkerRequest<T>/WorkerResponse<T>.
 *
 * USAGE:
 * const request: WorkerRequest<WriteLogPayload> = {
 *   id: uuid(),
 *   type: 'write-log',
 *   timestamp: new Date().toISOString(),
 *   userId: 'user-123',
 *   payload: {
 *     category: 'sql',
 *     level: 'info',
 *     component: 'DataDaemon',
 *     message: 'Query executed'
 *   }
 * };
 */

// ============================================================================
// Logger-Specific Types
// ============================================================================

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/**
 * Payload for write-log requests.
 */
export interface WriteLogPayload {
  category: string;        // e.g., 'sql', 'daemons/UserDaemonServer'
  level: LogLevel;
  component: string;       // e.g., 'PersonaUser', 'DataDaemonServer'
  message: string;
  args?: unknown[];        // Additional arguments to log
}

/**
 * Payload for write-log responses.
 */
export interface WriteLogResult {
  bytesWritten: number;
}

/**
 * Payload for flush-logs requests.
 */
export interface FlushLogsPayload {
  category?: string;       // Optional: flush specific category, or all if undefined
}

/**
 * Payload for flush-logs responses.
 */
export interface FlushLogsResult {
  categoriesFlushed: string[];
  totalBytesWritten: number;
}

/**
 * Payload for ping requests (health check).
 * Empty payload - just proves worker is alive.
 */
export interface PingPayload {}

/**
 * Payload for ping responses (health check).
 * Includes uptime and connection statistics.
 */
export interface PingResult {
  uptimeMs: number;
  connectionsTotal: number;
  requestsProcessed: number;
  activeCategories: number;
}
