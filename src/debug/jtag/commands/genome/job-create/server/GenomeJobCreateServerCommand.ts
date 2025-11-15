/**
 * GenomeJobCreateServerCommand - Create fine-tuning jobs with comprehensive configuration
 *
 * Creates a new fine-tuning job using the universal JobConfiguration schema.
 * Validates configuration, creates database entity, and returns job details.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeJobCreateParams,
  GenomeJobCreateResult
} from '../shared/GenomeJobCreateTypes';
import { FineTuningJobEntity } from '../../../../daemons/data-daemon/shared/entities/FineTuningJobEntity';
import { PARAMETER_RANGES, type JobConfiguration } from '../../../../daemons/data-daemon/shared/entities/FineTuningTypes';
import { COLLECTIONS } from '../../../../system/shared/Constants';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataCreateResult } from '../../../../commands/data/create/shared/DataCreateTypes';
import { v4 as uuidv4 } from 'uuid';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export class GenomeJobCreateServerCommand extends CommandBase<
  GenomeJobCreateParams,
  GenomeJobCreateResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-job-create', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeJobCreateResult> {
    const createParams = params as GenomeJobCreateParams;

    console.log('🧬 GENOME JOB CREATE: Starting fine-tuning job creation');
    console.log(`   PersonaId: ${createParams.personaId}`);
    console.log(`   Provider: ${createParams.provider}`);
    console.log(`   Base Model: ${createParams.configuration.model.baseModel}`);

    try {
      // 1. Validate required fields
      if (!createParams.personaId) {
        return transformPayload(params, {
          success: false,
          error: 'personaId is required'
        });
      }

      if (!createParams.provider) {
        return transformPayload(params, {
          success: false,
          error: 'provider is required'
        });
      }

      if (!createParams.configuration) {
        return transformPayload(params, {
          success: false,
          error: 'configuration is required'
        });
      }

      // 2. Validate provider
      const validProviders = ['openai', 'deepseek', 'fireworks', 'together'];
      if (!validProviders.includes(createParams.provider)) {
        return transformPayload(params, {
          success: false,
          error: `Invalid provider: ${createParams.provider}. Must be one of: ${validProviders.join(', ')}`
        });
      }

      // 3. Validate configuration against parameter ranges
      const warnings: string[] = [];
      const config = createParams.configuration;

      if (!createParams.skipValidation) {
        // Validate schedule parameters
        if (config.schedule.epochs < PARAMETER_RANGES.schedule.epochs.min ||
            config.schedule.epochs > PARAMETER_RANGES.schedule.epochs.max) {
          return transformPayload(params, {
            success: false,
            error: `epochs must be between ${PARAMETER_RANGES.schedule.epochs.min} and ${PARAMETER_RANGES.schedule.epochs.max}`
          });
        }

        if (config.schedule.batchSize < PARAMETER_RANGES.schedule.batchSize.min ||
            config.schedule.batchSize > PARAMETER_RANGES.schedule.batchSize.max) {
          return transformPayload(params, {
            success: false,
            error: `batchSize must be between ${PARAMETER_RANGES.schedule.batchSize.min} and ${PARAMETER_RANGES.schedule.batchSize.max}`
          });
        }

        if (config.schedule.sequenceLength < PARAMETER_RANGES.schedule.sequenceLength.min ||
            config.schedule.sequenceLength > PARAMETER_RANGES.schedule.sequenceLength.max) {
          return transformPayload(params, {
            success: false,
            error: `sequenceLength must be between ${PARAMETER_RANGES.schedule.sequenceLength.min} and ${PARAMETER_RANGES.schedule.sequenceLength.max}`
          });
        }

        // Validate optimizer parameters
        if (config.optimizer.learningRate < PARAMETER_RANGES.optimizer.learningRate.min ||
            config.optimizer.learningRate > PARAMETER_RANGES.optimizer.learningRate.max) {
          return transformPayload(params, {
            success: false,
            error: `learningRate must be between ${PARAMETER_RANGES.optimizer.learningRate.min} and ${PARAMETER_RANGES.optimizer.learningRate.max}`
          });
        }

        // Validate LoRA config if present
        if (config.method.loraConfig) {
          const loraConfig = config.method.loraConfig;

          if (loraConfig.rank < PARAMETER_RANGES.lora.rank.min ||
              loraConfig.rank > PARAMETER_RANGES.lora.rank.max) {
            return transformPayload(params, {
              success: false,
              error: `LoRA rank must be between ${PARAMETER_RANGES.lora.rank.min} and ${PARAMETER_RANGES.lora.rank.max}`
            });
          }

          if (loraConfig.alpha < PARAMETER_RANGES.lora.alpha.min ||
              loraConfig.alpha > PARAMETER_RANGES.lora.alpha.max) {
            return transformPayload(params, {
              success: false,
              error: `LoRA alpha must be between ${PARAMETER_RANGES.lora.alpha.min} and ${PARAMETER_RANGES.lora.alpha.max}`
            });
          }

          // Warning: Common mistake is alpha = rank (should be rank * 2)
          if (loraConfig.alpha === loraConfig.rank) {
            warnings.push('LoRA alpha equals rank. Consider setting alpha = rank * 2 for better results.');
          }
        }

        // Validate training file ID
        const trainingFileId = createParams.trainingFileId || config.datasets.trainingFileId;
        if (!trainingFileId) {
          return transformPayload(params, {
            success: false,
            error: 'trainingFileId is required (either in params or configuration.datasets)'
          });
        }
      }

      // 4. Create FineTuningJobEntity
      const jobId = uuidv4() as UUID;
      const now = Date.now();

      const jobEntity = new FineTuningJobEntity();
      jobEntity.id = jobId;
      jobEntity.personaId = createParams.personaId;
      jobEntity.provider = createParams.provider;
      jobEntity.providerJobId = `pending-${jobId.substring(0, 8)}`; // Temporary until provider creates job
      jobEntity.baseModel = config.model.baseModel;
      jobEntity.trainingFileId = createParams.trainingFileId || config.datasets.trainingFileId;
      jobEntity.validationFileId = (createParams.validationFileId ?? config.datasets.validationFileId) || null;
      jobEntity.configuration = config;
      jobEntity.status = 'queued';
      jobEntity.providerCreatedAt = now; // Set creation timestamp for validation
      jobEntity.createdAt = new Date(now);
      jobEntity.updatedAt = new Date(now);

      // Set metadata with creation context
      jobEntity.metadata = {
        createdBy: this.context.uuid,
        createdAt: now,
        configurationVersion: '1.0',
        ...(config.metadata || {})
      };

      // 5. Validate entity
      const validation = jobEntity.validate();
      if (!validation.success) {
        return transformPayload(params, {
          success: false,
          error: `Entity validation failed: ${validation.error}`
        });
      }

      // 6. Save to database
      const createResult = await Commands.execute<any, DataCreateResult<FineTuningJobEntity>>('data/create', {
        collection: COLLECTIONS.FINE_TUNING_JOBS,
        data: jobEntity
      });

      if (!createResult.success || !createResult.data) {
        return transformPayload(params, {
          success: false,
          error: createResult.error || 'Failed to create job entity in database'
        });
      }

      console.log(`✅ GENOME JOB CREATE: Job created with ID ${jobId}`);

      // 7. Estimate training duration (rough estimate based on epochs)
      // Real implementation would consider dataset size, batch size, GPU type
      const estimatedDuration = config.schedule.epochs * 600000; // 10 minutes per epoch (placeholder)

      // 8. Return job details
      return transformPayload(params, {
        success: true,
        job: {
          jobId: jobEntity.id,
          providerJobId: jobEntity.providerJobId,
          provider: jobEntity.provider,
          status: jobEntity.status,
          baseModel: jobEntity.baseModel,
          trainingFileId: jobEntity.trainingFileId,
          validationFileId: jobEntity.validationFileId,
          createdAt: jobEntity.createdAt.getTime(),
          estimatedDuration,
          configurationSummary: {
            method: config.method.type,
            epochs: config.schedule.epochs,
            batchSize: config.schedule.batchSize,
            learningRate: config.optimizer.learningRate,
            sequenceLength: config.schedule.sequenceLength
          }
        },
        warnings: warnings.length > 0 ? warnings : undefined
      });

    } catch (error) {
      console.error('❌ GENOME JOB CREATE: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
