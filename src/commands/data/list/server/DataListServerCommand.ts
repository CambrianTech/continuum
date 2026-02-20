/**
 * Data List Command - Server Implementation
 *
 * Uses ORM for unified Rust-backed storage operations.
 * Supports any storage backend via configurable adapters.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';
import { createDataListResultFromParams } from '../shared/DataListTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';  // Only for getDescriptionFieldForCollection
import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';  // Only for getDbPath
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';

// Rust-style config defaults for generic data access
const DEFAULT_CONFIG = {
  database: {
    queryLimit: 100,      // High default for substantial context
    maxBatchSize: 500,    // Safety ceiling
    minLimit: 1,          // Never allow 0 or negative
  }
} as const;



export class DataListServerCommand<T extends BaseEntity> extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }


  async execute(params: DataListParams): Promise<DataListResult<T>> {
    const collection = params.collection;

    // Validate collection is provided
    if (!collection) {
      // Get all registered collections from COLLECTIONS constant
      const allCollections = Object.values(COLLECTIONS);
      const commonCollections = ['users', 'rooms', 'chat_messages', 'memories', 'tasks', 'skills', 'wall_documents'];

      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: `Missing required parameter: collection. ` +
          `Common: ${commonCollections.join(', ')}. ` +
          `All ${allCollections.length} collections: ${allCollections.slice(0, 15).join(', ')}... ` +
          `Example: data/list --collection="users" --limit=10`
      });
    }

    try {
      const limit = Math.min(params.limit ?? DEFAULT_CONFIG.database.queryLimit, DEFAULT_CONFIG.database.maxBatchSize);

      // CRITICAL FIX: Use dbHandle when provided!
      // Previously, dbHandle was IGNORED and all queries went to the main database,
      // causing massive lock contention when multiple personas responded concurrently.
      // PersonaTimeline and other per-persona data structures pass their own dbHandle.

      let countResult: { success: boolean; data?: number } | undefined;
      let result;

      // Build queries
      const countQuery = {
        collection,
        filter: params.filter  // Use 'filter' (new) not 'filters' (legacy) for operator support
      };

      const storageQuery = {
        collection,
        filter: params.filter,  // Use 'filter' (new) not 'filters' (legacy) for operator support
        sort: params.orderBy?.map(order => ({
          field: order.field,
          direction: order.direction
        })),
        cursor: params.cursor,
        limit
      };

      // Resolve dbHandle to dbPath for per-persona databases
      // All operations now route through ORM → Rust with the correct dbPath
      let dbPath: string | undefined;
      if (params.dbHandle) {
        const registry = DatabaseHandleRegistry.getInstance();
        dbPath = registry.getDbPath(params.dbHandle) ?? undefined;
      }

      // Use ORM for all operations (routes to Rust with correct dbPath)
      // skipCount avoids a separate COUNT(*) round-trip when the caller only needs items
      if (!params.skipCount) {
        countResult = await ORM.count(countQuery, dbPath);
      }
      result = await ORM.query<BaseEntity>(storageQuery, dbPath);

      const totalCount = countResult?.success ? (countResult.data ?? 0) : 0;

      if (!result.success) {
        const errorMsg = result.error || 'Unknown DataDaemon error';
        console.error(`❌ DATA SERVER: DataDaemon query failed for '${collection}':`, errorMsg);
        return createDataListResultFromParams(params, {
          success: false,
          items: [],
          count: 0,
          error: errorMsg
        });
      }

      // Extract data from DataRecord array and merge database ID into entity data
      const items: T[] = result.data ? result.data.map(record => {
        const fullEntity = {
          ...record.data as T,
          id: record.id  // Merge database-generated UUID into entity
        };

        // Determine which fields to return based on params
        let fieldsToProject: string[] | undefined;

        if (params.fields && params.fields.length > 0) {
          // Explicit fields specified - use those
          fieldsToProject = [...params.fields];
        } else if (params.verbose === false) {
          // Lean mode - return only id + description field
          const descriptionField = DataDaemon.getDescriptionFieldForCollection(collection);
          if (descriptionField) {
            fieldsToProject = [descriptionField];
          }
          // If no description field, fall through to return all fields
        }

        // Apply field projection if we determined fields to project
        if (fieldsToProject && fieldsToProject.length > 0) {
          const projected: Partial<T> = { id: record.id } as Partial<T>;
          for (const field of fieldsToProject) {
            if (field in fullEntity) {
              (projected as any)[field] = (fullEntity as any)[field];
            }
          }
          return projected as T;
        }

        return fullEntity;
      }) : [];

      return createDataListResultFromParams(params, {
        success: true,
        items,
        count: totalCount // Use separate count query result
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: DataDaemon execution failed for '${collection}':`, errorMessage);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: errorMessage
      });
    }
  }
}