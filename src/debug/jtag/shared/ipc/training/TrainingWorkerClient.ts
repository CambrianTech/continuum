/**
 * TrainingWorkerClient - Type-Safe Client for Training Rust Worker
 *
 * This provides a production-ready interface for communicating with the
 * Rust training worker. It extends the generic WorkerClient with training-specific
 * methods and types.
 *
 * USAGE:
 * ```typescript
 * const trainer = new TrainingWorkerClient('/tmp/jtag-training-worker.sock');
 * await trainer.connect();
 *
 * // Export training data to JSONL (type-safe)
 * const result = await trainer.exportTraining({
 *   outputPath: './training.jsonl',
 *   limit: 1000,
 *   minQuality: 0.5,
 *   format: 'openai'
 * });
 *
 * console.log(`Exported ${result.examplesExported} examples in ${result.durationMs}ms`);
 * ```
 *
 * NOTE: This will be used by TrainingDaemon for high-performance export operations.
 */

import { WorkerClient, WorkerClientConfig } from '../WorkerClient.js';
import {
  ExportTrainingPayload,
  ExportTrainingResult,
  PingPayload,
  PingResult
} from './TrainingMessageTypes.js';

// ============================================================================
// TrainingWorkerClient Class
// ============================================================================

/**
 * Type-safe client for Training Rust worker.
 */
export class TrainingWorkerClient extends WorkerClient<
  ExportTrainingPayload | PingPayload,
  ExportTrainingResult | PingResult
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
  // Type-Safe Training Export Methods
  // ============================================================================

  /**
   * Export training data to JSONL format.
   *
   * @param payload - Export configuration (output path, filters, format)
   * @param userId - Optional userId context
   * @returns Promise resolving to export result with stats
   * @throws {WorkerError} if export fails
   */
  async exportTraining(
    payload: ExportTrainingPayload,
    userId?: string
  ): Promise<ExportTrainingResult> {
    const response = await this.send('export-training', payload, userId);
    return response.payload as ExportTrainingResult;
  }

  /**
   * Convenience method: Export all training data with defaults.
   */
  async exportAll(outputPath: string): Promise<ExportTrainingResult> {
    return this.exportTraining({
      outputPath,
      limit: 0,  // All examples
      minQuality: 0.0,  // All quality levels
      format: 'openai'
    });
  }

  /**
   * Convenience method: Export high-quality examples only.
   */
  async exportHighQuality(
    outputPath: string,
    minQuality: number = 0.7
  ): Promise<ExportTrainingResult> {
    return this.exportTraining({
      outputPath,
      limit: 0,
      minQuality,
      format: 'openai'
    });
  }

  /**
   * Convenience method: Export limited set for testing.
   */
  async exportSample(
    outputPath: string,
    limit: number = 10
  ): Promise<ExportTrainingResult> {
    return this.exportTraining({
      outputPath,
      limit,
      minQuality: 0.0,
      format: 'openai'
    });
  }

  // ============================================================================
  // Health Check Operations
  // ============================================================================

  /**
   * Ping the worker to check if it's alive and responsive.
   *
   * This sends a lightweight health check request to the worker and returns
   * statistics about uptime, connections, requests processed, and examples processed.
   *
   * @returns Promise resolving to ping result with worker health stats
   * @throws {WorkerError} if worker is frozen or unresponsive
   */
  async ping(): Promise<PingResult> {
    const response = await this.send('ping', {});
    return response.payload as PingResult;
  }
}

// ============================================================================
// Singleton Pattern (Optional)
// ============================================================================

/**
 * Shared singleton instance for application-wide use.
 * Call `TrainingWorkerClient.initialize()` once at startup.
 */
let sharedInstance: TrainingWorkerClient | null = null;

export namespace TrainingWorkerClient {
  /**
   * Initialize the shared training worker client.
   *
   * @param config - Configuration for worker client
   * @returns The shared instance
   */
  export function initialize(config: WorkerClientConfig | string): TrainingWorkerClient {
    if (sharedInstance) {
      throw new Error('TrainingWorkerClient already initialized');
    }
    sharedInstance = new TrainingWorkerClient(config);
    return sharedInstance;
  }

  /**
   * Get the shared training worker client instance.
   *
   * @throws {Error} if not initialized
   */
  export function getInstance(): TrainingWorkerClient {
    if (!sharedInstance) {
      throw new Error('TrainingWorkerClient not initialized. Call initialize() first.');
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
