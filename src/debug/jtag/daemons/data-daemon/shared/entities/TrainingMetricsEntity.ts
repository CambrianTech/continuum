/**
 * Training Metrics Entity
 *
 * Tracks time-series metrics during training (loss, learning rate, throughput, etc.).
 * Multiple metrics entries belong to one TrainingSessionEntity, providing detailed
 * progress tracking and visualization data.
 *
 * Integration:
 * - Created automatically during training at configurable intervals
 * - Queried by training/status for real-time progress
 * - Used by training/metrics for historical analysis
 * - Visualized in UI for training progress charts
 *
 * Purpose:
 * - Real-time training progress monitoring
 * - Historical performance analysis
 * - Identify training issues (divergence, overfitting)
 * - Compare hyperparameter configurations
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  NumberField
} from '../../../../system/data/decorators/FieldDecorators';

/**
 * Training Metrics Entity
 *
 * Time-series data point capturing training metrics at a specific step.
 * Stored frequently (e.g., every 10 steps) for granular progress tracking.
 *
 * @example
 * ```typescript
 * // Create metrics entry during training
 * const metrics = TrainingMetricsEntity.create({
 *   sessionId: session.id,
 *   step: 150,
 *   epoch: 2,
 *   trainingLoss: 0.42,
 *   validationLoss: 0.48,
 *   learningRate: 0.0001,
 *   tokensPerSecond: 1250,
 *   memoryUsedMB: 3200,
 *   gradNorm: 0.85
 * });
 *
 * // Query metrics for charting
 * const allMetrics = await Commands.execute('data/list', {
 *   collection: 'training_metrics',
 *   filter: { sessionId: session.id },
 *   orderBy: [{ field: 'step', direction: 'asc' }]
 * });
 *
 * // Plot loss curve
 * const lossData = allMetrics.data.map(m => ({ step: m.step, loss: m.trainingLoss }));
 * ```
 */
