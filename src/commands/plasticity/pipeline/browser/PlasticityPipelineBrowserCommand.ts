/**
 * Plasticity Pipeline Command - Browser Implementation
 *
 * End-to-end plasticity pipeline: gate_gradients.json → analysis → compaction. The 'wake up to a compacted model' command. Given a gate capture directory and a model path, runs the full pipeline: load gradients, compute optimization plan, build topology, compact model (multi-shard aware), write compacted model + topology + analysis.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PlasticityPipelineParams, PlasticityPipelineResult } from '../shared/PlasticityPipelineTypes';

export class PlasticityPipelineBrowserCommand extends CommandBase<PlasticityPipelineParams, PlasticityPipelineResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/pipeline', context, subpath, commander);
  }

  async execute(params: PlasticityPipelineParams): Promise<PlasticityPipelineResult> {
    console.log('🌐 BROWSER: Delegating Plasticity Pipeline to server');
    return await this.remoteExecute(params);
  }
}
