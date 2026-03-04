/**
 * Genome Layers Command - Server Implementation
 *
 * Queries AdapterStore for a persona's real LoRA adapter stack.
 * Returns actual adapter info — no fakes, no demos.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeLayersParams, GenomeLayersResult } from '../shared/GenomeLayersTypes';
import { createGenomeLayersResultFromParams } from '../shared/GenomeLayersTypes';
import { AdapterStore } from '../../../../system/genome/server/AdapterStore';

/** Max adapters before fitness saturates at 1.0 */
const MAX_EXPECTED_ADAPTERS = 10;

export class GenomeLayersServerCommand extends CommandBase<GenomeLayersParams, GenomeLayersResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-layers', context, subpath, commander);
  }

  async execute(params: GenomeLayersParams): Promise<GenomeLayersResult> {
    try {
      const { personaId } = params;

      if (!personaId) {
        return createGenomeLayersResultFromParams(params, {
          success: false,
          error: 'personaId is required'
        });
      }

      const adapters = AdapterStore.discoverForPersona(personaId);

      const layers = adapters.map(a => ({
        name: a.manifest.name,
        domain: a.manifest.traitType,
        hasWeights: a.hasWeights,
        baseModel: a.manifest.baseModel
      }));

      const trainedCount = adapters.filter(a => a.hasWeights).length;
      const fitness = Math.min(1.0, trainedCount / MAX_EXPECTED_ADAPTERS);

      return createGenomeLayersResultFromParams(params, {
        success: true,
        layers,
        fitness
      });
    } catch (error) {
      return createGenomeLayersResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
