/**
 * Plasticity Compact Command - Server Implementation
 * Routes to Rust plasticity module via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PlasticityCompactParams, PlasticityCompactResult } from '../shared/PlasticityCompactTypes';
import { createPlasticityCompactResultFromParams } from '../shared/PlasticityCompactTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class PlasticityCompactServerCommand extends CommandBase<PlasticityCompactParams, PlasticityCompactResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/compact', context, subpath, commander);
  }

  async execute(params: PlasticityCompactParams): Promise<PlasticityCompactResult> {
    if (!params.adapterPath) {
      throw new ValidationError('adapterPath', 'Required: path to adapter directory containing gate_gradients.json');
    }
    if (!params.modelPath) {
      throw new ValidationError('modelPath', 'Required: path to base model safetensors directory');
    }

    const client = RustCoreIPCClient.getInstance();
    const result = await client.plasticityCompact({
      adapterPath: params.adapterPath,
      modelPath: params.modelPath,
      outputPath: params.outputPath,
      config: params.config as Record<string, unknown> as import('../../../../../core/continuum-core/bindings/modules/plasticity').PlasticityCompactParams['config'],
    });

    return createPlasticityCompactResultFromParams(params, {
      success: true,
      modelPath: result.modelPath,
      topologyPath: result.topologyPath,
      originalSizeBytes: result.originalSizeBytes,
      compactedSizeBytes: result.compactedSizeBytes,
    });
  }
}
