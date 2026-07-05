/**
 * Plasticity Pipeline Command - Server Implementation
 * Routes to Rust plasticity module via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PlasticityPipelineParams, PlasticityPipelineResult } from '../shared/PlasticityPipelineTypes';
import { createPlasticityPipelineResultFromParams } from '../shared/PlasticityPipelineTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class PlasticityPipelineServerCommand extends CommandBase<PlasticityPipelineParams, PlasticityPipelineResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/pipeline', context, subpath, commander);
  }

  async execute(params: PlasticityPipelineParams): Promise<PlasticityPipelineResult> {
    if (!params.capturePath) {
      throw new ValidationError('capturePath', 'Required: gate capture directory containing gate_gradients.json');
    }
    if (!params.modelPath) {
      throw new ValidationError('modelPath', 'Required: base model path (directory for multi-shard, file for single)');
    }

    const client = RustCoreIPCClient.getInstance();
    const result = await client.plasticityPipeline({
      capturePath: params.capturePath,
      modelPath: params.modelPath,
      outputPath: params.outputPath,
      config: params.config as Record<string, unknown> as import('../../../../../core/continuum-core/bindings/modules/plasticity').PlasticityPipelineParams['config'],
    });

    return createPlasticityPipelineResultFromParams(params, {
      success: true,
      modelPath: result.modelPath,
      topologyPath: result.topologyPath,
      originalSizeBytes: result.originalSizeBytes,
      compactedSizeBytes: result.compactedSizeBytes,
    });
  }
}
