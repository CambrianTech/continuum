/**
 * Data Create Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { getDataEventName } from '../../shared/DataEventConstants';
import { Events } from '../../../../system/core/server/shared/Events';


export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }


  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    const collection = params.collection;
    console.debug(`🗄️ DATA SERVER: Creating ${collection} record via DataDaemon`);

    try {
      // Parse data if it's a string (from CLI)
      let entityData = params.data;
      if (typeof params.data === 'string') {
        try {
          entityData = JSON.parse(params.data);
        } catch (e) {
          return createDataCreateResultFromParams(params, {
            success: false,
            error: `Invalid JSON in data parameter: ${e instanceof Error ? e.message : 'Unknown error'}`
          });
        }
      }

      // Use enhanced DataDaemon with field extraction
      const result = await DataDaemon.store(collection, entityData as BaseEntity);

      if (result.success && result.data) {
        console.debug(`✅ DATA SERVER: Created ${collection}/${result.data.id} with field extraction`);

        // Emit generic data creation event for real-time UI updates
        try {
          console.log(`📡 DataCreateServerCommand: Emitting data:${collection}:created event via Events.emit<T>()`);
          // Match DataListServerCommand format: merge record.data + record.id
          const eventData = {
            ...result.data.data,
            id: result.data.id  // Same merge as DataListServerCommand lines 73-76
          };
          const eventName = getDataEventName(collection, 'created');
          await Events.emit(eventName, entityData, this.context, this.commander);
          console.log(`✅ DataCreateServerCommand: Successfully emitted ${eventName} via Events.emit<T>()`);
        } catch (eventError) {
          console.error(`❌ DataCreateServerCommand: Failed to emit event:`, eventError);
          // Don't fail the command if event emission fails
        }

        return createDataCreateResultFromParams(params, {
          success: true,
          id: result.data.id
        });
      } else {
        console.error(`❌ DATA SERVER: DataDaemon.store failed:`, result.error);
        return createDataCreateResultFromParams(params, {
          success: false,
          error: result.error || 'Unknown error'
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: DataDaemon create failed:`, errorMessage);
      return createDataCreateResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }

}