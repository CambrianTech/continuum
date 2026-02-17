/**
 * Genome Train Command - Server Implementation
 *
 * Loads JSONL dataset, validates Python environment, runs PEFT LoRA training,
 * returns adapter path and training metrics.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeTrainParams, GenomeTrainResult } from '../shared/GenomeTrainTypes';
import { createGenomeTrainResultFromParams } from '../shared/GenomeTrainTypes';
import { TrainingDatasetBuilder } from '@system/genome/fine-tuning/server/TrainingDatasetBuilder';
import { PEFTLoRAAdapter } from '@system/genome/fine-tuning/server/adapters/PEFTLoRAAdapter';
import { AdapterPackage } from '@system/genome/server/AdapterPackage';
import { GenomeLayerEntity } from '@system/genome/entities/GenomeLayerEntity';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Logger } from '@system/core/logging/Logger';
import { LOCAL_MODELS } from '@system/shared/Constants';

export class GenomeTrainServerCommand extends CommandBase<GenomeTrainParams, GenomeTrainResult> {
  private readonly log = Logger.create('genome/train', 'genome');

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train', context, subpath, commander);
  }

  async execute(params: GenomeTrainParams): Promise<GenomeTrainResult> {
    const { personaId, personaName, traitType, datasetPath } = params;
    const baseModel = params.baseModel ?? LOCAL_MODELS.DEFAULT;

    this.log.info(`GENOME TRAIN: persona=${personaName}, model=${baseModel}, dataset=${datasetPath}`);

    if (!personaId) {
      throw new ValidationError('personaId', 'Missing required parameter. See genome/train README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/train README.');
    }
    if (!traitType) {
      throw new ValidationError('traitType', 'Missing required parameter. See genome/train README.');
    }
    if (!datasetPath) {
      throw new ValidationError('datasetPath', 'Missing required parameter. See genome/train README.');
    }

    // 1. Validate Python environment
    const adapter = new PEFTLoRAAdapter();
    if (!adapter.supportsFineTuning()) {
      return createGenomeTrainResultFromParams(params, {
        success: false,
        error: 'PEFT training environment not available. Run bootstrap script first.',
        adapterPath: '',
        metrics: { finalLoss: 0, trainingTime: 0, examplesProcessed: 0, epochs: 0 },
      });
    }

    // 2. Load dataset from JSONL
    const dataset = await TrainingDatasetBuilder.loadFromJSONL(datasetPath, {
      personaId,
      personaName,
      traitType,
    });

    if (dataset.examples.length === 0) {
      return createGenomeTrainResultFromParams(params, {
        success: false,
        error: 'Dataset is empty — no training examples found in JSONL file',
        adapterPath: '',
        metrics: { finalLoss: 0, trainingTime: 0, examplesProcessed: 0, epochs: 0 },
      });
    }

    // 3. Validate dataset quality
    const validation = TrainingDatasetBuilder.validateDataset(dataset);
    if (!validation.valid) {
      return createGenomeTrainResultFromParams(params, {
        success: false,
        error: `Dataset validation failed: ${validation.errors.join('; ')}`,
        adapterPath: '',
        metrics: { finalLoss: 0, trainingTime: 0, examplesProcessed: 0, epochs: 0 },
      });
    }

    this.log.info(`Loaded ${dataset.examples.length} examples, starting training...`);

    // 4. Build training request and execute
    const result = await adapter.trainLoRA({
      personaId,
      personaName,
      traitType,
      baseModel,
      dataset,
      rank: params.rank ?? 32,
      epochs: params.epochs ?? 3,
      learningRate: params.learningRate ?? 0.0001,
      batchSize: params.batchSize ?? 4,
      quantize: params.quantize ?? true,
      quantizeBits: params.quantizeBits ?? 4,
    });

    if (!result.success) {
      return createGenomeTrainResultFromParams(params, {
        success: false,
        error: result.error ?? 'Training failed',
        adapterPath: '',
        metrics: { finalLoss: 0, trainingTime: 0, examplesProcessed: 0, epochs: 0 },
      });
    }

    const adapterPath = result.modelPath ?? '';
    this.log.info(`Adapter saved to ${adapterPath}`);

    // Create GenomeLayerEntity and persist to database
    let layerId: UUID | undefined;
    if (result.manifest) {
      try {
        const entity = AdapterPackage.toGenomeLayerEntity(result.manifest, adapterPath);
        await DataCreate.execute({
          collection: GenomeLayerEntity.collection,
          data: entity,
        });
        layerId = entity.id;
        this.log.info(`GenomeLayerEntity created: ${layerId}`);
      } catch (error) {
        this.log.warn(`Failed to persist GenomeLayerEntity: ${error}`);
        // Training succeeded — don't fail the whole operation for persistence issues
      }
    }

    return createGenomeTrainResultFromParams(params, {
      success: true,
      adapterPath,
      layerId,
      metrics: {
        finalLoss: result.metrics?.finalLoss ?? 0,
        trainingTime: result.metrics?.trainingTime ?? 0,
        examplesProcessed: result.metrics?.examplesProcessed ?? dataset.examples.length,
        epochs: result.metrics?.epochs ?? (params.epochs ?? 3),
      },
    });
  }
}
