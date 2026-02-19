/**
 * Genome Activate Command - Server Implementation
 *
 * Activates adapter for persona using GenomeDaemon.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeActivateParams, GenomeActivateResult } from '../shared/GenomeActivateTypes';
import { createGenomeActivateResultFromParams } from '../shared/GenomeActivateTypes';
import { GenomeDaemon } from '../../../../system/genome/server/GenomeDaemon';

/**
 * Get genome daemon singleton
 */
function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,          // 8GB GPU memory (adjust for your hardware)
    defaultPersonaQuotaMB: 1024,  // 1GB per persona default
    hysteresisSeconds: 60,        // 60s thrashing protection
    enableThrashingProtection: true
  });
}

export class GenomeActivateServerCommand extends CommandBase<GenomeActivateParams, GenomeActivateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-activate', context, subpath, commander);
  }

  async execute(params: GenomeActivateParams): Promise<GenomeActivateResult> {
    try {
      const daemon = getDaemon();

      // Auto-register persona if not already known to the genome daemon
      daemon.ensurePersonaRegistered(params.personaId);

      const loaded = await daemon.loadAdapter(params.personaId, params.adapterId);

      if (!loaded) {
        return createGenomeActivateResultFromParams(params, {
          success: true,
          loaded: false,
          thrashingDetected: true
        });
      }

      // Get eviction stats (simplified - would need to track this in daemon)
      const stats = daemon.getStats();

      return createGenomeActivateResultFromParams(params, {
        success: true,
        loaded: true,
        thrashingDetected: false,
        loadTimeMs: stats.averageLoadTimeMs
      });
    } catch (error) {
      return createGenomeActivateResultFromParams(params, {
        success: false,
        loaded: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
