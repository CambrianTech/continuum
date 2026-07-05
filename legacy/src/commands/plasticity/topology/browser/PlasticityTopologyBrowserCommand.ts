/**
 * Plasticity Topology Command - Browser Implementation
 *
 * Read the head topology of a compacted model. Returns per-layer head precision assignments showing which heads were pruned, quantized to different levels, or kept at full precision. Use this to inspect what compaction did to a model.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PlasticityTopologyParams, PlasticityTopologyResult } from '../shared/PlasticityTopologyTypes';

export class PlasticityTopologyBrowserCommand extends CommandBase<PlasticityTopologyParams, PlasticityTopologyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/topology', context, subpath, commander);
  }

  async execute(params: PlasticityTopologyParams): Promise<PlasticityTopologyResult> {
    console.log('🌐 BROWSER: Delegating Plasticity Topology to server');
    return await this.remoteExecute(params);
  }
}
