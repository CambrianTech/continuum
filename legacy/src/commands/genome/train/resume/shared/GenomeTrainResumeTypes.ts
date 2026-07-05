/**
 * Genome Train Resume Command - Shared Types
 *
 * Resume a crashed or failed training job from its latest checkpoint. Looks up the TrainingJobEntity, verifies checkpoint exists on disk, and restarts genome/train with resumeFromCheckpoint pointing to the latest checkpoint directory.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Train Resume Command Parameters
 */
export interface GenomeTrainResumeParams extends CommandParams {
  // TrainingJobEntity UUID of the job to resume
  jobId: string;
  // Specific checkpoint path to resume from (default: latest checkpoint)
  checkpoint?: string;
}

/**
 * Factory function for creating GenomeTrainResumeParams
 */
export const createGenomeTrainResumeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // TrainingJobEntity UUID of the job to resume
    jobId: string;
    // Specific checkpoint path to resume from (default: latest checkpoint)
    checkpoint?: string;
  }
): GenomeTrainResumeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  checkpoint: data.checkpoint ?? '',
  ...data
});

/**
 * Genome Train Resume Command Result
 */
export interface GenomeTrainResumeResult extends CommandResult {
  success: boolean;
  // Whether the job was successfully resumed
  resumed: boolean;
  // The training job UUID
  jobId: string;
  // Step number of the checkpoint being resumed from
  checkpointStep: number;
  // Path to the checkpoint directory
  checkpointPath: string;
  // Number of times this job has been resumed (including this one)
  crashCount: number;
  // New sentinel handle for the resumed training process
  sentinelHandle: string;
  error?: JTAGError;
}

/**
 * Factory function for creating GenomeTrainResumeResult with defaults
 */
export const createGenomeTrainResumeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the job was successfully resumed
    resumed?: boolean;
    // The training job UUID
    jobId?: string;
    // Step number of the checkpoint being resumed from
    checkpointStep?: number;
    // Path to the checkpoint directory
    checkpointPath?: string;
    // Number of times this job has been resumed (including this one)
    crashCount?: number;
    // New sentinel handle for the resumed training process
    sentinelHandle?: string;
    error?: JTAGError;
  }
): GenomeTrainResumeResult => createPayload(context, sessionId, {
  resumed: data.resumed ?? false,
  jobId: data.jobId ?? '',
  checkpointStep: data.checkpointStep ?? 0,
  checkpointPath: data.checkpointPath ?? '',
  crashCount: data.crashCount ?? 0,
  sentinelHandle: data.sentinelHandle ?? '',
  ...data
});

/**
 * Smart Genome Train Resume-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeTrainResumeResultFromParams = (
  params: GenomeTrainResumeParams,
  differences: Omit<GenomeTrainResumeResult, 'context' | 'sessionId' | 'userId'>
): GenomeTrainResumeResult => transformPayload(params, differences);

/**
 * Genome Train Resume — Type-safe command executor
 *
 * Usage:
 *   import { GenomeTrainResume } from '...shared/GenomeTrainResumeTypes';
 *   const result = await GenomeTrainResume.execute({ ... });
 */
export const GenomeTrainResume = {
  execute(params: CommandInput<GenomeTrainResumeParams>): Promise<GenomeTrainResumeResult> {
    return Commands.execute<GenomeTrainResumeParams, GenomeTrainResumeResult>('genome/train/resume', params as Partial<GenomeTrainResumeParams>);
  },
  commandName: 'genome/train/resume' as const,
} as const;
