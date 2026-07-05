/**
 * Training Dataset Entity
 *
 * Tracks parsed training datasets for AI fine-tuning.
 * Supports multiple sources (git, claude, cursor) and linking.
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { TextField, JsonField, DateField } from '../../../../system/data/decorators/FieldDecorators';

export type DatasetSource = 'git' | 'claude' | 'cursor' | 'vscode' | 'custom';
export type DatasetStage = 'parsing' | 'parsed' | 'preparing' | 'prepared' | 'linked' | 'failed';
export type DatasetFormat = 'chat-completion' | 'instruction-following' | 'code-completion';

/**
 * Quality distribution by score ranges
 */
export interface QualityDistribution {
  high: number;    // 0.8-1.0
  medium: number;  // 0.5-0.8
  low: number;     // 0.0-0.5
}

/**
 * Dataset metadata (size, quality, topics)
 */
export interface DatasetMetadata {
  totalExamples: number;
  totalTokens: number;
  averageLength: number;
  topics: string[];
  qualityDistribution: QualityDistribution;
  dateRange: {
    start: string;  // ISO timestamp
    end: string;    // ISO timestamp
  };
}

/**
 * Filter criteria used during parsing
 */
export interface FilterCriteria {
  minQuality?: number;
  includeTopics?: string[];
  excludeTopics?: string[];
  minLength?: number;
  maxLength?: number;
  since?: string;  // ISO timestamp for git history
  until?: string;  // ISO timestamp for git history
}

/**
 * Information about linked datasets (for master datasets)
 */
export interface LinkedDatasetInfo {
  datasetId: string;
  source: DatasetSource;
  weight: number;  // 0.0-1.0 (importance in final dataset)
  timeDelta?: string;  // e.g., "7 minutes" between conversation and commit
}

export class TrainingDatasetEntity extends BaseEntity {
  static readonly collection = 'training_datasets';

  get collection(): string {
    return TrainingDatasetEntity.collection;
  }

  /** Job ID (UUID) */
  @TextField()
  id!: string;

  /** Human-readable dataset name */
  @TextField()
  datasetName!: string;

  /** Data source type */
  @TextField()
  source!: DatasetSource;

  /** Path to source data (git repo path, archive path, etc.) */
  @TextField()
  sourcePath!: string;

  /** Path to parsed JSONL file in datasets/parsed/ */
  @TextField()
  jsonlPath!: string;

  /** Target skill for LoRA adapter */
  @TextField()
  targetSkill!: string;

  /** Training format */
  @TextField()
  format!: DatasetFormat;

  /** Current stage in pipeline */
  @TextField()
  stage!: DatasetStage;

  /** Dataset metadata (size, quality, topics) */
  @JsonField()
  metadata!: DatasetMetadata;

  /** Filter criteria used during parsing */
  @JsonField()
  filterCriteria!: FilterCriteria;

  /** Linked datasets (for master datasets created by linking) */
  @JsonField()
  linkedDatasets?: LinkedDatasetInfo[];

  /** Timestamp when parsing started */
  @DateField()
  parsedAt?: number;

  /** Timestamp when last used for training */
  @DateField()
  lastUsedAt?: number;

  /** Duration of parsing in milliseconds */
  @DateField()
  durationMs?: number;

  /** Error message (if failed) */
  @TextField()
  error?: string;

  validate(): { success: boolean; error?: string } {
    if (!this.id) {
      return { success: false, error: 'Missing required field: id' };
    }

    if (!this.datasetName) {
      return { success: false, error: 'Missing required field: datasetName' };
    }

    if (!this.source) {
      return { success: false, error: 'Missing required field: source' };
    }

    if (!this.sourcePath) {
      return { success: false, error: 'Missing required field: sourcePath' };
    }

    if (!this.jsonlPath) {
      return { success: false, error: 'Missing required field: jsonlPath' };
    }

    if (!this.targetSkill) {
      return { success: false, error: 'Missing required field: targetSkill' };
    }

    if (!this.format) {
      return { success: false, error: 'Missing required field: format' };
    }

    if (!this.stage) {
      return { success: false, error: 'Missing required field: stage' };
    }

    if (!this.metadata) {
      return { success: false, error: 'Missing required field: metadata' };
    }

    if (!this.filterCriteria) {
      return { success: false, error: 'Missing required field: filterCriteria' };
    }

    return { success: true };
  }
}
