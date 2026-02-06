/**
 * Query Next Command - Server Implementation
 *
 * Fetches next page from DataDaemon using query handle.
 * DataDaemon maintains cursor position internally.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { QueryNextCommand } from '../shared/QueryNextCommand';
import type { DataQueryNextParams, DataQueryNextResult } from '../shared/QueryNextTypes';
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';

export class QueryNextServerCommand extends QueryNextCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server implementation: fetches next page from DataDaemon
   */
  protected async executeDataCommand(params: DataQueryNextParams): Promise<DataQueryNextResult> {
    const queryHandle = params.queryHandle;
    console.debug(`📄 QUERY-NEXT SERVER: Fetching next page for query ${queryHandle}`);

    try {
      // Get next page using static DataDaemon interface
      const page = await ORM.getNextPage(queryHandle);

      console.debug(`✅ QUERY-NEXT SERVER: Fetched page ${page.pageNumber} (${page.items.length} items, hasMore=${page.hasMore})`);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        items: page.items,
        pageNumber: page.pageNumber,
        hasMore: page.hasMore,
        totalCount: page.totalCount,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      console.error(`❌ QUERY-NEXT SERVER: Failed to fetch page:`, error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        items: [],
        pageNumber: 0,
        hasMore: false,
        totalCount: 0,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }
}
