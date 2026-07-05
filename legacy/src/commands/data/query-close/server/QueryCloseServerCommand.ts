/**
 * Query Close Command - Server Implementation
 *
 * Closes query handle in DataDaemon and frees resources.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { QueryCloseCommand } from '../shared/QueryCloseCommand';
import type { DataQueryCloseParams, DataQueryCloseResult } from '../shared/QueryCloseTypes';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';

export class QueryCloseServerCommand extends QueryCloseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server implementation: closes query in DataDaemon
   */
  protected async executeDataCommand(params: DataQueryCloseParams): Promise<DataQueryCloseResult> {
    const queryHandle = params.queryHandle;
    console.debug(`🔒 QUERY-CLOSE SERVER: Closing query ${queryHandle}`);

    try {
      // Close query using static DataDaemon interface
      ORM.closePaginatedQuery(queryHandle);

      console.debug(`✅ QUERY-CLOSE SERVER: Closed query ${queryHandle}`);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      console.error(`❌ QUERY-CLOSE SERVER: Failed to close query:`, error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }
}
