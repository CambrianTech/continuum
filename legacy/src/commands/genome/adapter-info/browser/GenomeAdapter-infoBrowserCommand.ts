/**
 * Genome Adapter Info Command - Browser Implementation
 *
 * Get detailed information about a specific LoRA adapter including full manifest, training provenance, architecture details, and compatibility status.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterInfoParams, GenomeAdapterInfoResult } from '../shared/GenomeAdapterInfoTypes';

export class GenomeAdapterInfoBrowserCommand extends CommandBase<GenomeAdapterInfoParams, GenomeAdapterInfoResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-info', context, subpath, commander);
  }

  async execute(params: GenomeAdapterInfoParams): Promise<GenomeAdapterInfoResult> {
    console.log('🌐 BROWSER: Delegating Genome Adapter Info to server');
    return await this.remoteExecute(params);
  }
}
