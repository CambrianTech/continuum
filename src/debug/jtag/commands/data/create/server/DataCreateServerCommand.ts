/**
 * Data Create Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 * Supports multi-database operations via optional dbHandle parameter
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { DataCreateCommand } from '../shared/DataCreateCommand';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { Events } from '../../../../system/core/shared/Events';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class DataCreateServerCommand extends DataCreateCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server implementation: handles both server (SQLite) and localStorage (delegate) backends
   * Supports optional dbHandle for multi-database operations
   */
  protected async executeDataCommand(params: DataCreateParams): Promise<DataCreateResult> {
    // Server only handles server environment
    // Browser environment requests are delegated by base class
    const collection = params.collection;
    const dbHandle = params.dbHandle;

    console.debug(`🗄️ DATA SERVER: Creating ${collection} entity${dbHandle ? ` (handle: ${dbHandle})` : ''}`);

    let entity: BaseEntity;

    if (dbHandle) {
      // Multi-database operation: use DatabaseHandleRegistry to get adapter
      // Then create a temporary DataDaemon instance with that adapter
      const registry = DatabaseHandleRegistry.getInstance();
      const adapter = registry.getAdapter(dbHandle);

      // Create temporary DataDaemon instance with the specific adapter
      const tempDaemon = new DataDaemon({
        strategy: 'sql',
        backend: 'sqlite',
        namespace: dbHandle,
        options: {}
      }, adapter);

      // Use DataDaemon's create() method which handles DataRecord construction
      const operationContext = {
        sessionId: params.sessionId,
        timestamp: new Date().toISOString(),
        source: 'data-create-command'
      };

      entity = await tempDaemon.create(collection, params.data, operationContext);

      // Emit event manually since we're not using the global DataDaemon
      const eventName = BaseEntity.getEventName(collection, 'created');
      await Events.emit(this.context, eventName, entity);
    } else {
      // Default operation: use DataDaemon (backward compatible)
      // Events are emitted by DataDaemon.store() via universal Events system
      entity = await DataDaemon.store(collection, params.data);
    }

    console.debug(`✅ DATA SERVER: Created ${collection}/${entity.id}`);

    return createDataCreateResultFromParams(params, {
      success: true,
      data: entity
    });
  }
}