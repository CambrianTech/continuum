/**
 * Genome Train List Jobs Command - Shared Types
 *
 * List all training jobs with status, progress, checkpoints, and node info. Shows running, completed, crashed, and resumable jobs. Use genome/train/resume to restart crashed jobs from their latest checkpoint.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Train List Jobs Command Parameters
 */
export interface GenomeTrainListJobsParams extends CommandParams {
  // Filter by job status: pending, running, checkpointed, completed, failed, crashed, cancelled
  status?: string;
  // Filter by persona UUID
  personaId?: string;
  // Filter by grid node ID (e.g., '100.124.122.107' or 'local')
  nodeId?: string;
  // Maximum number of jobs to return (default: 20)
  limit?: number;
}

/**
 * Factory function for creating GenomeTrainListJobsParams
 */
export const createGenomeTrainListJobsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter by job status: pending, running, checkpointed, completed, failed, crashed, cancelled
    status?: string;
    // Filter by persona UUID
    personaId?: string;
    // Filter by grid node ID (e.g., '100.124.122.107' or 'local')
    nodeId?: string;
    // Maximum number of jobs to return (default: 20)
    limit?: number;
  }
): GenomeTrainListJobsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  status: data.status ?? '',
  personaId: data.personaId ?? '',
  nodeId: data.nodeId ?? '',
  limit: data.limit ?? 0,
  ...data
});

/**
 * Genome Train List Jobs Command Result
 */
export interface GenomeTrainListJobsResult extends CommandResult {
  success: boolean;
  // Array of training job summaries
  jobs: object[];
  // Total number of matching jobs
  totalCount: number;
  // Number of currently running/checkpointed jobs
  activeCount: number;
  // Number of crashed jobs that can be resumed
  resumableCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating GenomeTrainListJobsResult with defaults
 */
export const createGenomeTrainListJobsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of training job summaries
    jobs?: object[];
    // Total number of matching jobs
    totalCount?: number;
    // Number of currently running/checkpointed jobs
    activeCount?: number;
    // Number of crashed jobs that can be resumed
    resumableCount?: number;
    error?: JTAGError;
  }
): GenomeTrainListJobsResult => createPayload(context, sessionId, {
  jobs: data.jobs ?? [],
  totalCount: data.totalCount ?? 0,
  activeCount: data.activeCount ?? 0,
  resumableCount: data.resumableCount ?? 0,
  ...data
});

/**
 * Smart Genome Train List Jobs-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeTrainListJobsResultFromParams = (
  params: GenomeTrainListJobsParams,
  differences: Omit<GenomeTrainListJobsResult, 'context' | 'sessionId' | 'userId'>
): GenomeTrainListJobsResult => transformPayload(params, differences);

/**
 * Genome Train List Jobs — Type-safe command executor
 *
 * Usage:
 *   import { GenomeTrainListJobs } from '...shared/GenomeTrainListJobsTypes';
 *   const result = await GenomeTrainListJobs.execute({ ... });
 */
export const GenomeTrainListJobs = {
  execute(params: CommandInput<GenomeTrainListJobsParams>): Promise<GenomeTrainListJobsResult> {
    return Commands.execute<GenomeTrainListJobsParams, GenomeTrainListJobsResult>('genome/train/list-jobs', params as Partial<GenomeTrainListJobsParams>);
  },
  commandName: 'genome/train/list-jobs' as const,
} as const;