export class TrainingMetricsEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'training_metrics';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return TrainingMetricsEntity.collection;
  }

  /**
   * Training session this metrics entry belongs to
   */
  @TextField()
  sessionId!: UUID;

  /**
   * Training step number (e.g., 10, 20, 30)
   */
  @NumberField()
  step!: number;

  /**
   * Epoch number (e.g., 1, 2, 3)
   */
  @NumberField()
  epoch!: number;

  /**
   * Training loss at this step
   */
  @NumberField()
  trainingLoss!: number;

  /**
   * Validation loss (null if no validation set or not computed this step)
   */
  @NumberField({ nullable: true })
  validationLoss!: number | null;

  /**
   * Current learning rate (may change with scheduling)
   */
  @NumberField()
  learningRate!: number;

  /**
   * Training throughput in tokens per second
   */
  @NumberField({ nullable: true })
  tokensPerSecond!: number | null;

  /**
   * Memory usage in megabytes (GPU or CPU depending on device)
   */
  @NumberField({ nullable: true })
  memoryUsedMB!: number | null;

  /**
   * Gradient norm (useful for detecting exploding/vanishing gradients)
   */
  @NumberField({ nullable: true })
  gradNorm!: number | null;

  /**
   * Elapsed time since training started (milliseconds)
   */
  @NumberField()
  elapsedMs!: number;

  /**
   * Estimated time remaining (milliseconds, null if cannot estimate)
   */
  @NumberField({ nullable: true })
  estimatedRemainingMs!: number | null;

  /**
   * Metadata for extensibility (additional metrics, provider-specific data)
   */
  @JsonField()
  metadata!: Record<string, unknown>;

  /**
   * Validate entity data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.sessionId) {
      return { success: false, error: 'sessionId is required' };
    }
    if (typeof this.step !== 'number' || this.step < 0) {
      return { success: false, error: 'step must be non-negative number' };
    }
    if (typeof this.epoch !== 'number' || this.epoch < 0) {
      return { success: false, error: 'epoch must be non-negative number' };
    }
    if (typeof this.trainingLoss !== 'number' || this.trainingLoss < 0) {
      return { success: false, error: 'trainingLoss must be non-negative number' };
    }
    if (typeof this.learningRate !== 'number' || this.learningRate <= 0) {
      return { success: false, error: 'learningRate must be positive number' };
    }
    if (typeof this.elapsedMs !== 'number' || this.elapsedMs < 0) {
      return { success: false, error: 'elapsedMs must be non-negative number' };
    }

    // Validate optional numeric fields
    if (this.validationLoss !== null && (typeof this.validationLoss !== 'number' || this.validationLoss < 0)) {
      return { success: false, error: 'validationLoss must be non-negative number or null' };
    }
    if (this.tokensPerSecond !== null && (typeof this.tokensPerSecond !== 'number' || this.tokensPerSecond < 0)) {
      return { success: false, error: 'tokensPerSecond must be non-negative number or null' };
    }
    if (this.memoryUsedMB !== null && (typeof this.memoryUsedMB !== 'number' || this.memoryUsedMB < 0)) {
      return { success: false, error: 'memoryUsedMB must be non-negative number or null' };
    }
    if (this.gradNorm !== null && (typeof this.gradNorm !== 'number' || this.gradNorm < 0)) {
      return { success: false, error: 'gradNorm must be non-negative number or null' };
    }
    if (this.estimatedRemainingMs !== null && (typeof this.estimatedRemainingMs !== 'number' || this.estimatedRemainingMs < 0)) {
      return { success: false, error: 'estimatedRemainingMs must be non-negative number or null' };
    }

    // Initialize metadata if not set
    if (!this.metadata) {
      this.metadata = {};
    }

    return { success: true };
  }

  /**
   * Get human-readable elapsed time
   *
   * @returns Time string like "2h 15m 30s"
   */
  getHumanReadableElapsed(): string {
    return this.formatDuration(this.elapsedMs);
  }

  /**
   * Get human-readable estimated remaining time
   *
   * @returns Time string like "30m 45s" or "Unknown"
   */
  getHumanReadableRemaining(): string {
    if (this.estimatedRemainingMs === null) {
      return 'Unknown';
    }
    return this.formatDuration(this.estimatedRemainingMs);
  }

  /**
   * Format duration in milliseconds to human-readable string
   *
   * @param ms - Duration in milliseconds
   * @returns Formatted string
   */
  private formatDuration(ms: number): string {
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) {
      return `${days}d ${hours % 24}h ${minutes % 60}m`;
    }
    if (hours > 0) {
      return `${hours}h ${minutes % 60}m ${seconds % 60}s`;
    }
    if (minutes > 0) {
      return `${minutes}m ${seconds % 60}s`;
    }
    return `${seconds}s`;
  }

  /**
   * Get throughput display string
   *
   * @returns String like "1250 tok/s" or "N/A"
   */
  getThroughputDisplay(): string {
    if (this.tokensPerSecond === null) {
      return 'N/A';
    }
    return `${this.tokensPerSecond.toFixed(0)} tok/s`;
  }

  /**
   * Get memory usage display string
   *
   * @returns String like "3.2 GB" or "N/A"
   */
  getMemoryDisplay(): string {
    if (this.memoryUsedMB === null) {
      return 'N/A';
    }
    if (this.memoryUsedMB > 1024) {
      return `${(this.memoryUsedMB / 1024).toFixed(2)} GB`;
    }
    return `${this.memoryUsedMB.toFixed(0)} MB`;
  }

  /**
   * Check if gradient norm indicates potential training issue
   *
   * @returns True if gradNorm suggests exploding/vanishing gradients
   */
  hasGradientIssue(): boolean {
    if (this.gradNorm === null) {
      return false;
    }
    // Typical healthy range is 0.1 to 10.0
    return this.gradNorm < 0.01 || this.gradNorm > 100.0;
  }

  /**
   * Get metrics summary for logging
   */
  getSummary(): string {
    const valLossStr = this.validationLoss !== null
      ? `, val_loss=${this.validationLoss.toFixed(4)}`
      : '';
    const throughputStr = this.tokensPerSecond !== null
      ? `, ${this.getThroughputDisplay()}`
      : '';
    return `Step ${this.step} (epoch ${this.epoch}): loss=${this.trainingLoss.toFixed(4)}${valLossStr}, lr=${this.learningRate}${throughputStr}`;
  }

  /**
   * Calculate validation loss increase (compared to training loss)
   *
   * @returns Difference as percentage, or null if no validation loss
   */
  getValidationGap(): number | null {
    if (this.validationLoss === null) {
      return null;
    }
    return ((this.validationLoss - this.trainingLoss) / this.trainingLoss) * 100;
  }

  /**
   * Check if model might be overfitting (validation loss >> training loss)
   *
   * @param thresholdPercent - Percentage gap to consider overfitting (default: 20%)
   * @returns True if validation gap exceeds threshold
   */
  isOverfitting(thresholdPercent: number = 20): boolean {
    const gap = this.getValidationGap();
    return gap !== null && gap > thresholdPercent;
  }
}
