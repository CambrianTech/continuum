/**
 * Data List Command - Browser Implementation (LOCAL-FIRST)
 *
 * ALL collections use offline-first storage:
 * 1. Check localStorage first (instant)
 * 2. If found, return immediately
 * 3. If not found, fetch from server and cache
 *
 * No hardcoded collection lists - everything is cached.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';
import { createDataListResultFromParams } from '../shared/DataListTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { LocalStorageDataBackend } from '../../../../daemons/data-daemon/browser/LocalStorageDataBackend';

export class DataListBrowserCommand<T extends BaseEntity> extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult<T>> {
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
    const startTime = performance.now();

    // 1. Try localStorage first (instant)
    const localResult = await this.executeLocal(params);

    if (localResult.success && localResult.items.length > 0) {
      verbose && console.log(`⚡ data/list: LOCAL hit ${params.collection} (${localResult.items.length} items, ${(performance.now() - startTime).toFixed(1)}ms)`);
      return localResult;
    }

    // 2. Fetch from server
    verbose && console.log(`🔄 data/list: LOCAL miss ${params.collection}, fetching from server...`);
    const serverResult = await this.remoteExecute<DataListParams, DataListResult<T>>(params);
    verbose && console.log(`📡 data/list: SERVER ${params.collection} (${serverResult.items?.length ?? 0} items, ${(performance.now() - startTime).toFixed(1)}ms)`);

    // 3. Cache results for future reads
    if (serverResult.success && serverResult.items && serverResult.items.length > 0) {
      this.cacheResults(params.collection, serverResult.items);
    }

    return serverResult;
  }

  /**
   * Cache results in localStorage (fire-and-forget)
   */
  private cacheResults(collection: string, items: readonly T[]): void {
    for (const item of items) {
      if (item && 'id' in item) {
        LocalStorageDataBackend.create(collection, item).catch(() => {
          // Ignore cache errors - localStorage might be full
        });
      }
    }
  }

  /**
   * Execute query against localStorage
   */
  private async executeLocal(params: DataListParams): Promise<DataListResult<T>> {
    try {
      const result = await LocalStorageDataBackend.list<T>(params.collection, params.filter);

      if (result.success && result.entities && result.entities.length > 0) {
        let items = result.entities;

        // Apply sorting
        if (params.orderBy && params.orderBy.length > 0) {
          items = this.applySort(items, params.orderBy);
        }

        // Apply limit
        if (params.limit && items.length > params.limit) {
          items = items.slice(0, params.limit);
        }

        return createDataListResultFromParams<T>(params, {
          success: true,
          items: items as readonly T[],
          count: items.length,
        });
      }

      return createDataListResultFromParams<T>(params, {
        success: true,
        items: [],
        count: 0,
      });

    } catch {
      return createDataListResultFromParams<T>(params, {
        success: true,
        items: [],
        count: 0,
      });
    }
  }

  /**
   * Apply sort order to items
   */
  private applySort(items: T[], orderBy: { field: string; direction: 'asc' | 'desc' }[]): T[] {
    return [...items].sort((a, b) => {
      for (const order of orderBy) {
        const aVal = (a as any)[order.field];
        const bVal = (b as any)[order.field];
        if (aVal === bVal) continue;

        const direction = order.direction === 'asc' ? 1 : -1;
        if (aVal == null) return direction;
        if (bVal == null) return -direction;

        if (typeof aVal === 'string' && typeof bVal === 'string') {
          return aVal.localeCompare(bVal) * direction;
        }
        return (aVal < bVal ? -1 : 1) * direction;
      }
      return 0;
    });
  }
}
