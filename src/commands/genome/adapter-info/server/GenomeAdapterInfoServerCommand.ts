/**
 * Genome Adapter Info Command - Server Implementation
 *
 * Returns detailed info about a specific adapter: manifest, architecture, compatibility.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeAdapterInfoParams, GenomeAdapterInfoResult, AdapterArchitectureInfo } from '../shared/GenomeAdapterInfoTypes';
import { createGenomeAdapterInfoResultFromParams } from '../shared/GenomeAdapterInfoTypes';
import { AdapterStore } from '@system/genome/server/AdapterStore';

export class GenomeAdapterInfoServerCommand extends CommandBase<GenomeAdapterInfoParams, GenomeAdapterInfoResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-info', context, subpath, commander);
  }

  async execute(params: GenomeAdapterInfoParams): Promise<GenomeAdapterInfoResult> {
    if (!params.name || params.name.trim() === '') {
      throw new ValidationError(
        'name',
        `Missing required parameter 'name'. Provide an adapter name to inspect.`
      );
    }

    const all = AdapterStore.discoverAll();
    const adapter = all.find(a =>
      a.manifest.name === params.name ||
      a.manifest.id === params.name ||
      path.basename(a.dirPath) === params.name
    );

    if (!adapter) {
      throw new ValidationError(
        'name',
        `Adapter '${params.name}' not found. Use 'genome/adapter-list' to see available adapters.`
      );
    }

    const m = adapter.manifest;

    // Read adapter_config.json for architecture details
    let architecture: AdapterArchitectureInfo | undefined;
    const configPath = path.join(adapter.dirPath, 'adapter_config.json');
    if (fs.existsSync(configPath)) {
      try {
        const config = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
        architecture = {
          peftType: config.peft_type || 'LORA',
          rank: config.r || m.rank,
          loraAlpha: config.lora_alpha || m.rank,
          loraDropout: config.lora_dropout || 0,
          targetModules: config.target_modules || [],
          bias: config.bias || 'none',
        };
      } catch {
        // Corrupt config — skip architecture info
      }
    }

    return createGenomeAdapterInfoResultFromParams(params, {
      success: true,
      name: m.name,
      domain: m.traitType,
      sizeMB: m.sizeMB,
      baseModel: m.baseModel,
      createdAt: m.createdAt,
      dirPath: adapter.dirPath,
      personaId: m.personaId,
      personaName: m.personaName,
      hasWeights: adapter.hasWeights,
      isActive: false,  // TODO: cross-reference with running PersonaGenome instances
      trainingInfo: m.trainingMetadata ? {
        epochs: m.trainingMetadata.epochs,
        loss: m.trainingMetadata.loss,
        performance: m.trainingMetadata.performance,
        trainingDurationMs: m.trainingMetadata.trainingDuration,
        datasetHash: m.trainingMetadata.datasetHash,
      } : undefined,
      architecture,
      compatibility: {
        baseModel: m.baseModel,
        quantizationEnabled: m.quantization?.enabled ?? false,
        quantizationBits: m.quantization?.bits,
        quantizationType: m.quantization?.type,
      },
    });
  }
}
