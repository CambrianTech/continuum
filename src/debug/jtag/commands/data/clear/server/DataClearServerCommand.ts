/**
 * Data Clear Command - Server Implementation
 *
 * Uses DataDaemon adapter clear() method for proper storage abstraction
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataClearParams, DataClearResult } from '../shared/DataClearTypes';
import { createDataClearResultFromParams } from '../shared/DataClearTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';

export class DataClearServerCommand extends CommandBase<DataClearParams, DataClearResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-clear', context, subpath, commander);
  }

  async execute(params: DataClearParams): Promise<DataClearResult> {
    console.debug('🧹 DATA SERVER: Clearing all collections via adapter clear() method');

    try {
      // Get collections before clearing for reporting
      const collectionsResult = await DataDaemon.listCollections();
      const collectionCount = collectionsResult.success && collectionsResult.data ? collectionsResult.data.length : 0;

      // Use adapter clear() method - proper abstraction layer
      const result = await DataDaemon.clear();

      if (result.success) {
        console.log(`✅ DATA SERVER: Cleared all collections via adapter (${collectionCount} collections)`);

        return createDataClearResultFromParams(params, {
          success: true,
          collectionsCleared: collectionCount
        });
      } else {
        console.error('❌ DATA SERVER: Clear operation failed:', result.error);
        return createDataClearResultFromParams(params, {
          success: false,
          error: result.error || 'Clear operation failed'
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('❌ DATA SERVER: Clear operation error:', errorMessage);
      return createDataClearResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }
}