/**
 * Genome Layers Command - Browser Implementation
 *
 * Delegates to server where AdapterStore can scan the filesystem.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeLayersParams, GenomeLayersResult } from '../shared/GenomeLayersTypes';

export class GenomeLayersBrowserCommand extends CommandBase<GenomeLayersParams, GenomeLayersResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-layers', context, subpath, commander);
  }

  async execute(params: GenomeLayersParams): Promise<GenomeLayersResult> {
    return await this.remoteExecute(params);
  }
}
