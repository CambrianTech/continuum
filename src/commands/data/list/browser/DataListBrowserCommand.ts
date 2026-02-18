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
import { Events } from '../../../../system/core/shared/Events';

export class DataListBrowserCommand<T extends BaseEntity> extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult<T>> {
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
    const startTime = performance.now();
    const backend = params.backend ?? 'auto';

    // If backend is 'server', skip localStorage entirely
    if (backend === 'server') {
      verbose && console.log(`📡 data/list: SERVER-ONLY ${params.collection} (backend=server)`);
      const serverResult = await this.remoteExecute<DataListParams, DataListResult<T>>(params);
      verbose && console.log(`📡 data/list: SERVER ${params.collection} (${serverResult.items?.length ?? 0} items, ${(performance.now() - startTime).toFixed(1)}ms)`);
      return serverResult;
    }

    // Stale-while-revalidate: Return cached immediately, fetch fresh in background
    if (backend === 'stale-while-revalidate') {
      const localResult = await this.executeLocal(params);

      // Always trigger background revalidation
      this.revalidateInBackground(params, verbose);

      if (localResult.success && localResult.items.length > 0) {
        verbose && console.log(`⚡ data/list: STALE ${params.collection} (${localResult.items.length} cached, revalidating...)`);
        return localResult;
      }

      // No cache - must wait for server
      verbose && console.log(`🔄 data/list: STALE-MISS ${params.collection}, waiting for server...`);
      const serverResult = await this.remoteExecute<DataListParams, DataListResult<T>>(params);
      this.cacheResults(params.collection, serverResult.items ?? []);
      return serverResult;
    }

    // 1. Try localStorage first (instant) - for 'local' or 'auto' backend
    const localResult = await this.executeLocal(params);

    if (localResult.success && localResult.items.length > 0) {
      verbose && console.log(`⚡ data/list: LOCAL hit ${params.collection} (${localResult.items.length} items, ${(performance.now() - startTime).toFixed(1)}ms)`);
      return localResult;
    }

    // If backend is 'local', don't fall back to server
    if (backend === 'local') {
      verbose && console.log(`📦 data/list: LOCAL-ONLY ${params.collection} (no data, backend=local)`);
      return localResult;
    }

    // 2. Fetch from server (for 'auto' backend)
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
   * Revalidate cache in background - SMOOTH merge of new data
   *
   * KEY INSIGHT: Background revalidation should be INVISIBLE but still merge new data.
   * - Tab switching shows cached data instantly (reactive)
   * - Fresh server data updates cache in background
   * - NEW items are added via EntityCacheService (batched notifications = smooth)
   * - User sees new messages appear without jarring UI updates
   *
   * This follows the "stale-while-revalidate" pattern correctly:
   * Users see stale data immediately, new data merges smoothly.
   */
  private revalidateInBackground(params: DataListParams, verbose: boolean): void {
    // Use requestIdleCallback for truly non-blocking background work
    const doRevalidate = () => {
      // Get cached IDs first to detect truly NEW items
      this.executeLocal(params).then(cachedResult => {
        const cachedIds = new Set((cachedResult.items ?? []).map(item => item.id));

        return this.remoteExecute<DataListParams, DataListResult<T>>(params).then(serverResult => {
          if (serverResult.success && serverResult.items && serverResult.items.length > 0) {
            // Update localStorage cache
            this.cacheResults(params.collection, serverResult.items);

            // Merge NEW items via Events (EntityScroller listens to CRUD events)
            // Using queueMicrotask to batch all emissions together = smooth UI update
            const newItems: T[] = [];
            for (const item of serverResult.items) {
              if (!cachedIds.has(item.id)) {
                newItems.push(item);
              }
            }

            // Batch emit all new items in a microtask for smooth UI update
            if (newItems.length > 0) {
              queueMicrotask(() => {
                for (const item of newItems) {
                  // Emit via Events system so EntityScroller receives the update
                  Events.emit(`data:${params.collection}:created`, item);
                }
              });
            }

            const newCount = newItems.length;

            if (newCount > 0) {
              verbose && console.log(`🔄 data/list: REVALIDATED ${params.collection} (+${newCount} new items merged smoothly)`);
            } else {
              verbose && console.log(`🔄 data/list: REVALIDATED ${params.collection} (${serverResult.items.length} items, no new)`);
            }
          }
        });
      }).catch(err => {
        verbose && console.warn(`⚠️ data/list: REVALIDATE failed ${params.collection}:`, err);
      });
    };

    // Schedule during idle time to avoid blocking tab switches
    if (typeof requestIdleCallback !== 'undefined') {
      requestIdleCallback(doRevalidate, { timeout: 5000 });
    } else {
      // Fallback: use setTimeout with 0 to yield to event loop
      setTimeout(doRevalidate, 0);
    }
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
