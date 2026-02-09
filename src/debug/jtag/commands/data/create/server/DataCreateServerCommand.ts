/**
 * Data Create Command - Server Implementation
 *
 * Uses ORM for unified Rust-backed storage operations.
 * Supports multi-database operations via optional dbHandle parameter.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { DataCreateCommand } from '../shared/DataCreateCommand';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { CollectionName } from '../../../../shared/generated-collection-constants';

export class DataCreateServerCommand extends DataCreateCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server implementation: uses ORM for all operations
   * Supports optional dbHandle for per-persona databases
   */
  protected async executeDataCommand(params: DataCreateParams): Promise<DataCreateResult> {
    const collection = params.collection;

    // Resolve dbHandle to dbPath for per-persona databases
    let dbPath: string | undefined;
    if (params.dbHandle) {
      const registry = DatabaseHandleRegistry.getInstance();
      dbPath = registry.getDbPath(params.dbHandle) ?? undefined;
    }

    // Use ORM for all operations (routes to Rust with correct dbPath)
    const entity = await ORM.store(
      collection as CollectionName,
      params.data as BaseEntity,
      params.suppressEvents ?? false,
      dbPath
    );

    return createDataCreateResultFromParams(params, {
      success: true,
      data: entity
    });
  }
}