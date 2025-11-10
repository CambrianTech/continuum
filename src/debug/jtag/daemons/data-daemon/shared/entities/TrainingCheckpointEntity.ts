/**
 * Training Checkpoint Entity
 *
 * Tracks individual checkpoints saved during a training session.
 * Checkpoints are snapshots of model state at specific training steps,
 * enabling resume-from-failure and intermediate evaluation.
 *
 * Integration:
 * - Created automatically during training at configurable intervals
 * - Used by training/status to report progress
 * - Used by adapter/deploy to select best checkpoint
 * - Queried by GenomeManager for adapter loading
 *
 * Purpose:
 * - Resume training from failure
 * - Evaluate intermediate model quality
 * - Select best checkpoint based on validation metrics
 * - Track training progression over time
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  NumberField,
  BooleanField
} from '../../../../system/data/decorators/FieldDecorators';

/**
 * Validation metrics for checkpoint evaluation
 *
 * Used to select the best checkpoint based on validation performance.
 */
export interface ValidationMetrics {
  readonly loss: number;
  readonly perplexity?: number;
  readonly accuracy?: number;
  readonly f1Score?: number;
  readonly [key: string]: number | undefined;
}

/**
 * Training Checkpoint Entity
 *
 * Represents a saved model state at a specific training step.
 * Multiple checkpoints belong to one TrainingSessionEntity.
 *
 * @example
 * ```typescript
 * // Create checkpoint during training
 * const checkpoint = TrainingCheckpointEntity.create({
 *   sessionId: session.id,
 *   step: 100,
 *   epoch: 1,
 *   checkpointPath: '/datasets/prepared/sessions/session-001/checkpoint-100',
 *   loss: 0.45,
 *   validationMetrics: {
 *     loss: 0.52,
 *     perplexity: 1.68
 *   },
 *   fileSize: 512 * 1024 * 1024, // 512MB
 *   isBest: false
 * });
 *
 * // Query best checkpoint for deployment
 * const bestCheckpoint = await Commands.execute('data/list', {
 *   collection: 'training_checkpoints',
 *   filter: {
 *     sessionId: session.id,
 *     isBest: true
 *   }
 * });
 * ```
 */
export class TrainingCheckpointEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'training_checkpoints';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return TrainingCheckpointEntity.collection;
  }

  /**
   * Training session this checkpoint belongs to
   */
  @TextField()
  sessionId!: UUID;

  /**
   * Training step number (e.g., 100, 200, 300)
   */
  @NumberField()
  step!: number;

  /**
   * Epoch number (e.g., 1, 2, 3)
   */
  @NumberField()
  epoch!: number;

  /**
   * Path to checkpoint directory/file
   *
   * Example: '/datasets/prepared/sessions/session-abc123/checkpoint-100'
   *
   * Contains:
   * - adapter_model.safetensors (LoRA weights)
   * - adapter_config.json (LoRA config)
   * - tokenizer files
   * - training_state.json (optimizer state, random seed)
   */
  @TextField()
  checkpointPath!: string;

  /**
   * Training loss at this step
   */
  @NumberField()
  loss!: number;

  /**
   * Validation metrics (if validation set used)
   */
  @JsonField({ nullable: true })
  validationMetrics!: ValidationMetrics | null;

  /**
   * Size of checkpoint on disk (bytes)
   */
  @NumberField()
  fileSize!: number;

  /**
   * Whether this is the best checkpoint (lowest validation loss)
   *
   * Used for automatic adapter deployment - deploy best checkpoint.
   */
  @BooleanField()
  isBest!: boolean;

  /**
   * Metadata for extensibility (hardware info, timing, etc.)
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
    if (!this.checkpointPath) {
      return { success: false, error: 'checkpointPath is required' };
    }
    if (typeof this.loss !== 'number' || this.loss < 0) {
      return { success: false, error: 'loss must be non-negative number' };
    }
    if (typeof this.fileSize !== 'number' || this.fileSize < 0) {
      return { success: false, error: 'fileSize must be non-negative number' };
    }
    if (typeof this.isBest !== 'boolean') {
      return { success: false, error: 'isBest must be boolean' };
    }

    // Validate validation metrics if present
    if (this.validationMetrics !== null) {
      if (typeof this.validationMetrics !== 'object') {
        return { success: false, error: 'validationMetrics must be object or null' };
      }
      if (typeof this.validationMetrics.loss !== 'number' || this.validationMetrics.loss < 0) {
        return { success: false, error: 'validationMetrics.loss must be non-negative number' };
      }
    }

    // Initialize metadata if not set
    if (!this.metadata) {
      this.metadata = {};
    }

    return { success: true };
  }

  /**
   * Compare checkpoints by validation loss (lower is better)
   *
   * @param other - Checkpoint to compare against
   * @returns Negative if this is better, positive if other is better, 0 if equal
   */
  compareValidationLoss(other: TrainingCheckpointEntity): number {
    const thisLoss = this.validationMetrics?.loss ?? this.loss;
    const otherLoss = other.validationMetrics?.loss ?? other.loss;
    return thisLoss - otherLoss;
  }

  /**
   * Check if this checkpoint has validation metrics
   */
  hasValidationMetrics(): boolean {
    return this.validationMetrics !== null;
  }

  /**
   * Get validation loss (falls back to training loss if no validation)
   */
  getValidationLoss(): number {
    return this.validationMetrics?.loss ?? this.loss;
  }

  /**
   * Mark this checkpoint as best (and unmark others)
   *
   * NOTE: Caller is responsible for unmarking other checkpoints in the session.
   * This just sets isBest=true on this checkpoint.
   */
  markAsBest(): void {
    this.isBest = true;
  }

  /**
   * Unmark this checkpoint as best
   */
  unmarkAsBest(): void {
    this.isBest = false;
  }

  /**
   * Get human-readable size
   *
   * @returns Size string like "512 MB" or "1.2 GB"
   */
  getHumanReadableSize(): string {
    const bytes = this.fileSize;
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  /**
   * Get checkpoint summary for logging
   */
  getSummary(): string {
    const valLossStr = this.validationMetrics
      ? `, val_loss=${this.validationMetrics.loss.toFixed(4)}`
      : '';
    const bestStr = this.isBest ? ' [BEST]' : '';
    return `Step ${this.step} (epoch ${this.epoch}): loss=${this.loss.toFixed(4)}${valLossStr}${bestStr}`;
  }
}
