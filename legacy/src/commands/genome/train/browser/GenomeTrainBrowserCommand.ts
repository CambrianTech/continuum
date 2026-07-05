/**
 * Genome Train Command - Browser Implementation
 *
 * Execute LoRA fine-tuning on a JSONL dataset using PEFTLoRAAdapter. Wraps trainLoRA() as a command for Sentinel pipeline orchestration
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainParams, GenomeTrainResult } from '../shared/GenomeTrainTypes';

export class GenomeTrainBrowserCommand extends CommandBase<GenomeTrainParams, GenomeTrainResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train', context, subpath, commander);
  }

  async execute(params: GenomeTrainParams): Promise<GenomeTrainResult> {
    console.log('🌐 BROWSER: Delegating Genome Train to server');
    return await this.remoteExecute(params);
  }
}
