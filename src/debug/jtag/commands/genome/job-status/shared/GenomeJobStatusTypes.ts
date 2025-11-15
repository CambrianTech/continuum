/**
 * GenomeJobStatusTypes - Query fine-tuning job status
 *
 * Retrieves current status, progress, and metadata for a training job.
 * Works with jobs created via genome/job-create.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface GenomeJobStatusParams extends CommandParams {
  /** Job ID (from database, not provider's job ID) */
  jobId: UUID;

  /** If true, also query provider for latest status */
  refresh?: boolean;
}

export interface GenomeJobStatusResult extends CommandResult {
  success: boolean;
  error?: string;

  job?: {
    jobId: UUID;
    personaId: UUID;
    provider: string;
    providerJobId: string;
    baseModel: string;

    /** Current status from provider or database */
    status: 'validating_files' | 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

    /** Training progress (0-100, if available) */
    progress?: number;

    /** Timestamps */
    createdAt: number;
    updatedAt: number;
    startedAt?: number;
    completedAt?: number;

    /** Fine-tuned model ID (if succeeded) */
    fineTunedModel?: string;

    /** Training file IDs */
    trainingFileId: string;
    validationFileId?: string | null;

    /** Error details (if failed) */
    errorMessage?: string;
    errorCode?: string;

    /** Configuration summary */
    configuration: {
      method: string;
      epochs: number;
      batchSize: number;
      learningRate: number;
      sequenceLength: number;
    };

    /** Training metrics (if available) */
    metrics?: {
      trainingLoss?: number;
      validationLoss?: number;
      trainingAccuracy?: number;
      validationAccuracy?: number;
      currentEpoch?: number;
      totalEpochs?: number;
    };
  };

  /** True if status was refreshed from provider */
  refreshed?: boolean;
}
