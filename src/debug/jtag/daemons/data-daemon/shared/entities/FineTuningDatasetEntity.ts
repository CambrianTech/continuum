/**
 * Fine-Tuning Dataset Entity
 *
 * Tracks uploaded training/validation datasets across providers.
 * Each dataset represents a file uploaded to a provider's API for fine-tuning.
 *
 * Lifecycle:
 * 1. 'pending' - File is being uploaded
 * 2. 'processing' - Provider is validating the file
 * 3. 'processed' - File is ready for use in training
 * 4. 'error' - Validation failed
 * 5. 'deleted' - File has been deleted from provider
 *
 * Integration:
 * - Created by genome/dataset-upload command
 * - Referenced by FineTuningJobEntity.trainingFileId
 * - Queried by genome/dataset-list command
 * - Deleted by genome/dataset-delete command
 *
 * Persistence:
 * - Survives server restarts
 * - Syncs with provider API on status queries
 * - entity.id serves as the handle for all dataset operations
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

export type DatasetStatus = 'pending' | 'processing' | 'processed' | 'error' | 'deleted';
export type DatasetPurpose = 'fine-tune' | 'validation';

/**
 * Status details from provider validation
 */
export interface DatasetStatusDetails {
  readonly exampleCount?: number;        // Number of training examples
  readonly tokenCount?: number;          // Total tokens in dataset
  readonly errors?: string[];            // Validation errors if any
  readonly warnings?: string[];          // Validation warnings
  readonly estimatedCost?: number;       // Estimated training cost (if available)
}

/**
 * Fine-Tuning Dataset Entity
 *
 * Represents an uploaded dataset file tracked across server restarts.
 *
 * @example
 * ```typescript
 * // Create new dataset entity
 * const dataset = FineTuningDatasetEntity.create({
 *   personaId: 'helper-ai-uuid',
 *   provider: 'openai',
 *   providerFileId: 'file-abc123',
 *   name: 'Coding Examples v1',
 *   filename: 'coding-examples.jsonl',
 *   purpose: 'fine-tune',
 *   bytes: 524288,
 *   status: 'processing'
 * });
 *
 * // Update status after provider validation
 * dataset.status = 'processed';
 * dataset.statusDetails = {
 *   exampleCount: 1000,
 *   tokenCount: 250000,
 *   warnings: ['Some examples truncated to max length']
 * };
 * ```
 */
export class FineTuningDatasetEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'fine_tuning_datasets';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return FineTuningDatasetEntity.collection;
  }

  /**
   * PersonaUser that owns this dataset
   */
  @ForeignKeyField({ references: 'users' })
  personaId!: UUID;

  /**
   * Provider name (e.g., 'openai', 'fireworks', 'mistral', 'together')
   */
  @TextField()
  provider!: string;

  /**
   * Provider-specific file ID (e.g., OpenAI's 'file-abc123')
   * Used to reference this dataset in fine-tuning API calls
   */
  @TextField()
  providerFileId!: string;

  /**
   * User-friendly name for the dataset
   */
  @TextField()
  name!: string;

  /**
   * Original filename (e.g., 'training-data.jsonl')
   */
  @TextField()
  filename!: string;

  /**
   * Purpose of the dataset ('fine-tune' or 'validation')
   */
  @EnumField()
  purpose!: DatasetPurpose;

  /**
   * File size in bytes
   */
  @NumberField()
  bytes!: number;

  /**
   * Current status of the dataset
   */
  @EnumField()
  status!: DatasetStatus;

  /**
   * Status details from provider validation
   */
  @JsonField({ nullable: true })
  statusDetails!: DatasetStatusDetails | null;

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
    if (!this.provider) {
      return { success: false, error: 'provider is required' };
    }
    if (!this.providerFileId) {
      return { success: false, error: 'providerFileId is required' };
    }
    if (!this.name) {
      return { success: false, error: 'name is required' };
    }
    if (!this.filename) {
      return { success: false, error: 'filename is required' };
    }
    if (!this.purpose) {
      return { success: false, error: 'purpose is required' };
    }
    if (typeof this.bytes !== 'number' || this.bytes < 0) {
      return { success: false, error: 'bytes must be non-negative number' };
    }
    if (!this.status) {
      return { success: false, error: 'status is required' };
    }

    // Validate purpose enum
    const validPurposes: DatasetPurpose[] = ['fine-tune', 'validation'];
    if (!validPurposes.includes(this.purpose)) {
      return { success: false, error: `Invalid purpose: ${this.purpose}` };
    }

    // Validate status enum
    const validStatuses: DatasetStatus[] = ['pending', 'processing', 'processed', 'error', 'deleted'];
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
   * Check if dataset is ready for use in training
   */
  isReady(): boolean {
    return this.status === 'processed';
  }

  /**
   * Check if dataset has errors
   */
  hasError(): boolean {
    return this.status === 'error';
  }

  /**
   * Check if dataset is deleted
   */
  isDeleted(): boolean {
    return this.status === 'deleted';
  }

  /**
   * Get validation error messages (if any)
   */
  getErrors(): string[] {
    return this.statusDetails?.errors || [];
  }

  /**
   * Get validation warnings (if any)
   */
  getWarnings(): string[] {
    return this.statusDetails?.warnings || [];
  }

  /**
   * Mark dataset as processed
   *
   * @param details - Validation details from provider
   */
  markProcessed(details: DatasetStatusDetails): void {
    this.status = 'processed';
    this.statusDetails = details;
  }

  /**
   * Mark dataset as failed
   *
   * @param errors - Error messages from provider
   */
  markFailed(errors: string[]): void {
    this.status = 'error';
    this.statusDetails = {
      errors,
      exampleCount: 0,
      tokenCount: 0
    };
  }

  /**
   * Mark dataset as deleted
   */
  markDeleted(): void {
    this.status = 'deleted';
  }
}
