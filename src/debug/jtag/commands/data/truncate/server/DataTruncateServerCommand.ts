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
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import { Events } from '../../../../system/core/shared/Events';
import { getDataEventName } from '../../shared/DataEventConstants';

export class DataTruncateServerCommand extends CommandBase<DataTruncateParams, DataTruncateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-truncate', context, subpath, commander);
  }

  async execute(params: DataTruncateParams): Promise<DataTruncateResult> {
    const { collection } = params;

    try {
      // Get record count before truncating for reporting
      const statsResult = await ORM.listCollections();
      let recordCount = 0;

      if (statsResult.success && statsResult.data?.includes(collection)) {
        // Collection exists, we can get stats
        // Note: We can't easily get record count without implementing a count method
        // For now, we'll just indicate that truncation was attempted
      }

      // Use adapter truncate() method - proper abstraction layer
      const result = await ORM.truncate(collection);

      if (result.success) {

        // Emit truncated event for widgets to clear their state
        // For truncate, we don't have entity data (clearing all), so use empty object with proper typing
        interface TruncateEventData {
          success: boolean;
          data: {
            collection: string;
            id: string;
            data: Record<string, never>; // Empty object type
            metadata?: {
              createdAt: string;
              updatedAt: string;
              version: number;
            };
          };
          timestamp?: string;
        }

        const eventName = getDataEventName(collection, 'truncated');
        const eventData: TruncateEventData = {
          success: true,
          data: {
            collection,
            id: '', // No specific ID for truncate operation
            data: {}, // No entity data for truncate
            metadata: {
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              version: 0
            }
          },
          timestamp: new Date().toISOString()
        };

        // Emit event so browser widgets can clear their caches
        await Events.emit<TruncateEventData>(this.context, eventName, eventData);

        return createDataTruncateResultFromParams(params, {
          success: true,
          recordsDeleted: recordCount > 0 ? recordCount : undefined
        });
      } else {
        console.error(`❌ DATA SERVER: Truncate operation failed for '${collection}':`, result.error);
        return createDataTruncateResultFromParams(params, {
          success: false,
          error: result.error ?? 'Truncate operation failed'
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