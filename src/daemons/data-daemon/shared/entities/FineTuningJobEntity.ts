/**
 * Fine-Tuning Job Entity
 *
 * Tracks fine-tuning job lifecycle from creation through completion.
 * Each job represents one training run with specific dataset and hyperparameters.
 *
 * Lifecycle:
 * 1. 'validating_files' - Provider is validating training/validation files
 * 2. 'queued' - Job accepted, waiting for resources
 * 3. 'running' - Training in progress
 * 4. 'succeeded' - Training completed successfully
 * 5. 'failed' - Training failed with errors
 * 6. 'cancelled' - User or system cancelled training
 *
 * Integration:
 * - Created by genome/job-create command
 * - Updated by genome/job-status command (polls provider API)
 * - Referenced by FineTunedModelEntity.jobId
 * - Queried for live metrics/charts by UI widgets
 *
 * Persistence:
 * - Survives server restarts
 * - Metrics and events continuously sync with provider
 * - entity.id serves as the handle for all job operations
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  EnumField,
  NumberField,
  ForeignKeyField
} from '../../../../system/data/decorators/FieldDecorators';

// Import types from the single source of truth
import type {
  JobConfiguration,
  JobStatus,
  EventLevel,
  MetricPoint,
  JobMetrics,
  JobEvent,
  JobError
} from './FineTuningTypes';

// Re-export types for backward compatibility
export type {
  JobStatus,
  EventLevel,
  MetricPoint,
  JobMetrics,
  JobEvent,
  JobError
};

/**
 * @deprecated Use JobConfiguration instead. This interface is kept for backward compatibility only.
 *
 * Hyperparameters for fine-tuning job (LEGACY)
 *
 * Provider-agnostic core parameters. Additional provider-specific
 * parameters can be stored in metadata field.
 */
export interface JobHyperparameters {
  readonly n_epochs?: number;                  // Number of training epochs (default varies by provider)
  readonly batch_size?: number;                // Training batch size (default: 'auto' or 4)
  readonly learning_rate_multiplier?: number;  // LR multiplier (default: 'auto' or 1.0)
  readonly warmup_ratio?: number;              // Warmup steps ratio (optional)
  readonly weight_decay?: number;              // Weight decay for regularization (optional)
}

/**
 * Fine-Tuning Job Entity
 *
 * Comprehensive job tracking with metrics, events, and status sync.
 *
 * @example
 * ```typescript
 * // Create new job entity (Phase 1: Using new JobConfiguration)
 * const job = FineTuningJobEntity.create({
 *   personaId: 'helper-ai-uuid',
 *   provider: 'openai',
 *   providerJobId: 'ftjob-abc123',
 *   baseModel: 'gpt-4o-mini-2024-07-18',
 *   trainingFileId: 'dataset-uuid-001',
 *   configuration: {
 *     model: {
 *       baseModel: 'gpt-4o-mini-2024-07-18',
 *       precision: ModelPrecision.FP16
 *     },
 *     datasets: {
 *       trainingFileId: 'dataset-uuid-001',
 *       validationFileId: null
 *     },
 *     method: {
 *       type: TrainingMethod.LORA,
 *       loraConfig: {
 *         rank: 8,
 *         alpha: 16,
 *         dropout: 0,
 *         trainableModules: 'all-linear'
 *       }
 *     },
 *     schedule: {
 *       epochs: 3,
 *       batchSize: 4,
 *       sequenceLength: 2048,
 *       gradientAccumulation: 1,
 *       checkpoints: 1,
 *       evaluations: 1,
 *       trainOnInputs: TrainOnInputs.AUTO
 *     },
 *     optimizer: {
 *       learningRate: 0.00001,
 *       scheduler: {
 *         type: LRSchedulerType.COSINE,
 *         minLRRatio: 0,
 *         warmupRatio: 0.03
 *       },
 *       weightDecay: 0,
 *       maxGradientNorm: 1
 *     },
 *     optimizations: {
 *       enabled: [OptimizationFeature.FLASH_ATTENTION]
 *     },
 *     output: {
 *       suffix: 'v1'
 *     },
 *     metadata: {}
 *   },
 *   status: 'queued',
 *   metrics: { loss: [], accuracy: [] },
 *   events: []
 * });
 *
 * // Update with new metrics
 * job.addMetric('loss', { step: 100, value: 0.45, timestamp: Date.now() });
 * job.addEvent({
 *   message: 'Training started',
 *   level: 'info',
 *   createdAt: Date.now()
 * });
 * ```
 */
