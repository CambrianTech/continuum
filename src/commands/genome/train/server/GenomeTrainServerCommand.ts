/**
 * Genome Train Command - Server Implementation
 *
 * Two modes:
 *   sync (default):  Blocks until training completes, returns adapter + metrics.
 *                    Used by sentinel pipeline command steps that need the result.
 *   async:           Returns sentinel handle immediately, training runs in background.
 *                    Used by CLI/widget callers that want non-blocking + real-time events.
 *                    TrainingCompletionHandler processes the result when done.
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
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { sentinelEventBridge } from '@system/sentinel/SentinelEventBridge';
import { registerSentinelHandle } from '@system/sentinel/SentinelEscalationService';
import { registerTrainingCompletion } from '@system/genome/server/TrainingCompletionHandler';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Logger } from '@system/core/logging/Logger';
import { LOCAL_MODELS } from '@system/shared/Constants';
import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';

export class GenomeTrainServerCommand extends CommandBase<GenomeTrainParams, GenomeTrainResult> {
  private readonly log = Logger.create('genome/train', 'genome');

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train', context, subpath, commander);
  }

  async execute(params: GenomeTrainParams): Promise<GenomeTrainResult> {
    const { personaId, personaName, traitType, datasetPath } = params;
    const baseModel = params.baseModel ?? LOCAL_MODELS.DEFAULT;
    const asyncMode = (params as any).async === true;

    this.log.info(`GENOME TRAIN: persona=${personaName}, model=${baseModel}, dataset=${datasetPath}, async=${asyncMode}`);

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

    this.log.info(`Loaded ${dataset.examples.length} examples`);

    // ── ASYNC MODE: fire-and-forget, return handle immediately ──────────────
    if (asyncMode) {
      return this._executeAsync(params, adapter, dataset, personaId, personaName, traitType, baseModel);
    }

    // ── SYNC MODE: block until complete (default, backward compatible) ──────
    return this._executeSync(params, adapter, dataset, personaId, personaName, traitType, baseModel);
  }

  /**
   * Async mode: start training sentinel, register for completion handling, return handle.
   * Post-training work (adapter save, entity creation) runs in TrainingCompletionHandler.
   */
  private async _executeAsync(
    params: GenomeTrainParams,
    adapter: PEFTLoRAAdapter,
    dataset: ReturnType<typeof TrainingDatasetBuilder.loadFromJSONL> extends Promise<infer T> ? T : never,
    personaId: UUID,
    personaName: string,
    traitType: string,
    baseModel: string,
  ): Promise<GenomeTrainResult> {
    // Prepare training files (same as sync path)
    const hfModelName = LOCAL_MODELS.mapToHuggingFace(baseModel);
    const datasetTempPath = await adapter.exportDatasetForAsync(dataset);
    const configPath = await adapter.createConfigForAsync(
      { ...params, baseModel: hfModelName } as any,
      datasetTempPath,
    );
    const outputDir = path.join(os.tmpdir(), `jtag-training-${Date.now()}`);
    await fs.promises.mkdir(outputDir, { recursive: true });

    // Get script paths from adapter
    const wrapperPath = adapter.wrapperPath;
    const scriptPath = adapter.scriptPath;

    // Start training via Rust sentinel (returns immediately)
    const rustClient = RustCoreIPCClient.getInstance();
    const runResult = await rustClient.sentinelRun({
      command: wrapperPath,
      args: [scriptPath, '--config', configPath, '--output', outputDir],
      workingDir: process.cwd(),
      timeout: 600,
      type: 'training',
    });

    const handle = runResult.handle;
    this.log.info(`Async training started: handle=${handle}`);

    // Register with event bridge (polls Rust, emits TypeScript Events)
    sentinelEventBridge.watch(handle, 'training', {
      personaId,
      personaName,
      traitType,
      baseModel,
    });

    // Register with escalation service (routes completion to persona inbox)
    registerSentinelHandle(
      handle,
      '', // No entity ID for ad-hoc training
      personaId,
      undefined,
      `genome-train-${personaName}-${traitType}`,
    );

    // Register completion context (TrainingCompletionHandler will process when done)
    registerTrainingCompletion({
      handle,
      personaId,
      personaName,
      traitType,
      baseModel,
      rank: params.rank ?? 32,
      epochs: params.epochs ?? 3,
      exampleCount: dataset.examples.length,
      outputDir,
      datasetPath: datasetTempPath,
      configPath,
      startTime: Date.now(),
    });

    return createGenomeTrainResultFromParams(params, {
      success: true,
      adapterPath: '', // Not yet known — will be set by completion handler
      sentinelHandle: handle,
      metrics: {
        finalLoss: 0,
        trainingTime: 0,
        examplesProcessed: dataset.examples.length,
        epochs: params.epochs ?? 3,
      },
    });
  }

  /**
   * Sync mode: block until training completes, return full result.
   * Used by sentinel pipeline command steps that need the result immediately.
   */
  private async _executeSync(
    params: GenomeTrainParams,
    adapter: PEFTLoRAAdapter,
    dataset: ReturnType<typeof TrainingDatasetBuilder.loadFromJSONL> extends Promise<infer T> ? T : never,
    personaId: UUID,
    personaName: string,
    traitType: string,
    baseModel: string,
  ): Promise<GenomeTrainResult> {
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
    this.log.info(`Adapter saved to ${adapterPath} (sentinel=${result.sentinelHandle})`);

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
      }
    }

    return createGenomeTrainResultFromParams(params, {
      success: true,
      adapterPath,
      layerId,
      sentinelHandle: result.sentinelHandle,
      metrics: {
        finalLoss: result.metrics?.finalLoss ?? 0,
        trainingTime: result.metrics?.trainingTime ?? 0,
        examplesProcessed: result.metrics?.examplesProcessed ?? dataset.examples.length,
        epochs: result.metrics?.epochs ?? (params.epochs ?? 3),
      },
    });
  }
}
