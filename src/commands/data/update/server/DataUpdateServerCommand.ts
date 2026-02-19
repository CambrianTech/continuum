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
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
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

    // Resolve dbHandle to dbPath for per-persona databases
    let dbPath: string | undefined;
    if (params.dbHandle) {
      const registry = DatabaseHandleRegistry.getInstance();
      dbPath = registry.getDbPath(params.dbHandle) ?? undefined;
    }

    // Use ORM for all operations (routes to Rust with correct dbPath)
    const entity = await ORM.update(
      collection as CollectionName,
      params.id,
      params.data as Partial<BaseEntity>,
      params.incrementVersion ?? true,
      dbPath,
      params.suppressEvents ?? false
    );

    return createDataUpdateResultFromParams(params, {
      success: Boolean(entity),
      found: Boolean(entity),
      data: entity || undefined,
    });
  }

}