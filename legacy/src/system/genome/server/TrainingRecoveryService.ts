/**
 * TrainingRecoveryService — Startup crash recovery for training jobs
 *
 * On server startup, scans for orphaned training jobs:
 * - Jobs marked 'running'/'checkpointed' but no sentinel process alive
 * - Auto-marks as 'crashed' and attempts resume from last checkpoint
 *
 * This closes the gap where in-memory state (TrainingCompletionHandler,
 * TrainingStepBridge) is lost on server crash but TrainingJobEntity
 * persists in the database.
 *
 * @see #365 — Training job persistence: checkpoint resume, crash recovery
 */

import { Logger } from '../../core/logging/Logger';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { DataUpdate } from '../../../commands/data/update/shared/DataUpdateTypes';
import { GenomeTrainResume } from '../../../commands/genome/train/resume/shared/GenomeTrainResumeTypes';
import { TrainingJobEntity } from '../entities/TrainingJobEntity';
import type { TrainingJobStatus } from '../entities/TrainingJobEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';

const log = Logger.create('TrainingRecovery');

/** Active statuses that indicate a training process should be running */
const ACTIVE_STATUSES: TrainingJobStatus[] = ['running', 'checkpointed'];

/**
 * Scan for orphaned training jobs and attempt recovery.
 *
 * Called once during ServiceInitializer startup, after sentinel module is ready.
 * For each orphaned job with a checkpoint: auto-resume via genome/train/resume.
 * For jobs without checkpoints: mark as failed (nothing to recover from).
 */
export async function recoverOrphanedTrainingJobs(): Promise<void> {
  log.info('Scanning for orphaned training jobs...');

  // Find all jobs that were supposedly running when the server last died
  const listResult = await DataList.execute({
    dbHandle: 'default',
    collection: TrainingJobEntity.collection,
  });

  if (!listResult.success || !listResult.items?.length) {
    log.info('No training jobs found');
    return;
  }

  const orphaned: Array<{ id: string; status: string; sentinelHandle?: string; checkpoints: unknown[]; latestCheckpointPath?: string }> = [];

  for (const item of listResult.items) {
    const job = item as Record<string, unknown>;
    const status = job.status as string;
    if (!ACTIVE_STATUSES.includes(status as TrainingJobStatus)) continue;
    orphaned.push({
      id: job.id as string,
      status,
      sentinelHandle: job.sentinelHandle as string | undefined,
      checkpoints: (job.checkpoints as unknown[]) ?? [],
      latestCheckpointPath: job.latestCheckpointPath as string | undefined,
    });
  }

  if (orphaned.length === 0) {
    log.info('No orphaned training jobs found');
    return;
  }

  log.info(`Found ${orphaned.length} orphaned training job(s) — recovering...`);

  for (const job of orphaned) {
    await recoverJob(job);
  }
}

async function recoverJob(job: {
  id: string;
  status: string;
  sentinelHandle?: string;
  checkpoints: unknown[];
  latestCheckpointPath?: string;
}): Promise<void> {
  const hasCheckpoint = job.checkpoints.length > 0 && !!job.latestCheckpointPath;

  // First, mark the job as crashed (sentinel process is gone)
  await DataUpdate.execute({
    dbHandle: 'default',
    collection: TrainingJobEntity.collection,
    id: job.id as UUID,
    data: {
      status: 'crashed' as TrainingJobStatus,
      sentinelHandle: null,
    },
  });

  if (!hasCheckpoint) {
    // No checkpoint — mark as failed, nothing to resume from
    await DataUpdate.execute({
      dbHandle: 'default',
      collection: TrainingJobEntity.collection,
      id: job.id as UUID,
      data: {
        status: 'failed' as TrainingJobStatus,
        error: 'Server crash with no checkpoint — training must restart from scratch',
      },
    });
    log.warn(`Job ${job.id}: no checkpoint, marked as failed`);
    return;
  }

  // Has checkpoint — attempt auto-resume
  log.info(`Job ${job.id}: has checkpoint at ${job.latestCheckpointPath}, attempting auto-resume...`);

  try {
    const resumeResult = await GenomeTrainResume.execute({
      jobId: job.id,
    });

    if (resumeResult.success) {
      log.info(`Job ${job.id}: auto-resumed successfully (handle: ${resumeResult.sentinelHandle})`);
    } else {
      log.warn(`Job ${job.id}: resume failed: ${resumeResult.error}`);
    }
  } catch (err) {
    log.warn(`Job ${job.id}: resume threw: ${err}`);
    // Job stays as 'crashed' — user can manually resume via genome/train/resume
  }
}

/**
 * Initialize training recovery service.
 * Call this from ServiceInitializer after sentinel module is ready.
 */
export function initializeTrainingRecovery(): void {
  // Delay 5s to ensure sentinels and data layer are fully ready
  setTimeout(async () => {
    try {
      await recoverOrphanedTrainingJobs();
    } catch (err) {
      log.warn(`Training recovery scan failed: ${err}`);
    }
  }, 5_000);
}
