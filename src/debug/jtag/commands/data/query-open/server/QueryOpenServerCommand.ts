/**
 * Query Open Command - Server Implementation
 *
 * Opens a paginated query in DataDaemon and returns handle.
 * Uses entity's pagination config for defaults.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { QueryOpenCommand } from '../shared/QueryOpenCommand';
import type { DataQueryOpenParams, DataQueryOpenResult } from '../shared/QueryOpenTypes';
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';

export class QueryOpenServerCommand extends QueryOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server implementation: opens paginated query in DataDaemon
   */
  protected async executeDataCommand(params: DataQueryOpenParams): Promise<DataQueryOpenResult> {
    const collection = params.collection;
    console.debug(`📖 QUERY-OPEN SERVER: Opening paginated query for ${collection}`);

    try {
      // Open paginated query using static DataDaemon interface
      const handle = await ORM.openPaginatedQuery({
        collection: params.collection,
        filter: params.filter,
        orderBy: params.orderBy,
        pageSize: params.pageSize
      });

      console.debug(`✅ QUERY-OPEN SERVER: Opened query ${handle.queryId} (${handle.totalCount} total records)`);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        queryHandle: handle.queryId,
        totalCount: handle.totalCount,
        pageSize: handle.pageSize,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      console.error(`❌ QUERY-OPEN SERVER: Failed to open query:`, error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        queryHandle: '' as any, // Required by interface
        totalCount: 0,
        pageSize: 0,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }
}
