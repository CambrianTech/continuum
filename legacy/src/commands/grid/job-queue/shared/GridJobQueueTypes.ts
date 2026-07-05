/**
 * Grid Job Queue Command - Shared Types
 *
 * List all jobs on a grid node: queued, running, paused, completed, failed. Shows the full job lifecycle with alloy names, progress, and timing.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Grid Job Queue Command Parameters
 */
export interface GridJobQueueParams extends CommandParams {
  // Target node. Default: all known nodes.
  nodeId?: string;
  // Filter by state: 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'all'. Default: 'all'
  state?: string;
  // Max jobs to return. Default: 20
  limit?: number;
}

/**
 * Factory function for creating GridJobQueueParams
 */
export const createGridJobQueueParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Target node. Default: all known nodes.
    nodeId?: string;
    // Filter by state: 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'all'. Default: 'all'
    state?: string;
    // Max jobs to return. Default: 20
    limit?: number;
  }
): GridJobQueueParams => createPayload(context, sessionId, {
  // userId is auto-injected by infrastructure at runtime
  nodeId: data.nodeId ?? '',
  state: data.state ?? '',
  limit: data.limit ?? 0,
  ...data
}) as GridJobQueueParams;

/**
 * Grid Job Queue Command Result
 */
export interface GridJobQueueResult extends CommandResult {
  success: boolean;
  // Array of jobs: { jobId, alloyName, state, progress: { cycle, totalCycles, step, totalSteps }, startedAt, estimatedCompletion, nodeId }
  jobs: object;
  // { queued, running, paused, completed, failed } counts
  summary: object;
  error?: JTAGError;
}

/**
 * Factory function for creating GridJobQueueResult with defaults
 */
export const createGridJobQueueResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of jobs: { jobId, alloyName, state, progress: { cycle, totalCycles, step, totalSteps }, startedAt, estimatedCompletion, nodeId }
    jobs?: object;
    // { queued, running, paused, completed, failed } counts
    summary?: object;
    error?: JTAGError;
  }
): GridJobQueueResult => createPayload(context, sessionId, {
  jobs: data.jobs ?? {},
  summary: data.summary ?? {},
  ...data
});

/**
 * Smart Grid Job Queue-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGridJobQueueResultFromParams = (
  params: GridJobQueueParams,
  differences: Omit<GridJobQueueResult, 'context' | 'sessionId' | 'userId'>
): GridJobQueueResult => transformPayload(params, differences);

/**
 * Grid Job Queue — Type-safe command executor
 *
 * Usage:
 *   import { GridJobQueue } from '...shared/GridJobQueueTypes';
 *   const result = await GridJobQueue.execute({ ... });
 */
export const GridJobQueue = {
  execute(params: CommandInput<GridJobQueueParams>): Promise<GridJobQueueResult> {
    return Commands.execute<GridJobQueueParams, GridJobQueueResult>('grid/job-queue', params as Partial<GridJobQueueParams>);
  },
  commandName: 'grid/job-queue' as const,
} as const;
