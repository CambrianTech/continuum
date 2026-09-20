/**
 * Plasticity Compact Command - Browser Implementation
 *
 * Physically remove pruned heads from a model's safetensors. Reads gate_gradients.json from adapter directory, computes which heads to prune, then slices Q/K/V/O projection weights to remove dead heads. Produces a smaller model with fewer parameters. Handles both single-file and multi-shard models.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PlasticityCompactParams, PlasticityCompactResult } from '../shared/PlasticityCompactTypes';

export class PlasticityCompactBrowserCommand extends CommandBase<PlasticityCompactParams, PlasticityCompactResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/compact', context, subpath, commander);
  }

  async execute(params: PlasticityCompactParams): Promise<PlasticityCompactResult> {
    console.log('🌐 BROWSER: Delegating Plasticity Compact to server');
    return await this.remoteExecute(params);
  }
}
