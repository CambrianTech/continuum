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
    console.log(`📄 QueryNextBrowser: Delegating to server for query: ${params.queryHandle}`);

    // Browser always delegates to server
    // Query handles and pagination state are server-side only
    throw new Error('Browser must delegate query-next to server');
  }
}
