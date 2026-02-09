/**
 * Data Delete Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataDeleteParams, DataDeleteResult } from '../shared/DataDeleteTypes';
import { createDataDeleteResultFromParams } from '../shared/DataDeleteTypes';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import type { BaseEntity } from '@system/data/entities/BaseEntity';
import { isValidCollection, type CollectionName } from '../../../../shared/generated-collection-constants';

export class DataDeleteServerCommand extends CommandBase<DataDeleteParams, DataDeleteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-delete', context, subpath, commander);
  }

  async execute(params: DataDeleteParams): Promise<DataDeleteResult> {
    const collection = params.collection;
    console.debug(`🗑️ DATA DELETE: Deleting ${collection}/${params.id} entity`);

    // Validate collection name at runtime (user input comes as string)
    if (!isValidCollection(collection)) {
      return createDataDeleteResultFromParams(params, {
        error: `Invalid collection name: ${collection}`,
        found: false
      });
    }
    const validCollection = collection as CollectionName;

    // First read the entity before deletion for the event
    let entityBeforeDelete: BaseEntity | null;
    try {
      entityBeforeDelete = await ORM.read(validCollection, params.id);
    } catch (error: any) {
      return createDataDeleteResultFromParams(params, {
        error: `Record not found: ${collection}/${params.id}`,
        found: false
      });
    }

    // DataDaemon throws on failure, returns success result on success
    // Events are emitted by DataDaemon.remove() via universal Events system
    // Pass suppressEvents flag to prevent events during internal operations (e.g., archiving)
    const result = await ORM.remove(validCollection, params.id, params.suppressEvents);

    if (!result.success) {
      throw new Error(result.error ?? 'Unknown error during data delete');
    }

    console.debug(`✅ DATA DELETE: Deleted ${collection}/${params.id}`);

    // Event emission handled by DataDaemon layer (no duplicate emission)
    // const eventName = BaseEntity.getEventName(collection, 'deleted');
    // await Events.emit(eventName, entityBeforeDelete, this.context, this.commander);
    // console.log(`✅ DataDeleteServerCommand: Emitted ${eventName}`);

    return createDataDeleteResultFromParams(params, {
      found: true,
      deleted: true,
    });
  }
}