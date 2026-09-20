/**
 * Plasticity Compress Command - Server Implementation
 * Routes to Rust plasticity module via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PlasticityCompressParams, PlasticityCompressResult } from '../shared/PlasticityCompressTypes';
import { createPlasticityCompressResultFromParams } from '../shared/PlasticityCompressTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class PlasticityCompressServerCommand extends CommandBase<PlasticityCompressParams, PlasticityCompressResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/compress', context, subpath, commander);
  }

  async execute(params: PlasticityCompressParams): Promise<PlasticityCompressResult> {
    const client = RustCoreIPCClient.getInstance();

    const result = await client.plasticityCompress({
      capturePath: params.capturePath,
      modelPath: params.modelPath,
      deviceSpec: params.deviceSpec,
      outputPath: params.outputPath,
      architecture: params.architecture,
    });

    return createPlasticityCompressResultFromParams(params, {
      success: true,
      ggufPath: result.ggufPath,
      outputSizeGb: result.outputSizeBytes / (1024 * 1024 * 1024),
      compressionRatio: result.compressionRatio,
      quantDistribution: result.recipe,
      verified: result.verified,
    });
  }
}
