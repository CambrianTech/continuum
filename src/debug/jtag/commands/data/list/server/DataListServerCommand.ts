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
import { DataServiceFactory } from '../../../../system/data/services/DataServiceFactory';
import type { QueryOptions, BaseEntity } from '../../../../system/data/domains/CoreTypes';
import { convertToDomainObjects, hasDomainSupport, getDomainFactory } from '../../../../system/data/core/DomainRegistry';
import { DATABASE_PATHS } from '../../../../system/data/config/DatabaseConfig';

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
    console.debug(`🗄️ DATA SERVER: Listing ${params.collection} via DataService`);

    try {
      const limit = Math.min(params.limit ?? DEFAULT_CONFIG.database.queryLimit, DEFAULT_CONFIG.database.maxBatchSize);

      // Create DataService using existing abstraction with centralized config
      const dataService = await DataServiceFactory.createSQLiteOnly(
        DATABASE_PATHS.SQLITE
      );

      // Convert DataListParams to QueryOptions format
      const queryOptions: QueryOptions<T> = {
        limit,
        filters: params.filter as Record<keyof T, unknown>,
        orderBy: params.orderBy?.map(order => ({
          field: order.field as keyof T,
          direction: order.direction.toUpperCase() as 'ASC' | 'DESC'
        })),
        cursor: params.cursor ? {
          field: params.cursor.field as keyof T,
          value: params.cursor.value,
          direction: params.cursor.direction
        } : undefined
      };

      // Use DataService list method - storage agnostic
      const result = await dataService.list<T>(params.collection, queryOptions);

      // Close DataService connection
      await dataService.close();

      if (!result.success) {
        console.error(`❌ DATA SERVER: DataService query failed:`, result.error?.message);
        return createDataListResultFromParams(params, {
          success: false,
          items: [],
          count: 0,
          error: result.error?.message || 'Unknown DataService error'
        });
      }

      console.debug(`✅ DATA SERVER: Listed ${result.data.length} items from ${params.collection}`);

      // Elegantly convert to domain objects if requested
      let items: T[] = result.data;
      if (params.convertToDomain && hasDomainSupport(params.collection)) {
        try {
          const domainFactory = getDomainFactory<T>(params.collection);
          if (domainFactory) {
            items = result.data.map(data => domainFactory.fromData(data)) as T[];
            console.debug(`✨ DATA SERVER: Converted ${items.length} items to domain objects for ${params.collection}`);
          }
        } catch (error) {
          console.error(`❌ DATA SERVER: Domain conversion failed for ${params.collection}:`, error);
          throw error; // Fail fast, don't return bad data
        }
      }

      return createDataListResultFromParams(params, {
        success: true,
        items,
        count: items.length
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: DataService execution failed:`, errorMessage);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: errorMessage
      });
    }
  }
}