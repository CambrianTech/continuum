/**
 * Genome Train List Jobs Command - Server Implementation
 *
 * List all training jobs with status, progress, checkpoints, and node info.
 * Shows running, completed, crashed, and resumable jobs.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainListJobsParams, GenomeTrainListJobsResult } from '../shared/GenomeTrainListJobsTypes';
import { createGenomeTrainListJobsResultFromParams } from '../shared/GenomeTrainListJobsTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { TrainingJobEntity } from '@system/genome/entities/TrainingJobEntity';

export class GenomeTrainListJobsServerCommand extends CommandBase<GenomeTrainListJobsParams, GenomeTrainListJobsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train/list-jobs', context, subpath, commander);
  }

  async execute(params: GenomeTrainListJobsParams): Promise<GenomeTrainListJobsResult> {
    const limit = params.limit ?? 20;

    // Build filter
    const filter: Record<string, unknown> = {};
    if (params.status) filter.status = params.status;
    if (params.personaId) filter.personaId = params.personaId;
    if (params.nodeId) filter.nodeId = params.nodeId;

    const result = await DataList.execute({
      collection: TrainingJobEntity.collection,
      filter: Object.keys(filter).length > 0 ? filter : undefined,
      orderBy: [{ field: 'createdAt', direction: 'desc' }],
      limit,
      dbHandle: 'default',
    });

    const items = (result.success && result.items) ? result.items : [];

    // Compute counts
    let activeCount = 0;
    let resumableCount = 0;
    const jobs = items.map((item: any) => {
      const isActive = item.status === 'running' || item.status === 'checkpointed';
      const isResumable = (item.status === 'crashed' || item.status === 'failed') &&
        item.checkpoints && item.checkpoints.length > 0;

      if (isActive) activeCount++;
      if (isResumable) resumableCount++;

      return {
        id: item.id,
        personaId: item.personaId,
        personaName: item.personaName,
        domain: item.domain,
        status: item.status,
        nodeId: item.nodeId,
        baseModel: item.trainingConfig?.baseModel ?? '',
        currentStep: item.currentStep ?? 0,
        totalSteps: item.totalSteps ?? 0,
        currentLoss: item.currentLoss ?? 0,
        progress: item.totalSteps > 0 ? Math.round((item.currentStep / item.totalSteps) * 100) : 0,
        epochs: item.trainingConfig?.epochs ?? 0,
        exampleCount: item.exampleCount ?? 0,
        crashCount: item.crashCount ?? 0,
        latestCheckpointStep: item.latestCheckpointStep,
        latestCheckpointAt: item.latestCheckpointAt,
        resumable: isResumable,
        startedAt: item.startedAt,
        completedAt: item.completedAt,
        totalTrainingTimeMs: item.totalTrainingTimeMs ?? 0,
        error: item.error,
      };
    });

    return createGenomeTrainListJobsResultFromParams(params, {
      success: true,
      jobs,
      totalCount: jobs.length,
      activeCount,
      resumableCount,
    });
  }
}
