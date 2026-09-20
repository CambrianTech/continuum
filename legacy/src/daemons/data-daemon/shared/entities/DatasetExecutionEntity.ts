/**
 * Dataset Execution Entity
 *
 * Tracks the status of dataset archive creation jobs.
 * Allows for async/background execution with progress tracking.
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { TextField, BooleanField, JsonField, DateField } from '../../../../system/data/decorators/FieldDecorators';
import type { CompressionType } from '../../../../commands/ai/dataset/shared/DatasetConfig';

export interface DatasetArchiveInfo {
  projectId: string;
  projectName: string;
  filename: string;
  path: string;
  sizeBytes: number;
  compressionType: CompressionType;
}

export interface DatasetProgress {
  totalProjects: number;
  completedProjects: number;
  percentComplete: number;
}

export interface DatasetSummary {
  total: number;
  successful: number;
  failed: number;
  totalSizeBytes: number;
}

export class DatasetExecutionEntity extends BaseEntity {
  static readonly collection = 'dataset_executions';

  get collection(): string {
    return DatasetExecutionEntity.collection;
  }

  /** Job ID (UUID) */
  @TextField()
  id!: string;

  /** Filter: specific project ID (optional) */
  @TextField()
  projectFilter?: string;

  /** Filter: specific source ID (optional) */
  @TextField()
  sourceFilter?: string;

  /** Output path for archives */
  @TextField()
  outputPath!: string;

  /** Compression type */
  @TextField()
  compression!: CompressionType;

  /** Include manifest.json */
  @BooleanField()
  includeManifest!: boolean;

  /** Job status */
  @TextField()
  status!: 'queued' | 'running' | 'completed' | 'failed';

  /** Progress tracking */
  @JsonField()
  progress!: DatasetProgress;

  /** Created archives */
  @JsonField()
  archives!: DatasetArchiveInfo[];

  /** Summary statistics */
  @JsonField()
  summary!: DatasetSummary;

  /** Start timestamp */
  @DateField()
  startedAt?: number;

  /** End timestamp */
  @DateField()
  completedAt?: number;

  /** Duration in milliseconds */
  @DateField()
  durationMs?: number;

  /** Error message (if failed) */
  @TextField()
  error?: string;

  validate(): { success: boolean; error?: string } {
    if (!this.id) {
      return { success: false, error: 'Missing required field: id' };
    }

    if (!this.outputPath) {
      return { success: false, error: 'Missing required field: outputPath' };
    }

    if (!this.compression) {
      return { success: false, error: 'Missing required field: compression' };
    }

    if (!this.status) {
      return { success: false, error: 'Missing required field: status' };
    }

    if (!this.progress || typeof this.progress.totalProjects !== 'number') {
      return { success: false, error: 'Invalid progress object' };
    }

    if (!this.summary) {
      return { success: false, error: 'Missing required field: summary' };
    }

    return { success: true };
  }
}
