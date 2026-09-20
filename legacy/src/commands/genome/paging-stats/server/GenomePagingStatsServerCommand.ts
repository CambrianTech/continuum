/**
 * Genome Stats Command - Server Implementation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeStatsParams, GenomeStatsResult } from '../shared/GenomeStatsTypes';
import { createGenomeStatsResultFromParams } from '../shared/GenomeStatsTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';

function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,
    defaultPersonaQuotaMB: 1024,
    hysteresisSeconds: 60,
    enableThrashingProtection: true
  });
}

export class GenomePagingStatsServerCommand extends CommandBase<GenomeStatsParams, GenomeStatsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-paging-stats', context, subpath, commander);
  }

  async execute(params: GenomeStatsParams): Promise<GenomeStatsResult> {
    try {
      const daemon = getDaemon();
      const stats = daemon.getStats();
      const config = daemon.getConfig();

      // Get personas
      const allPersonas = daemon.listPersonas();
      const personasToShow = params.personaId
        ? allPersonas.filter(p => p.getPersonaId() === params.personaId)
        : allPersonas;

      const personas = personasToShow.map(state => ({
        personaId: state.getPersonaId(),
        displayName: state.getDisplayName(),
        memoryUsedMB: state.getMemoryUsed(),
        memoryQuotaMB: state.getMemoryQuota(),
        memoryUtilization: state.getMemoryUtilization(),
        activeAdapters: state.getActiveAdapters(),
        priority: state.getPriority(),
        loadCount: state.getLoadCount(),
        evictionCount: state.getEvictionCount(),
        lastActivatedAt: state.getLastActivatedAt()
      }));

      return createGenomeStatsResultFromParams(params, {
        success: true,
        globalMemoryUsedMB: stats.currentMemoryUsedMB,
        globalMemoryTotalMB: config.totalMemoryMB,
        globalMemoryUtilization: stats.currentMemoryUtilization,
        totalLoadCount: stats.totalLoadCount,
        totalEvictionCount: stats.totalEvictionCount,
        totalThrashingEvents: stats.totalThrashingEvents,
        averageLoadTimeMs: stats.averageLoadTimeMs,
        personas
      });
    } catch (error) {
      return createGenomeStatsResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
