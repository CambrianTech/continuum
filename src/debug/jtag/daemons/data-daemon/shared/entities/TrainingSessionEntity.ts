/**
 * Training Session Entity
 *
 * Tracks an individual fine-tuning session from preparation through completion.
 * Each session represents one training run with specific hyperparameters,
 * dataset, and target model.
 *
 * Integration:
 * - Created by training/prepare command
 * - Updated by training/start, training/status commands
 * - Queried by PersonaUser for autonomous training triggers
 * - Used by GenomeManager for adapter deployment
 *
 * Lifecycle:
 * 1. 'pending' - Session prepared, not started
 * 2. 'running' - Training in progress
 * 3. 'completed' - Training finished successfully
 * 4. 'failed' - Training encountered error
 * 5. 'cancelled' - User/system cancelled training
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  EnumField,
  NumberField
} from '../../../../system/data/decorators/FieldDecorators';

export type TrainingStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
export type TrainingProvider = 'peft' | 'mlx' | 'openai' | 'deepseek' | 'anthropic';

/**
 * Hyperparameters for fine-tuning
 *
 * These are provider-agnostic defaults. Specific providers may support
 * additional parameters in the providerConfig field.
 */
export interface TrainingHyperparameters {
  readonly learningRate: number;          // Default: 1e-4
  readonly batchSize: number;             // Default: 4
  readonly epochs: number;                // Default: 3
  readonly rank: number;                  // LoRA rank (default: 8)
  readonly alpha: number;                 // LoRA alpha (default: 16)
  readonly targetModules: readonly string[];  // Layers to adapt (e.g., ['q_proj', 'v_proj'])
  readonly warmupSteps?: number;          // Optional warmup
  readonly weightDecay?: number;          // Optional regularization
  readonly maxGradNorm?: number;          // Gradient clipping
}

/**
 * Provider-specific configuration
 *
 * Each provider may require additional settings beyond standard hyperparameters.
 */
export interface ProviderConfig {
  readonly provider: TrainingProvider;
  readonly modelPath?: string;            // Local model path (MLX, PEFT)
  readonly apiKey?: string;               // API key (OpenAI, DeepSeek, Anthropic)
  readonly organizationId?: string;       // Org ID for managed services
  readonly customEndpoint?: string;       // Custom API endpoint
  readonly [key: string]: unknown;        // Provider-specific options
}

/**
 * Error information when training fails
 */
export interface TrainingError {
  readonly message: string;
  readonly code?: string;
  readonly stack?: string;
  readonly timestamp: number;
}

/**
 * Training Session Entity
 *
 * Core entity for tracking fine-tuning sessions. Each session represents
 * one complete training run from preparation to completion/failure.
 *
 * @example
 * ```typescript
 * // Create new training session
 * const session = TrainingSessionEntity.create({
 *   personaId: 'helper-ai-uuid',
 *   datasetHandle: 'training-db-handle',
 *   baseModel: 'qwen2.5:3b',
 *   hyperparameters: {
 *     learningRate: 1e-4,
 *     batchSize: 4,
 *     epochs: 3,
 *     rank: 8,
 *     alpha: 16,
 *     targetModules: ['q_proj', 'v_proj']
 *   },
 *   providerConfig: {
 *     provider: 'mlx',
 *     modelPath: '/models/qwen2.5-3b'
 *   },
 *   outputDir: '/datasets/prepared/sessions/session-001'
 * });
 *
 * // Update status during training
 * session.status = 'running';
 * session.startedAt = Date.now();
 *
 * // Mark completed
 * session.status = 'completed';
 * session.completedAt = Date.now();
 * session.finalCheckpointPath = '/datasets/prepared/sessions/session-001/checkpoint-300';
 * ```
 */
