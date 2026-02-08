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
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { CollectionName } from '../../../../shared/generated-collection-constants';

export class DataCreateServerCommand extends DataCreateCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server implementation: handles both server (SQLite) and localStorage (delegate) backends
   * Supports optional dbHandle for multi-database operations
   */
  protected async executeDataCommand(params: DataCreateParams): Promise<DataCreateResult> {
    const collection = params.collection;
    const dbHandle = params.dbHandle;

    let entity: BaseEntity;

    if (dbHandle) {
      // Multi-database operation: use DatabaseHandleRegistry to get adapter
      // Then create a temporary DataDaemon instance with that adapter
      const registry = DatabaseHandleRegistry.getInstance();
      const adapter = registry.getAdapter(dbHandle);

      // Get emitEvents preference from handle metadata
      const metadata = registry.getMetadata(dbHandle);
      const shouldEmitEvents = metadata?.emitEvents ?? true;

      // Create temporary DataDaemon instance with the specific adapter
      const tempDaemon = new DataDaemon({
        strategy: 'sql',
        backend: 'sqlite',
        namespace: dbHandle,
        options: {}
      }, adapter, true);

      const operationContext = {
        sessionId: params.sessionId,
        timestamp: new Date().toISOString(),
        source: 'data-create-command'
      };

      const suppressEvents = params.suppressEvents ?? !shouldEmitEvents;
      entity = await tempDaemon.create(collection, params.data as BaseEntity, operationContext, suppressEvents);
    } else {
      // Default operation: use ORM (Rust-backed unified path)
      entity = await ORM.store(collection as CollectionName, params.data as BaseEntity, params.suppressEvents ?? false);
    }

    return createDataCreateResultFromParams(params, {
      success: true,
      data: entity
    });
  }
}