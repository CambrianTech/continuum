/**
 * Data Create Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/domains/CoreTypes';
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../../system/data/entities/RoomEntity';


export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }


  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    const collection = params.collection;
    console.debug(`🗄️ DATA SERVER: Creating ${collection} record via DataDaemon`);

    try {
      const id = params.id ?? generateUUID();

      // Use enhanced DataDaemon with field extraction
      const result = await DataDaemon.store(collection, params.data, id);

      if (result.success && result.data) {
        console.debug(`✅ DATA SERVER: Created ${collection}/${result.data.id} with field extraction`);
        console.debug(`🔧 CLAUDE-FIX-${Date.now()}: DataCreateServerCommand now uses DataDaemon.store() for field extraction`);

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