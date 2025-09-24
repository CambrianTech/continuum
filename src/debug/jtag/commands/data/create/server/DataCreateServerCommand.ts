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
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Events } from '../../../../system/core/server/shared/Events';


export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }


  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    const collection = params.collection;
    console.debug(`🗄️ DATA SERVER: Creating ${collection} entity`);

    // DataDaemon creates entity directly or throws
    const entity = await DataDaemon.store(collection, params.data);

    console.debug(`✅ DATA SERVER: Created ${collection}/${entity.id}`);

    // Emit event using BaseEntity helper
    const eventName = BaseEntity.getEventName(collection, 'created');
    await Events.emit(eventName, entity, this.context, this.commander);
    console.log(`✅ DataCreateServerCommand: Emitted ${eventName}`);

    return createDataCreateResultFromParams(params, {
      success: true,
      data: entity,
    });
  }

}