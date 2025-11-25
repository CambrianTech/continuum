/**
 * Vector Search Command - Browser Implementation
 *
 * Delegates to server-side execution via remoteExecute.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { VectorSearchParams, VectorSearchResult_CLI } from '../shared/VectorSearchCommandTypes';

export class VectorSearchBrowserCommand extends CommandBase<VectorSearchParams, VectorSearchResult_CLI> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-vector-search', context, subpath, commander);
  }

  async execute(params: VectorSearchParams): Promise<VectorSearchResult_CLI> {
    return await this.remoteExecute(params);
  }
}
