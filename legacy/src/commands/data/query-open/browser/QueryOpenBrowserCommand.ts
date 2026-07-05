/**
 * Query Open Command - Browser Implementation
 *
 * Delegates to server for query handle management.
 * Browser doesn't maintain pagination state locally.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { QueryOpenCommand } from '../shared/QueryOpenCommand';
import type { DataQueryOpenParams, DataQueryOpenResult } from '../shared/QueryOpenTypes';

export class QueryOpenBrowserCommand extends QueryOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser implementation: delegates to server
   * All pagination state managed server-side
   */
  protected async executeDataCommand(params: DataQueryOpenParams): Promise<DataQueryOpenResult> {
    return await this.remoteExecute(params);
  }
}