export class FineTuningJobEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'fine_tuning_jobs';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return FineTuningJobEntity.collection;
  }

  /**
   * PersonaUser that owns this job
   */
  @ForeignKeyField({ references: 'users' })
  personaId!: UUID;

  /**
   * Provider name (e.g., 'openai', 'fireworks', 'mistral', 'together')
   */
  @TextField()
  provider!: string;

  /**
   * Provider-specific job ID (e.g., OpenAI's 'ftjob-abc123')
   * Used to query status from provider API
   */
  @TextField()
  providerJobId!: string;

  /**
   * Base model identifier (e.g., 'gpt-4o-mini-2024-07-18', 'accounts/fireworks/models/llama-v3p1-8b-instruct')
   */
  @TextField()
  baseModel!: string;

  /**
   * Training dataset entity ID (references FineTuningDatasetEntity.id)
   */
  @ForeignKeyField({ references: 'fine_tuning_datasets' })
  trainingFileId!: UUID;

  /**
   * Validation dataset entity ID (optional, references FineTuningDatasetEntity.id)
   */
  @ForeignKeyField({ references: 'fine_tuning_datasets', nullable: true })
  validationFileId!: UUID | null;

  /**
   * @deprecated Use 'configuration' field instead. This field is kept for backward compatibility only.
   *
   * Hyperparameters for this training run (LEGACY)
   */
  @JsonField({ nullable: true })
  hyperparameters!: JobHyperparameters | null;

  /**
   * Complete job configuration (NEW - Phase 1)
   *
   * Comprehensive configuration covering all aspects of the fine-tuning job:
   * - Model selection (base model, precision)
   * - Dataset configuration (training + validation files)
   * - Training method (full/LoRA/QLoRA with LoRA config)
   * - Training schedule (epochs, batch size, sequence length, etc.)
   * - Optimization (learning rate, scheduler, weight decay, gradient clipping)
   * - Memory optimizations (Flash Attention, Gradient Checkpointing, etc.)
   * - Hardware configuration (GPU type, count)
   * - External integrations (W&B, Hugging Face)
   * - Output configuration (model suffix)
   * - Provider-specific overrides (metadata)
   *
   * This field replaces the legacy 'hyperparameters' field with a comprehensive,
   * provider-agnostic schema based on the union of all provider capabilities.
   */
  @JsonField({ nullable: true })
  configuration!: JobConfiguration | null;

  /**
   * Model name suffix (optional, e.g., 'coding-expert-v1')
   * Final model name will be baseModel + suffix
   */
  @TextField({ nullable: true })
  suffix!: string | null;

  /**
   * Current status of the job
   */
  @EnumField()
  status!: JobStatus;

  /**
   * When the job was created from provider (Unix timestamp)
   * Note: This is separate from BaseEntity.createdAt which tracks database insertion
   */
  @NumberField()
  providerCreatedAt!: number;

  /**
   * When training started (null if not started yet)
   */
  @NumberField({ nullable: true })
  startedAt!: number | null;

  /**
   * When training finished/failed (null if still running)
   */
  @NumberField({ nullable: true })
  finishedAt!: number | null;

  /**
   * Fine-tuned model ID from provider (e.g., 'ft:gpt-4o-mini:org-id:suffix:job-id')
   * Null until job succeeds
   */
  @TextField({ nullable: true })
  fineTunedModel!: string | null;

  /**
   * Total tokens trained on (null until job completes)
   */
  @NumberField({ nullable: true })
  trainedTokens!: number | null;

  /**
   * Error information (null unless status === 'failed')
   */
  @JsonField({ nullable: true })
  error!: JobError | null;

  /**
   * Training metrics (loss, accuracy curves)
   * Updated continuously during training
   */
  @JsonField()
  metrics!: JobMetrics;

  /**
   * Job events (activity log)
   * Events added continuously during training
   */
  @JsonField()
  events!: JobEvent[];

  /**
   * Metadata for extensibility (custom fields, provider-specific data)
   */
  @JsonField()
  metadata!: Record<string, unknown>;

  /**
   * Validate entity data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.personaId) {
      return { success: false, error: 'personaId is required' };
    }
    if (!this.provider) {
      return { success: false, error: 'provider is required' };
    }
    if (!this.providerJobId) {
      return { success: false, error: 'providerJobId is required' };
    }
    if (!this.baseModel) {
      return { success: false, error: 'baseModel is required' };
    }
    if (!this.trainingFileId) {
      return { success: false, error: 'trainingFileId is required' };
    }

    // Either hyperparameters (legacy) or configuration (new) must be present
    if (!this.hyperparameters && !this.configuration) {
      return { success: false, error: 'Either hyperparameters (legacy) or configuration is required' };
    }

    if (!this.status) {
      return { success: false, error: 'status is required' };
    }
    if (typeof this.providerCreatedAt !== 'number' || this.providerCreatedAt <= 0) {
      return { success: false, error: 'providerCreatedAt must be positive number' };
    }

    // Validate status enum
    const validStatuses: JobStatus[] = [
      'validating_files',
      'queued',
      'running',
      'succeeded',
      'failed',
      'cancelled'
    ];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Invalid status: ${this.status}` };
    }

    // Initialize metrics if not set
    if (!this.metrics) {
      this.metrics = {};
    }

    // Initialize events if not set
    if (!this.events) {
      this.events = [];
    }

    // Initialize metadata if not set
    if (!this.metadata) {
      this.metadata = {};
    }

    return { success: true };
  }

  /**
   * Check if job is in terminal state (succeeded/failed/cancelled)
   */
  isTerminal(): boolean {
    return this.status === 'succeeded' ||
           this.status === 'failed' ||
           this.status === 'cancelled';
  }

  /**
   * Check if job is active (validating/queued/running)
   */
  isActive(): boolean {
    return this.status === 'validating_files' ||
           this.status === 'queued' ||
           this.status === 'running';
  }

  /**
   * Check if job succeeded
   */
  isSucceeded(): boolean {
    return this.status === 'succeeded';
  }

  /**
   * Check if job failed
   */
  isFailed(): boolean {
    return this.status === 'failed';
  }

  /**
   * Get training duration in milliseconds (null if not started or still running)
   */
  getDuration(): number | null {
    if (!this.startedAt || !this.finishedAt) {
      return null;
    }
    return this.finishedAt - this.startedAt;
  }

  /**
   * Get latest loss value (null if no metrics yet)
   */
  getLatestLoss(): number | null {
    if (!this.metrics.loss || this.metrics.loss.length === 0) {
      return null;
    }
    return this.metrics.loss[this.metrics.loss.length - 1].value;
  }

  /**
   * Get latest accuracy value (null if not available)
   */
  getLatestAccuracy(): number | null {
    if (!this.metrics.accuracy || this.metrics.accuracy.length === 0) {
      return null;
    }
    return this.metrics.accuracy[this.metrics.accuracy.length - 1].value;
  }

  /**
   * Add a metric data point
   *
   * @param metricName - Name of metric ('loss', 'accuracy', 'valLoss', 'valAccuracy')
   * @param point - Metric data point
   */
  addMetric(metricName: keyof JobMetrics, point: MetricPoint): void {
    const array = this.metrics[metricName];
    if (!array || !Array.isArray(array)) {
      // TypeScript requires explicit property assignment for JobMetrics
      if (metricName === 'loss') {
        this.metrics.loss = [point];
      } else if (metricName === 'accuracy') {
        this.metrics.accuracy = [point];
      } else if (metricName === 'valLoss') {
        this.metrics.valLoss = [point];
      } else if (metricName === 'valAccuracy') {
        this.metrics.valAccuracy = [point];
      }
    } else {
      array.push(point);
    }
  }

  /**
   * Add a job event
   *
   * @param event - Event to add
   */
  addEvent(event: JobEvent): void {
    this.events.push(event);
  }

  /**
   * Mark job as started
   */
  markStarted(): void {
    this.status = 'running';
    this.startedAt = Date.now();
    this.addEvent({
      message: 'Training started',
      level: 'info',
      createdAt: Date.now()
    });
  }

  /**
   * Mark job as succeeded
   *
   * @param fineTunedModel - Provider's model ID for the fine-tuned model
   * @param trainedTokens - Total tokens trained on
   */
  markSucceeded(fineTunedModel: string, trainedTokens: number): void {
    this.status = 'succeeded';
    this.finishedAt = Date.now();
    this.fineTunedModel = fineTunedModel;
    this.trainedTokens = trainedTokens;
    this.error = null;
    this.addEvent({
      message: `Training succeeded. Model: ${fineTunedModel}`,
      level: 'info',
      createdAt: Date.now()
    });
  }

  /**
   * Mark job as failed
   *
   * @param error - Error that caused failure
   */
  markFailed(error: JobError): void {
    this.status = 'failed';
    this.finishedAt = Date.now();
    this.error = error;
    this.addEvent({
      message: `Training failed: ${error.message}`,
      level: 'error',
      createdAt: Date.now()
    });
  }

  /**
   * Mark job as cancelled
   */
  markCancelled(): void {
    this.status = 'cancelled';
    this.finishedAt = Date.now();
    this.addEvent({
      message: 'Training cancelled',
      level: 'warning',
      createdAt: Date.now()
    });
  }

  /**
   * Update job status from provider API response
   *
   * @param newStatus - New status from provider
   */
  updateStatus(newStatus: JobStatus): void {
    const oldStatus = this.status;
    this.status = newStatus;

    // Auto-set startedAt if transitioning to running
    if (newStatus === 'running' && !this.startedAt) {
      this.startedAt = Date.now();
    }

    // Auto-set finishedAt if transitioning to terminal state
    if (this.isTerminal() && !this.finishedAt) {
      this.finishedAt = Date.now();
    }

    // Log status change
    if (oldStatus !== newStatus) {
      this.addEvent({
        message: `Status changed: ${oldStatus} → ${newStatus}`,
        level: 'info',
        createdAt: Date.now()
      });
    }
  }
}
