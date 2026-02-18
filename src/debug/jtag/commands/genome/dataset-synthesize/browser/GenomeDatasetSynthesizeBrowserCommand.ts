/**
 * Genome Dataset Synthesize Command - Browser Implementation
 *
 * Delegates to server for LLM-based training data synthesis.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult } from '../shared/GenomeDatasetSynthesizeTypes';

export class GenomeDatasetSynthesizeBrowserCommand extends CommandBase<GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/dataset-synthesize', context, subpath, commander);
  }

  async execute(params: GenomeDatasetSynthesizeParams): Promise<GenomeDatasetSynthesizeResult> {
    console.log('🌐 BROWSER: Delegating Genome Dataset Synthesize to server');
    return await this.remoteExecute(params);
  }
}
