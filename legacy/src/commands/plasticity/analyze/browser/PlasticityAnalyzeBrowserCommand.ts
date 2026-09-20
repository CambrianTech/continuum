/**
 * Plasticity Analyze Command - Browser Implementation
 *
 * Dry-run analysis of what compaction would do to a model. Reads gate_gradients.json from the adapter directory, computes per-head utilization scores, and returns a topology showing which heads would be pruned/compressed/kept. Does NOT modify any files.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PlasticityAnalyzeParams, PlasticityAnalyzeResult } from '../shared/PlasticityAnalyzeTypes';

export class PlasticityAnalyzeBrowserCommand extends CommandBase<PlasticityAnalyzeParams, PlasticityAnalyzeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/analyze', context, subpath, commander);
  }

  async execute(params: PlasticityAnalyzeParams): Promise<PlasticityAnalyzeResult> {
    console.log('🌐 BROWSER: Delegating Plasticity Analyze to server');
    return await this.remoteExecute(params);
  }
}
