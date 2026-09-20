/**
 * Genome Adapter List Command - Server Implementation
 *
 * Lists all LoRA adapters via AdapterStore (filesystem single source of truth).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterListParams, GenomeAdapterListResult, AdapterListEntry } from '../shared/GenomeAdapterListTypes';
import { createGenomeAdapterListResultFromParams } from '../shared/GenomeAdapterListTypes';
import { AdapterStore, type DiscoveredAdapter } from '@system/genome/server/AdapterStore';

export class GenomeAdapterListServerCommand extends CommandBase<GenomeAdapterListParams, GenomeAdapterListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-list', context, subpath, commander);
  }

  async execute(params: GenomeAdapterListParams): Promise<GenomeAdapterListResult> {
    let discovered: DiscoveredAdapter[];

    if (params.personaId) {
      discovered = AdapterStore.discoverForPersona(params.personaId);
    } else {
      discovered = AdapterStore.discoverAll();
    }

    if (params.domain) {
      discovered = discovered.filter(a => a.manifest.traitType === params.domain);
    }

    const sortBy = params.sortBy || 'name';
    discovered.sort((a, b) => {
      switch (sortBy) {
        case 'size': return b.manifest.sizeMB - a.manifest.sizeMB;
        case 'lastUsed':
        case 'created': return new Date(b.manifest.createdAt).getTime() - new Date(a.manifest.createdAt).getTime();
        case 'domain': return a.manifest.traitType.localeCompare(b.manifest.traitType);
        default: return a.manifest.name.localeCompare(b.manifest.name);
      }
    });

    const adapters: AdapterListEntry[] = discovered.map(a => ({
      name: a.manifest.name,
      domain: a.manifest.traitType,
      sizeMB: a.manifest.sizeMB,
      baseModel: a.manifest.baseModel,
      personaId: a.manifest.personaId,
      personaName: a.manifest.personaName,
      createdAt: a.manifest.createdAt,
      hasWeights: a.hasWeights,
      isActive: false,  // TODO: cross-reference with running PersonaGenome instances
      rank: a.manifest.rank,
      ...(params.includeMetrics && a.manifest.trainingMetadata ? {
        loss: a.manifest.trainingMetadata.loss,
        epochs: a.manifest.trainingMetadata.epochs,
      } : {}),
    }));

    const totalSizeMB = adapters.reduce((sum, a) => sum + a.sizeMB, 0);

    return createGenomeAdapterListResultFromParams(params, {
      success: true,
      adapters,
      totalCount: adapters.length,
      totalSizeMB: Math.round(totalSizeMB * 100) / 100,
      activeCount: adapters.filter(a => a.isActive).length,
    });
  }
}
