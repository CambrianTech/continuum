/**
 * Plasticity Compress Command - Server Implementation
 *
 * Compress a model using utilization-aware head pruning + mixed quantization.
 * Routes to Rust plasticity module via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PlasticityCompressParams, PlasticityCompressResult } from '../shared/PlasticityCompressTypes';
import { createPlasticityCompressResultFromParams } from '../shared/PlasticityCompressTypes';

export class PlasticityCompressServerCommand extends CommandBase<PlasticityCompressParams, PlasticityCompressResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/compress', context, subpath, commander);
  }

  async execute(params: PlasticityCompressParams): Promise<PlasticityCompressResult> {
    if (!params.capturePath || params.capturePath.trim() === '') {
      throw new ValidationError(
        'capturePath',
        `Missing required parameter 'capturePath'. Path to gradient capture directory ` +
        `(from plasticity/analyze). See the plasticity/compress README for usage.`
      );
    }

    if (!params.modelPath || params.modelPath.trim() === '') {
      throw new ValidationError(
        'modelPath',
        `Missing required parameter 'modelPath'. Path to base model safetensors ` +
        `or HuggingFace model ID. See the plasticity/compress README for usage.`
      );
    }

    // Route to Rust plasticity module via IPC
    const ipcResult = await this.executeRustCommand('plasticity/compress', {
      capturePath: params.capturePath,
      modelPath: params.modelPath,
      deviceSpec: params.deviceSpec || '32gb',
      outputPath: params.outputPath || '',
      architecture: params.architecture || '',
    });

    return createPlasticityCompressResultFromParams(params, {
      success: true,
      ggufPath: ipcResult.ggufPath || '',
      outputSizeGb: ipcResult.outputSizeGb || 0,
      compressionRatio: ipcResult.compressionRatio || 0,
      quantDistribution: ipcResult.quantDistribution || {},
      verified: ipcResult.verified || false,
    });
  }
}
