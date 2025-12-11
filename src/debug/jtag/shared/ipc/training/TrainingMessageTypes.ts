/**
 * Training Worker - Worker-Specific Message Types
 *
 * These types mirror the Rust types in workers/training/src/messages.rs
 * and are used as payload types with JTAGRequest<T>/JTAGResponse<T>.
 *
 * USAGE:
 * const request: JTAGRequest<ExportTrainingPayload> = {
 *   id: uuid(),
 *   type: 'export-training',
 *   timestamp: new Date().toISOString(),
 *   userId: 'training-daemon',
 *   payload: {
 *     outputPath: './training.jsonl',
 *     limit: 1000,
 *     minQuality: 0.5,
 *     format: 'openai'
 *   }
 * };
 */

// ============================================================================
// Training-Specific Types
// ============================================================================

/**
 * Payload for export-training requests.
 */
export interface ExportTrainingPayload {
  /** Output file path for JSONL export */
  outputPath: string;

  /** Maximum number of examples to export (0 = all) */
  limit?: number;

  /** Minimum quality score threshold (0.0 - 1.0) */
  minQuality?: number;

  /** Export format: 'openai', 'llama', 'alpaca' */
  format?: string;
}

/**
 * Payload for export-training responses.
 */
export interface ExportTrainingResult {
  /** Number of examples exported */
  examplesExported: number;

  /** Total bytes written to file */
  bytesWritten: number;

  /** Average quality score of exported examples */
  averageQuality: number;

  /** Export duration in milliseconds */
  durationMs: number;
}

/**
 * Payload for ping requests (health check).
 * Empty payload - just proves worker is alive.
 */
export interface PingPayload {}

/**
 * Payload for ping responses (health check).
 * Includes uptime and statistics.
 */
export interface PingResult {
  uptimeMs: number;
  connectionsTotal: number;
  requestsProcessed: number;
  examplesProcessed: number;
}
