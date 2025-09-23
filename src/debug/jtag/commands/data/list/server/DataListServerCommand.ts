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
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../../system/data/entities/RoomEntity';

import type { StorageQuery, RecordData } from '../../../../daemons/data-daemon/shared/DataStorageAdapter';

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
    console.debug(`🗄️ DATA SERVER: Listing ${collection} entities via elegant type extraction`);

    try {
      const limit = Math.min(params.limit ?? DEFAULT_CONFIG.database.queryLimit, DEFAULT_CONFIG.database.maxBatchSize);

      // Use enhanced DataDaemon with field extraction - same as DataCreateCommand
      const storageQuery = {
        collection,
        filters: params.filter,
        sort: params.orderBy?.map(order => ({
          field: order.field,
          direction: order.direction
        })),
        limit
      };

      const result = await DataDaemon.query<BaseEntity>(storageQuery);

      if (!result.success) {
        console.error(`❌ DATA SERVER: DataDaemon query failed:`, result.error);
        return createDataListResultFromParams(params, {
          success: false,
          items: [],
          count: 0,
          error: result.error || 'Unknown DataDaemon error'
        });
      }

      console.debug(`✅ DATA SERVER: Listed ${result.data?.length || 0} items from ${collection} via DataDaemon`);
      console.debug(`🔧 CLAUDE-FIX-${Date.now()}: DataListServerCommand now uses entity-type-driven collection mapping`);

      // Extract data from DataRecord array and merge database ID into entity data
      const items: T[] = result.data ? result.data.map(record => ({
        ...record.data as T,
        id: record.id  // Merge database-generated UUID into entity
      })) : [];

      return createDataListResultFromParams(params, {
        success: true,
        items,
        count: items.length
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: DataDaemon execution failed:`, errorMessage);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: errorMessage
      });
    }
  }
}