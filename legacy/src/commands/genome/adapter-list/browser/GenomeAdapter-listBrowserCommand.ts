/**
 * Genome Adapter List Command - Browser Implementation
 *
 * List all LoRA adapters in the genome with sizes, domains, last-used timestamps, and cascade scores. Shows both on-disk (available) and loaded (active) adapters.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterListParams, GenomeAdapterListResult } from '../shared/GenomeAdapterListTypes';

export class GenomeAdapterListBrowserCommand extends CommandBase<GenomeAdapterListParams, GenomeAdapterListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-list', context, subpath, commander);
  }

  async execute(params: GenomeAdapterListParams): Promise<GenomeAdapterListResult> {
    console.log('🌐 BROWSER: Delegating Genome Adapter List to server');
    return await this.remoteExecute(params);
  }
}
