/**
 * Genome Training Pipeline Command - Server Implementation
 *
 * Builds a LoRA training pipeline via buildLoRATrainingPipeline(),
 * forwards to sentinel/run for background execution, returns handle.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeTrainingPipelineParams, GenomeTrainingPipelineResult } from '../shared/GenomeTrainingPipelineTypes';
import { createGenomeTrainingPipelineResultFromParams } from '../shared/GenomeTrainingPipelineTypes';
import { buildLoRATrainingPipeline } from '@system/sentinel/pipelines/LoRATrainingPipeline';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GenomeTrainingPipelineServerCommand extends CommandBase<GenomeTrainingPipelineParams, GenomeTrainingPipelineResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-pipeline', context, subpath, commander);
  }

  async execute(params: GenomeTrainingPipelineParams): Promise<GenomeTrainingPipelineResult> {
    const { personaId, personaName, roomId } = params;

    console.log(`🧬 TRAINING PIPELINE: persona=${personaName}, room=${roomId}`);

    if (!personaId) {
      throw new ValidationError('personaId', 'Missing required parameter. See genome/training-pipeline README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/training-pipeline README.');
    }
    if (!roomId) {
      throw new ValidationError('roomId', 'Missing required parameter. See genome/training-pipeline README.');
    }

    // 1. Build pipeline definition
    const pipeline = buildLoRATrainingPipeline({
      personaId,
      personaName,
      roomId,
      traitType: params.traitType,
      baseModel: params.baseModel,
      rank: params.rank,
      epochs: params.epochs,
      learningRate: params.learningRate,
      batchSize: params.batchSize,
    });

    const pipelineName = pipeline.name ?? 'lora-training';

    // 2. Forward to Rust sentinel for background execution
    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.sentinelRun({
      type: 'pipeline',
      command: 'pipeline',
      args: [],
      env: { PIPELINE_JSON: JSON.stringify(pipeline) },
    });

    console.log(`✅ TRAINING PIPELINE: Started as ${result.handle}`);

    return createGenomeTrainingPipelineResultFromParams(params, {
      success: true,
      handle: result.handle,
      pipelineName,
    });
  }
}
