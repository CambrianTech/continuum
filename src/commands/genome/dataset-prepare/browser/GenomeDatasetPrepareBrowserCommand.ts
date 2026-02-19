/**
 * Genome Dataset Prepare Command - Browser Implementation
 *
 * Collect training data from chat history for a persona and export as JSONL dataset for LoRA fine-tuning
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeDatasetPrepareParams, GenomeDatasetPrepareResult } from '../shared/GenomeDatasetPrepareTypes';

export class GenomeDatasetPrepareBrowserCommand extends CommandBase<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/dataset-prepare', context, subpath, commander);
  }

  async execute(params: GenomeDatasetPrepareParams): Promise<GenomeDatasetPrepareResult> {
    console.log('🌐 BROWSER: Delegating Genome Dataset Prepare to server');
    return await this.remoteExecute(params);
  }
}
