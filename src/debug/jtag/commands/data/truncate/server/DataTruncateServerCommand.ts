/**
 * Data Truncate Command - Server Implementation
 *
 * Uses DataDaemon adapter truncate() method for proper storage abstraction
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataTruncateParams, DataTruncateResult } from '../shared/DataTruncateTypes';
import { createDataTruncateResultFromParams } from '../shared/DataTruncateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';

export class DataTruncateServerCommand extends CommandBase<DataTruncateParams, DataTruncateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-truncate', context, subpath, commander);
  }

  async execute(params: DataTruncateParams): Promise<DataTruncateResult> {
    const { collection } = params;
    console.debug(`🗑️ DATA SERVER: Truncating collection '${collection}' via adapter truncate() method`);

    try {
      // Get record count before truncating for reporting
      const statsResult = await DataDaemon.listCollections();
      let recordCount = 0;

      if (statsResult.success && statsResult.data?.includes(collection)) {
        // Collection exists, we can get stats
        // Note: We can't easily get record count without implementing a count method
        // For now, we'll just indicate that truncation was attempted
      }

      // Use adapter truncate() method - proper abstraction layer
      const result = await DataDaemon.truncate(collection);

      if (result.success) {
        console.log(`✅ DATA SERVER: Truncated collection '${collection}' via adapter`);

        return createDataTruncateResultFromParams(params, {
          success: true,
          recordsDeleted: recordCount > 0 ? recordCount : undefined
        });
      } else {
        console.error(`❌ DATA SERVER: Truncate operation failed for '${collection}':`, result.error);
        return createDataTruncateResultFromParams(params, {
          success: false,
          error: result.error || 'Truncate operation failed'
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: Truncate operation error for '${collection}':`, errorMessage);
      return createDataTruncateResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }
}