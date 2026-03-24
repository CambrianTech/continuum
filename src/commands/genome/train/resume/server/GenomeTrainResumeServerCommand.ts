/**
 * Genome Train Resume Command - Server Implementation
 *
 * Resume a crashed or failed training job from its latest checkpoint.
 * Looks up the TrainingJobEntity, verifies checkpoint on disk, restarts
 * genome/train with resumeFromCheckpoint.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeTrainResumeParams, GenomeTrainResumeResult } from '../shared/GenomeTrainResumeTypes';
import { createGenomeTrainResumeResultFromParams } from '../shared/GenomeTrainResumeTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
import { TrainingJobEntity } from '@system/genome/entities/TrainingJobEntity';
import { GenomeTrain } from '@commands/genome/train/shared/GenomeTrainTypes';
import * as fs from 'fs';

export class GenomeTrainResumeServerCommand extends CommandBase<GenomeTrainResumeParams, GenomeTrainResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train/resume', context, subpath, commander);
  }

  async execute(params: GenomeTrainResumeParams): Promise<GenomeTrainResumeResult> {
    if (!params.jobId) {
      throw new ValidationError('jobId', 'Missing required parameter. Use genome/train/list-jobs to find job IDs.');
    }

    // 1. Load job from database
    const readResult = await DataList.execute({
      collection: TrainingJobEntity.collection,
      filter: { id: params.jobId },
      limit: 1,
      dbHandle: 'default',
    });

    const job = readResult.items?.[0] as any;
    if (!job) {
      throw new ValidationError('jobId', `Training job not found: ${params.jobId}`);
    }

    // 2. Verify job is resumable
    if (job.status !== 'crashed' && job.status !== 'failed') {
      throw new ValidationError('jobId',
        `Job status is '${job.status}' — only crashed or failed jobs can be resumed`);
    }

    // 3. Find checkpoint to resume from
    const checkpointPath = params.checkpoint ?? job.latestCheckpointPath;
    if (!checkpointPath) {
      throw new ValidationError('checkpoint',
        'No checkpoint available to resume from. Training must be restarted from scratch.');
    }

    // 4. Verify checkpoint exists on disk
    if (!fs.existsSync(checkpointPath)) {
      throw new ValidationError('checkpoint',
        `Checkpoint directory not found on disk: ${checkpointPath}`);
    }

    // 5. Restart training with checkpoint
    const config = job.trainingConfig ?? {};
    const trainResult = await GenomeTrain.execute({
      personaId: job.personaId,
      personaName: job.personaName,
      traitType: job.domain,
      datasetPath: job.datasetPath,
      baseModel: config.baseModel,
      rank: config.rank,
      epochs: config.epochs,
      learningRate: config.learningRate,
      batchSize: config.batchSize,
      quantize: config.quantize,
      quantizeBits: config.quantizeBits,
      async: true,
      resumeFromCheckpoint: checkpointPath,
    });

    if (!trainResult.success) {
      throw new Error(`Failed to restart training: ${trainResult.error}`);
    }

    // 6. Update job entity with resumed state
    const newCrashCount = (job.crashCount ?? 0) + 1;
    try {
      await DataUpdate.execute({
        collection: TrainingJobEntity.collection,
        id: params.jobId,
        data: {
          status: 'running',
          sentinelHandle: trainResult.sentinelHandle,
          crashCount: newCrashCount,
        },
        dbHandle: 'default',
      });
    } catch (err) {
      console.warn(`[genome/train/resume] Failed to update job entity: ${err}`);
    }

    return createGenomeTrainResumeResultFromParams(params, {
      success: true,
      resumed: true,
      jobId: params.jobId,
      checkpointStep: job.latestCheckpointStep ?? 0,
      checkpointPath,
      crashCount: newCrashCount,
      sentinelHandle: trainResult.sentinelHandle ?? '',
    });
  }
}
