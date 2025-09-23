/**
 * Data Update Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { Events } from '../../../../system/core/server/shared/Events';

export class DataUpdateServerCommand extends CommandBase<DataUpdateParams, DataUpdateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  async execute(params: DataUpdateParams): Promise<DataUpdateResult> {
    const collection = params.collection;
    console.debug(`🔄 DATA UPDATE: Updating ${collection}/${params.id} entity`);

    // DataDaemon returns updated entity directly or throws
    const entity = await DataDaemon.update(collection, params.id, params.data);

    console.debug(`✅ DATA UPDATE: Updated ${collection}/${entity.id}`);

    // Emit event using BaseEntity helper
    const eventName = BaseEntity.getEventName(collection, 'updated');
    await Events.emit(eventName, entity, this.context, this.commander);
    console.log(`✅ DataUpdateServerCommand: Emitted ${eventName}`);

    return createDataUpdateResultFromParams(params, {
      found: true
    });
  }

}