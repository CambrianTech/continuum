/**
 * Data Update Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 * Supports multi-database operations via optional dbHandle parameter
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
// import { Events } from '../../../../system/core/server/shared/Events';
import { DataUpdateCommand } from '../shared/DataUpdateCommand';

export class DataUpdateServerCommand extends DataUpdateCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataUpdateParams): Promise<DataUpdateResult<BaseEntity>> {
    const collection = params.collection;

    let entity: BaseEntity | null;

    // CRITICAL FIX: Use dbHandle when provided!
    // Previously, dbHandle was IGNORED and all updates went to the main database.
    if (params.dbHandle) {
      // Per-persona database: get adapter from registry
      const registry = DatabaseHandleRegistry.getInstance();
      const adapter = registry.getAdapter(params.dbHandle);

      // Ensure schema is cached on the per-persona adapter before updating
      await DataDaemon.ensureAdapterSchema(adapter, collection);

      // Use adapter's update method directly
      // Note: Per-persona databases don't emit global events by design
      const result = await adapter.update<BaseEntity>(collection, params.id, params.data as Partial<BaseEntity>, true);
      entity = result.success && result.data ? result.data.data : null;
    } else {
      // Default operation: use DataDaemon (backward compatible)
      // Events are emitted by DataDaemon.update() via universal Events system
      entity = await DataDaemon.update(collection, params.id, params.data);
    }

    return createDataUpdateResultFromParams(params, {
      success: Boolean(entity),
      found: Boolean(entity),
      data: entity || undefined,
    });
  }

}