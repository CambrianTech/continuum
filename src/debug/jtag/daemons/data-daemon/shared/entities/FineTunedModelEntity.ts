/**
 * Fine-Tuned Model Entity
 *
 * Tracks successfully trained fine-tuned models across providers.
 * Each entity represents a deployed model ready for inference.
 *
 * Lifecycle:
 * 1. Created when FineTuningJobEntity status becomes 'succeeded'
 * 2. 'active' - Model is deployed and available for inference
 * 3. 'inactive' - Model exists but not currently deployed
 * 4. 'deleted' - Model has been removed from provider
 *
 * Integration:
 * - Created automatically when fine-tuning job succeeds
 * - Referenced by FineTuningJobEntity.jobId (one-to-one relationship)
 * - Queried by genome/model-list command
 * - Used by AI adapters for inference routing
 *
 * Persistence:
 * - Survives server restarts
 * - Syncs with provider API to verify model still exists
 * - entity.id serves as the handle for all model operations
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  EnumField,
  NumberField,
  ForeignKeyField,
  BooleanField
} from '../../../../system/data/decorators/FieldDecorators';

export type ModelStatus = 'active' | 'inactive' | 'deleted';

/**
 * Validation metrics from training job
 */
export interface ValidationMetrics {
  readonly loss?: number;           // Final validation loss
  readonly accuracy?: number;        // Final validation accuracy
  readonly perplexity?: number;      // Model perplexity (if available)
  readonly bleu?: number;            // BLEU score (if available)
}

/**
 * Fine-Tuned Model Entity
 *
 * Registry of successfully trained models ready for inference.
 *
 * @example
 * ```typescript
 * // Create new model entity after job succeeds
 * const model = FineTunedModelEntity.create({
 *   personaId: 'helper-ai-uuid',
 *   provider: 'openai',
 *   providerModelId: 'ft:gpt-4o-mini:org:suffix:jobid',
 *   name: 'Coding Expert v1',
 *   baseModel: 'gpt-4o-mini-2024-07-18',
 *   jobId: 'job-uuid-001',
 *   object: 'fine_tuned_model',
 *   ownedBy: 'org-id',
 *   status: 'active',
 *   validationMetrics: {
 *     loss: 0.123,
 *     accuracy: 0.95
 *   }
 * });
 *
 * // Mark model as deleted
 * model.markDeleted();
 * ```
 */
export class FineTunedModelEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'fine_tuned_models';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return FineTunedModelEntity.collection;
  }

  /**
   * PersonaUser that owns this model
   */
  @ForeignKeyField({ references: 'users' })
  personaId!: UUID;

  /**
   * Provider name (e.g., 'openai', 'fireworks', 'mistral', 'together')
   */
  @TextField()
  provider!: string;

  /**
   * Provider-specific model ID (e.g., 'ft:gpt-4o-mini:org:suffix:jobid')
   * Used for inference API calls
   */
  @TextField()
  providerModelId!: string;

  /**
   * User-friendly name for the model
   */
  @TextField()
  name!: string;

  /**
   * Base model that was fine-tuned (e.g., 'gpt-4o-mini-2024-07-18')
   */
  @TextField()
  baseModel!: string;

  /**
   * Fine-tuning job that created this model (references FineTuningJobEntity.id)
   */
  @ForeignKeyField({ references: 'fine_tuning_jobs' })
  jobId!: UUID;

  /**
   * Object type from provider API (e.g., 'fine_tuned_model', 'model')
   */
  @TextField()
  object!: string;

  /**
   * When the model was created from provider (Unix timestamp)
   * Note: This is separate from BaseEntity.createdAt which tracks database insertion
   */
  @NumberField()
  providerCreatedAt!: number;

  /**
   * Owner identifier from provider (e.g., 'org-abc123', 'user:xyz789')
   */
  @TextField()
  ownedBy!: string;

  /**
   * Current status of the model
   */
  @EnumField()
  status!: ModelStatus;

  /**
   * Validation metrics from training job (if available)
   */
  @JsonField({ nullable: true })
  validationMetrics!: ValidationMetrics | null;

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
    if (!this.providerModelId) {
      return { success: false, error: 'providerModelId is required' };
    }
    if (!this.name) {
      return { success: false, error: 'name is required' };
    }
    if (!this.baseModel) {
      return { success: false, error: 'baseModel is required' };
    }
    if (!this.jobId) {
      return { success: false, error: 'jobId is required' };
    }
    if (!this.object) {
      return { success: false, error: 'object is required' };
    }
    if (typeof this.createdAt !== 'number' || this.createdAt <= 0) {
      return { success: false, error: 'createdAt must be positive number' };
    }
    if (!this.ownedBy) {
      return { success: false, error: 'ownedBy is required' };
    }
    if (!this.status) {
      return { success: false, error: 'status is required' };
    }

    // Validate status enum
    const validStatuses: ModelStatus[] = ['active', 'inactive', 'deleted'];
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
   * Check if model is active and available for inference
   */
  isActive(): boolean {
    return this.status === 'active';
  }

  /**
   * Check if model is deleted
   */
  isDeleted(): boolean {
    return this.status === 'deleted';
  }

  /**
   * Get final validation loss (if available)
   */
  getValidationLoss(): number | null {
    return this.validationMetrics?.loss || null;
  }

  /**
   * Get final validation accuracy (if available)
   */
  getValidationAccuracy(): number | null {
    return this.validationMetrics?.accuracy || null;
  }

  /**
   * Mark model as active (ready for inference)
   */
  markActive(): void {
    this.status = 'active';
  }

  /**
   * Mark model as inactive (exists but not deployed)
   */
  markInactive(): void {
    this.status = 'inactive';
  }

  /**
   * Mark model as deleted
   */
  markDeleted(): void {
    this.status = 'deleted';
  }

  /**
   * Update validation metrics
   *
   * @param metrics - Validation metrics from training job
   */
  setValidationMetrics(metrics: ValidationMetrics): void {
    this.validationMetrics = metrics;
  }
}
