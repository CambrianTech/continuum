/**
 * Query Close Command - Browser Implementation
 *
 * Delegates to server for closing query handle.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { QueryCloseCommand } from '../shared/QueryCloseCommand';
import type { DataQueryCloseParams, DataQueryCloseResult } from '../shared/QueryCloseTypes';

export class QueryCloseBrowserCommand extends QueryCloseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser implementation: delegates to server
   * Query handles managed server-side only
   */
  protected async executeDataCommand(params: DataQueryCloseParams): Promise<DataQueryCloseResult> {
    return await this.remoteExecute(params);
  }
}
