/**
 * Data Update Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
// import { Events } from '../../../../system/core/server/shared/Events';
import { DataUpdateCommand } from '../shared/DataUpdateCommand';

export class DataUpdateServerCommand extends DataUpdateCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataUpdateParams): Promise<DataUpdateResult<BaseEntity>> {
    const collection = params.collection;

    // DataDaemon returns updated entity directly or throws
    // Events are emitted by DataDaemon.update() via universal Events system
    const entity = await DataDaemon.update(collection, params.id, params.data);

    // Event emission handled by DataDaemon layer (no duplicate emission)
    // const eventName = BaseEntity.getEventName(collection, 'updated');
    // await Events.emit(eventName, entity, this.context, this.commander);
    // console.log(`✅ DataUpdateServerCommand: Emitted ${eventName}`);

    return createDataUpdateResultFromParams(params, {
      success: Boolean(entity),
      found: Boolean(entity),
      data: entity,
    });
  }

}