/**
 * Genome Adapter Prune Command - Browser Implementation
 *
 * Prune unused LoRA adapters to reclaim disk space. Removes adapters not used since the specified cutoff date. Supports dry-run mode to preview what would be deleted.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterPruneParams, GenomeAdapterPruneResult } from '../shared/GenomeAdapterPruneTypes';

export class GenomeAdapterPruneBrowserCommand extends CommandBase<GenomeAdapterPruneParams, GenomeAdapterPruneResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-prune', context, subpath, commander);
  }

  async execute(params: GenomeAdapterPruneParams): Promise<GenomeAdapterPruneResult> {
    console.log('🌐 BROWSER: Delegating Genome Adapter Prune to server');
    return await this.remoteExecute(params);
  }
}
