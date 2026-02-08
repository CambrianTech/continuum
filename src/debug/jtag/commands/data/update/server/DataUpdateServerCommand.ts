/**
 * Data Update Command - Server Implementation
 *
 * Uses ORM for unified Rust-backed storage operations.
 * Supports multi-database operations via optional dbHandle parameter.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { DataUpdateCommand } from '../shared/DataUpdateCommand';
import type { CollectionName } from '../../../../shared/generated-collection-constants';

export class DataUpdateServerCommand extends DataUpdateCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataUpdateParams): Promise<DataUpdateResult<BaseEntity>> {
    const collection = params.collection;

    let entity: BaseEntity | null;

    if (params.dbHandle) {
      // Per-persona database: get adapter from registry
      const registry = DatabaseHandleRegistry.getInstance();
      const adapter = registry.getAdapter(params.dbHandle);

      // Ensure schema is cached on the per-persona adapter before updating
      await DataDaemon.ensureAdapterSchema(adapter, collection);

      // Use adapter's update method directly (per-persona databases don't emit global events)
      const result = await adapter.update<BaseEntity>(collection, params.id, params.data as Partial<BaseEntity>, true);
      entity = result.success && result.data ? result.data.data : null;
    } else {
      // Default operation: use ORM (Rust-backed unified path)
      entity = await ORM.update(collection as CollectionName, params.id, params.data as Partial<BaseEntity>, true);
    }

    return createDataUpdateResultFromParams(params, {
      success: Boolean(entity),
      found: Boolean(entity),
      data: entity || undefined,
    });
  }

}