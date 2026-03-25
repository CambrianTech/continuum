/**
 * Plasticity Analyze Command - Server Implementation
 * Routes to Rust plasticity module via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PlasticityAnalyzeParams, PlasticityAnalyzeResult } from '../shared/PlasticityAnalyzeTypes';
import { createPlasticityAnalyzeResultFromParams } from '../shared/PlasticityAnalyzeTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class PlasticityAnalyzeServerCommand extends CommandBase<PlasticityAnalyzeParams, PlasticityAnalyzeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/analyze', context, subpath, commander);
  }

  async execute(params: PlasticityAnalyzeParams): Promise<PlasticityAnalyzeResult> {
    if (!params.adapterPath) {
      throw new ValidationError('adapterPath', 'Required: path to adapter directory containing gate_gradients.json');
    }

    const client = RustCoreIPCClient.getInstance();
    const result = await client.plasticityAnalyze({
      adapterPath: params.adapterPath,
      config: params.config as Record<string, unknown> as import('../../../../workers/continuum-core/bindings/modules/plasticity').PlasticityAnalyzeParams['config'],
    });

    return createPlasticityAnalyzeResultFromParams(params, {
      success: true,
      topology: result.topology,
      layerSummaries: result.layerSummaries,
      estimatedSavingsBytes: result.estimatedSavingsBytes,
      saturatedHeads: result.saturatedHeads,
    });
  }
}
