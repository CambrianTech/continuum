/**
 * Query Next Command - Browser Implementation
 *
 * Delegates to server for fetching next page.
 * All pagination state managed server-side.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { QueryNextCommand } from '../shared/QueryNextCommand';
import type { DataQueryNextParams, DataQueryNextResult } from '../shared/QueryNextTypes';

export class QueryNextBrowserCommand extends QueryNextCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser implementation: delegates to server
   * Query state maintained server-side only
   */
  protected async executeDataCommand(params: DataQueryNextParams): Promise<DataQueryNextResult> {
    return await this.remoteExecute(params);
  }
}
