/**
 * GenomeJobStatusServerCommand - Query fine-tuning job status
 *
 * Retrieves current status, progress, and metadata for a training job.
 * This is a convenience wrapper around data/read with proper typing.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeJobStatusParams,
  GenomeJobStatusResult
} from '../shared/GenomeJobStatusTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { COLLECTIONS } from '../../../../system/shared/Constants';
import type { DataReadParams, DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { FineTuningJobEntity } from '../../../../daemons/data-daemon/shared/entities/FineTuningJobEntity';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';

import { DataRead } from '../../../data/read/shared/DataReadTypes';
export class GenomeJobStatusServerCommand extends CommandBase<
  GenomeJobStatusParams,
  GenomeJobStatusResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-job-status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeJobStatusResult> {
    const statusParams = params as GenomeJobStatusParams;

    console.log('🧬 GENOME JOB STATUS: Querying job status');
    console.log(`   Job ID: ${statusParams.jobId}`);

    try {
      // 1. Validate required fields
      if (!statusParams.jobId) {
        return transformPayload(params, {
          success: false,
          error: 'jobId is required'
        });
      }

      // 2. Query database for job
      const result = await DataRead.execute<FineTuningJobEntity>({
        collection: COLLECTIONS.FINE_TUNING_JOBS,
        id: statusParams.jobId
      });

      if (!result.success) {
        return transformPayload(params, {
          success: false,
          error: result.error || 'Failed to read job from database'
        });
      }

      if (!result.found || !result.data) {
        return transformPayload(params, {
          success: false,
          error: `Job not found: ${statusParams.jobId}`
        });
      }

      const jobEntity = result.data;

      // 3. TODO: If refresh=true, query provider for latest status and update database
      // This would call the provider's adapter to get real-time status
      // For now, we just return what's in the database

      console.log(`✅ GENOME JOB STATUS: Job found`);
      console.log(`   Provider: ${jobEntity.provider}`);
      console.log(`   Status: ${jobEntity.status}`);

      // 4. Return formatted job status
      return transformPayload(params, {
        success: true,
        job: {
          jobId: jobEntity.id,
          personaId: jobEntity.personaId,
          provider: jobEntity.provider,
          providerJobId: jobEntity.providerJobId,
          baseModel: jobEntity.baseModel,
          status: jobEntity.status,
          progress: undefined, // TODO: Calculate from metrics if available
          createdAt: jobEntity.providerCreatedAt,
          updatedAt: jobEntity.updatedAt instanceof Date ? jobEntity.updatedAt.getTime() : Date.now(),
          startedAt: typeof jobEntity.startedAt === 'number' ? jobEntity.startedAt : undefined,
          completedAt: typeof jobEntity.completedAt === 'number' ? jobEntity.completedAt : undefined,
          fineTunedModel: jobEntity.fineTunedModel || undefined,
          trainingFileId: jobEntity.trainingFileId,
          validationFileId: jobEntity.validationFileId,
          errorMessage: jobEntity.error?.message,
          errorCode: jobEntity.error?.code,
          configuration: jobEntity.configuration ? {
            method: jobEntity.configuration.method.type,
            epochs: jobEntity.configuration.schedule.epochs,
            batchSize: jobEntity.configuration.schedule.batchSize,
            learningRate: jobEntity.configuration.optimizer.learningRate,
            sequenceLength: jobEntity.configuration.schedule.sequenceLength
          } : {
            method: 'unknown',
            epochs: 0,
            batchSize: 0,
            learningRate: 0,
            sequenceLength: 0
          },
          metrics: undefined // Metrics structure needs to be defined in FineTuningJobEntity
        },
        refreshed: false // TODO: Set to true when we implement provider refresh
      });

    } catch (error) {
      console.error('❌ GENOME JOB STATUS: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