export class TrainingSessionEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'training_sessions';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return TrainingSessionEntity.collection;
  }

  /**
   * PersonaUser this training session belongs to
   */
  @TextField()
  personaId!: UUID;

  /**
   * DbHandle pointing to training dataset (SQLite with examples)
   *
   * Use this handle with data/list to fetch training examples:
   * ```typescript
   * const examples = await Commands.execute('data/list', {
   *   dbHandle: session.datasetHandle,
   *   collection: 'training_examples'
   * });
   * ```
   */
  @TextField()
  datasetHandle!: string;

  /**
   * Base model identifier (e.g., 'qwen2.5:3b', 'llama3.2:1b')
   */
  @TextField()
  baseModel!: string;

  /**
   * Provider name (e.g., 'openai', 'fireworks', 'mistral', 'together', 'ollama')
   */
  @TextField()
  provider!: string;

  /**
   * Provider-specific job ID (e.g., OpenAI fine-tune job ID, Fireworks job ID)
   * Used to query status from the provider API
   */
  @TextField()
  providerJobId!: string;

  /**
   * Hyperparameters for this training run
   */
  @JsonField()
  hyperparameters!: TrainingHyperparameters;

  /**
   * Provider-specific configuration (MLX, PEFT, OpenAI, etc.)
   */
  @JsonField()
  providerConfig!: ProviderConfig;

  /**
   * Output directory for checkpoints, logs, and final adapter
   *
   * Example: '/datasets/prepared/sessions/session-abc123'
   */
  @TextField()
  outputDir!: string;

  /**
   * Current status of training session
   */
  @EnumField()
  status!: TrainingStatus;

  /**
   * When training started (null if not yet started)
   */
  @NumberField({ nullable: true })
  startedAt!: number | null;

  /**
   * When training completed/failed (null if still running/pending)
   */
  @NumberField({ nullable: true })
  completedAt!: number | null;

  /**
   * Path to final checkpoint (null until training completes)
   *
   * Example: '/datasets/prepared/sessions/session-abc123/checkpoint-300'
   */
  @TextField({ nullable: true })
  finalCheckpointPath!: string | null;

  /**
   * Error information (null unless status === 'failed')
   */
  @JsonField({ nullable: true })
  error!: TrainingError | null;

  /**
   * Estimated training duration in milliseconds
   *
   * Calculated during preparation based on:
   * - Dataset size
   * - Batch size
   * - Epochs
   * - Historical training times
   */
  @NumberField({ nullable: true })
  estimatedDuration!: number | null;

  /**
   * User-provided description/notes about this session
   */
  @TextField({ nullable: true })
  description!: string | null;

  /**
   * Metadata for extensibility (custom fields, tags, etc.)
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
    if (!this.datasetHandle) {
      return { success: false, error: 'datasetHandle is required' };
    }
    if (!this.baseModel) {
      return { success: false, error: 'baseModel is required' };
    }
    if (!this.provider) {
      return { success: false, error: 'provider is required' };
    }
    if (!this.providerJobId) {
      return { success: false, error: 'providerJobId is required' };
    }
    if (!this.hyperparameters) {
      return { success: false, error: 'hyperparameters is required' };
    }
    if (!this.providerConfig) {
      return { success: false, error: 'providerConfig is required' };
    }
    if (!this.outputDir) {
      return { success: false, error: 'outputDir is required' };
    }
    if (!this.status) {
      return { success: false, error: 'status is required' };
    }

    // Validate hyperparameters structure
    const hp = this.hyperparameters;
    if (typeof hp.learningRate !== 'number' || hp.learningRate <= 0) {
      return { success: false, error: 'Invalid learning rate' };
    }
    if (typeof hp.batchSize !== 'number' || hp.batchSize <= 0) {
      return { success: false, error: 'Invalid batch size' };
    }
    if (typeof hp.epochs !== 'number' || hp.epochs <= 0) {
      return { success: false, error: 'Invalid epochs' };
    }
    if (typeof hp.rank !== 'number' || hp.rank <= 0) {
      return { success: false, error: 'Invalid LoRA rank' };
    }
    if (typeof hp.alpha !== 'number' || hp.alpha <= 0) {
      return { success: false, error: 'Invalid LoRA alpha' };
    }
    if (!Array.isArray(hp.targetModules) || hp.targetModules.length === 0) {
      return { success: false, error: 'targetModules must be non-empty array' };
    }

    // Validate provider config
    if (!this.providerConfig.provider) {
      return { success: false, error: 'providerConfig.provider is required' };
    }

    // Validate status enum
    const validStatuses: TrainingStatus[] = ['pending', 'running', 'completed', 'failed', 'cancelled'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Invalid status: ${this.status}` };
    }

    // Initialize metadata if not set
    if (!this.metadata) {
      this.metadata = {};
    }

    return { success: true };
  }

  /**
   * Check if session is in terminal state (completed/failed/cancelled)
   */
  isTerminal(): boolean {
    return this.status === 'completed' ||
           this.status === 'failed' ||
           this.status === 'cancelled';
  }

  /**
   * Check if session is active (running or pending)
   */
  isActive(): boolean {
    return this.status === 'running' || this.status === 'pending';
  }

  /**
   * Get training duration in milliseconds (null if not started or still running)
   */
  getDuration(): number | null {
    if (!this.startedAt || !this.completedAt) {
      return null;
    }
    return this.completedAt - this.startedAt;
  }

  /**
   * Mark session as started
   */
  markStarted(): void {
    this.status = 'running';
    this.startedAt = Date.now();
  }

  /**
   * Mark session as completed
   *
   * @param finalCheckpointPath - Path to final checkpoint
   */
  markCompleted(finalCheckpointPath: string): void {
    this.status = 'completed';
    this.completedAt = Date.now();
    this.finalCheckpointPath = finalCheckpointPath;
    this.error = null;
  }

  /**
   * Mark session as failed
   *
   * @param error - Error that caused failure
   */
  markFailed(error: TrainingError): void {
    this.status = 'failed';
    this.completedAt = Date.now();
    this.error = error;
  }

  /**
   * Mark session as cancelled
   */
  markCancelled(): void {
    this.status = 'cancelled';
    this.completedAt = Date.now();
  }

  /**
   * Update estimated duration
   *
   * @param durationMs - Estimated duration in milliseconds
   */
  setEstimatedDuration(durationMs: number): void {
    this.estimatedDuration = durationMs;
  }
}
