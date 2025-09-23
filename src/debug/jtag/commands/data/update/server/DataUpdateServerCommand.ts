/**
 * Data Update Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 * Mirrors DataCreateServerCommand pattern for consistency
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { getDataEventName } from '../../shared/DataEventConstants';
import { Events } from '../../../../system/core/server/shared/Events';

export class DataUpdateServerCommand extends CommandBase<DataUpdateParams, DataUpdateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  async execute(params: DataUpdateParams): Promise<DataUpdateResult> {
    const collection = params.collection;
    console.debug(`🔄 DATA UPDATE: Updating ${collection}/${params.id} via DataDaemon`);

    try {
      // Parse update data if it's a string (from CLI)
      let updateData = params.data;
      if (typeof params.data === 'string') {
        try {
          updateData = JSON.parse(params.data);
        } catch (e) {
          return createDataUpdateResultFromParams(params, {
            error: `Invalid JSON in data parameter: ${e instanceof Error ? e.message : 'Unknown error'}`,
            found: false
          });
        }
      }

      // Use DataDaemon.update (mirroring DataDaemon.store pattern from create command)
      const result = await DataDaemon.update(collection, params.id, updateData as Partial<BaseEntity>);

      if (result.success && result.data) {
        console.log(`✅ DATA UPDATE: Updated ${collection}/${params.id} via DataDaemon`);

        // Emit data:collection:updated event (mirror the create pattern)
        try {
          console.log(`📡 DataUpdateServerCommand: Emitting data:${collection}:updated event via Events.emit<T>()`);
          // Parse JSON string to object before spreading (same as create command)
          const parsedData = typeof result.data.data === 'string' ? JSON.parse(result.data.data) : result.data.data;
          const entityData = {
            ...parsedData,
            id: result.data.id
          };
          const eventName = getDataEventName(collection, 'updated');
          await Events.emit(eventName, entityData, this.context, this.commander);
          console.log(`✅ DataUpdateServerCommand: Successfully emitted ${eventName} via Events.emit<T>()`);
        } catch (eventError) {
          console.error(`❌ DataUpdateServerCommand: Failed to emit event:`, eventError);
          // Don't fail the command if event emission fails
        }

        return createDataUpdateResultFromParams(params, {
          found: true,
          data: result.data,
          previousVersion: result.data.metadata.version - 1, // Assuming version was incremented
          newVersion: result.data.metadata.version
        });
      } else {
        console.error(`❌ DATA UPDATE: DataDaemon.update failed:`, result.error);
        return createDataUpdateResultFromParams(params, {
          error: result.error || 'Record not found',
          found: false
        });
      }

    } catch (error: any) {
      console.error(`❌ DATA UPDATE: Update failed:`, error.message);
      return createDataUpdateResultFromParams(params, {
        error: error.message,
        found: false
      });
    }
  }
}