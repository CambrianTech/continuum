/**
 * Data List Command - Server Implementation
 *
 * Storage-agnostic data listing using proper DataService abstraction
 * Supports any storage backend via configurable adapters
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';
import { createDataListResultFromParams } from '../shared/DataListTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';

// Rust-style config defaults for generic data access
const DEFAULT_CONFIG = {
  database: {
    queryLimit: 100,      // High default for substantial context
    maxBatchSize: 500,    // Safety ceiling
    minLimit: 1,          // Never allow 0 or negative
  }
} as const;



export class DataListServerCommand<T extends BaseEntity> extends CommandBase<DataListParams<T>, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }


  async execute(params: DataListParams<T>): Promise<DataListResult<T>> {
    const collection = params.collection;

    try {
      const limit = Math.min(params.limit ?? DEFAULT_CONFIG.database.queryLimit, DEFAULT_CONFIG.database.maxBatchSize);

      // FIRST: Get total count with same filters (no limit, no cursor)
      const countQuery = {
        collection,
        filter: params.filter  // Use 'filter' (new) not 'filters' (legacy) for operator support
      };
      const countResult = await DataDaemon.query<BaseEntity>(countQuery);
      const totalCount = countResult.success ? (countResult.data?.length ?? 0) : 0;

      // SECOND: Get paginated data with sorting, cursor, and limit
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
      //console.debug(`🔧 CURSOR-DEBUG: Received cursor from client: ${params.cursor ? JSON.stringify(params.cursor) : 'NONE'}`);

      const result = await DataDaemon.query<BaseEntity>(storageQuery);

      if (!result.success) {
        const availableCollections = Object.values(COLLECTIONS).join(', ');
        const errorWithHint = `${result.error || 'Unknown DataDaemon error'}. Valid collections are: ${availableCollections}`;
        console.error(`❌ DATA SERVER: DataDaemon query failed:`, errorWithHint);
        return createDataListResultFromParams(params, {
          success: false,
          items: [],
          count: 0,
          error: errorWithHint
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
      const availableCollections = Object.values(COLLECTIONS).join(', ');
      const errorWithHint = `${errorMessage}. Valid collections are: ${availableCollections}`;
      console.error(`❌ DATA SERVER: DataDaemon execution failed:`, errorWithHint);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: errorWithHint
      });
    }
  }
}