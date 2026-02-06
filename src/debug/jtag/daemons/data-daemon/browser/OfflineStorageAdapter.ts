/**
 * OfflineStorageAdapter - Offline-First Dual-Storage
 *
 * Implements the offline-first pattern like Twitter/Gmail:
 * - Reads: localStorage first (instant), server fallback
 * - Writes: localStorage immediately + SyncQueue for server
 * - Offline: Works entirely from localStorage
 * - Reconnect: SyncQueue drains to server automatically
 *
 * Entities exist in BOTH localStorage and server. This ensures:
 * 1. Fast reads (no network latency)
 * 2. Offline capability (app works without server)
 * 3. Eventual consistency (sync when reconnected)
 *
 * Part of the offline-first dual-storage ORM architecture.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { StorageResult, StorageQuery } from '../shared/DataStorageAdapter';
import { createDataOperationPayload, type DataOperationPayload } from '../shared/DataDaemonBase';
import { Events } from '../../../system/core/shared/Events';
import { LocalStorageDataBackend } from './LocalStorageDataBackend';
import { IndexedDBBackend } from './IndexedDBBackend';
import { SyncQueue, type SyncOperation } from './SyncQueue';
import { ConnectionStatus } from './ConnectionStatus';
import { OFFLINE_CACHEABLE_COLLECTIONS } from '../../../system/shared/Constants';

/**
 * OfflineStorageAdapter - The core dual-storage adapter
 *
 * Usage (in DataDaemonBrowser):
 * ```typescript
 * const adapter = new OfflineStorageAdapter(router);
 *
 * // All operations are local-first
 * const result = await adapter.read('user_states', userId);
 * await adapter.update('user_states', userId, { currentTab: 'settings' });
 * ```
 */
export class OfflineStorageAdapter {
  private readonly context: JTAGContext;
  private readonly router: JTAGRouter;
  private readonly syncQueue: SyncQueue;
  private readonly connectionStatus: ConnectionStatus;
  private isSyncing: boolean = false;

  // Debounce user_states updates to prevent main thread blocking
  private pendingUserStateUpdates: Map<UUID, { id: UUID; [key: string]: unknown }> = new Map();
  private userStateUpdateScheduled = false;
  private static readonly USER_STATE_DEBOUNCE_MS = 100;

  constructor(context: JTAGContext, router: JTAGRouter) {
    this.context = context;
    this.router = router;
    this.syncQueue = new SyncQueue();
    this.connectionStatus = new ConnectionStatus();

    // Sync when reconnecting
    this.connectionStatus.onReconnect(() => {
      console.log('OfflineStorageAdapter: Reconnected, processing sync queue...');
      this.processSyncQueue();
    });

    // Subscribe to server events for cache invalidation
    this.subscribeToServerEvents();

    // Process any pending operations from previous session
    if (this.connectionStatus.isOnline && this.syncQueue.hasItems()) {
      console.log(`OfflineStorageAdapter: Found ${this.syncQueue.length} pending operations from previous session`);
      this.processSyncQueue();
    }
  }

  /**
   * Read an entity - local-first
   *
   * 1. Try localStorage (instant)
   * 2. If not found AND online → fetch from server, cache locally
   * 3. If not found AND offline → return null (graceful degradation)
   */
  async read(collection: string, id: UUID, sessionId: UUID): Promise<StorageResult<unknown>> {
    // 1. Try localStorage first (instant)
    const cached = await LocalStorageDataBackend.read(collection, id);
    if (cached.success && cached.entity) {
      return { success: true, data: cached.entity };
    }

    // 2. If online, fetch from server and cache
    if (this.connectionStatus.isOnline) {
      const serverResult = await this.forwardToServer('read', {
        collection,
        id,
        sessionId,
      });

      if (serverResult.success && serverResult.data) {
        // Cache the result locally
        await LocalStorageDataBackend.create(collection, serverResult.data as any);
      }

      return serverResult;
    }

    // 3. Offline and not cached - graceful degradation
    return { success: false, error: `Entity ${id} not found (offline)` };
  }

  /**
   * Create an entity - local + queue
   *
   * 1. Write to localStorage immediately
   * 2. Queue for server sync
   * 3. Attempt immediate sync if online
   */
  async create(collection: string, entity: { id: UUID; [key: string]: unknown }, sessionId: UUID): Promise<StorageResult<unknown>> {
    // 1. Write to localStorage immediately
    const localResult = await LocalStorageDataBackend.create(collection, entity as any);
    if (!localResult.success) {
      // localStorage write failed - still try server
      console.warn('OfflineStorageAdapter: Local create failed:', localResult.error);
    }

    // 2. Queue for server sync
    this.syncQueue.enqueue({
      type: 'create',
      collection,
      id: entity.id,
      data: entity,
    });

    // 3. Attempt immediate sync if online
    if (this.connectionStatus.isOnline) {
      this.processSyncQueue(); // Fire-and-forget
    }

    return { success: true, data: entity };
  }

  /**
   * Update an entity - local + queue
   *
   * 1. Update localStorage immediately
   * 2. Queue for server sync
   * 3. Attempt immediate sync if online
   */
  async update(collection: string, id: UUID, data: Record<string, unknown>, sessionId: UUID): Promise<StorageResult<unknown>> {
    // 1. Update localStorage immediately
    const localResult = await LocalStorageDataBackend.update(collection, id, data as any);
    if (!localResult.success) {
      // Entity doesn't exist locally yet - try to create it
      const createResult = await LocalStorageDataBackend.create(collection, { id, ...data } as any);
      if (!createResult.success) {
        console.warn('OfflineStorageAdapter: Local update/create failed:', createResult.error);
      }
    }

    // 2. Queue for server sync
    this.syncQueue.enqueue({
      type: 'update',
      collection,
      id,
      data,
    });

    // 3. Attempt immediate sync if online
    if (this.connectionStatus.isOnline) {
      this.processSyncQueue(); // Fire-and-forget
    }

    return { success: true };
  }

  /**
   * Delete an entity - local + queue
   */
  async delete(collection: string, id: UUID, sessionId: UUID): Promise<StorageResult<boolean>> {
    // 1. Delete from localStorage immediately
    await LocalStorageDataBackend.delete(collection, id);

    // 2. Queue for server sync
    this.syncQueue.enqueue({
      type: 'delete',
      collection,
      id,
    });

    // 3. Attempt immediate sync if online
    if (this.connectionStatus.isOnline) {
      this.processSyncQueue();
    }

    return { success: true, data: true };
  }

  /**
   * Query entities - local-first with server fallback
   *
   * For queries we try local first, then merge with server if online.
   */
  async query(collection: string, query: StorageQuery, sessionId: UUID): Promise<StorageResult<unknown[]>> {
    // 1. Try local first
    const cached = await LocalStorageDataBackend.list(collection, query?.filter as Record<string, unknown>);
    const hasLocalData = cached.success && cached.entities && cached.entities.length > 0;

    // 2. If we have local data, return it immediately
    if (hasLocalData) {
      return { success: true, data: cached.entities! };
    }

    // 3. If online, fetch from server and cache results
    if (this.connectionStatus.isOnline) {
      const serverResult = await this.forwardToServer('query', {
        collection,
        query,
        sessionId,
      }) as StorageResult<unknown[]>;

      if (serverResult.success && Array.isArray(serverResult.data)) {
        // Cache all results locally
        for (const entity of serverResult.data) {
          if (entity && typeof entity === 'object' && 'id' in entity) {
            // Only create if not already cached
            const existing = await LocalStorageDataBackend.read(collection, (entity as any).id);
            if (!existing.success) {
              await LocalStorageDataBackend.create(collection, entity as any);
            }
          }
        }
      }

      return serverResult;
    }

    // 4. Offline with no local data - return empty
    return { success: true, data: cached.entities || [] };
  }

  /**
   * Process the sync queue - send pending operations to server
   *
   * Called automatically on reconnect and after local writes.
   * Uses FIFO order (oldest operations first).
   */
  private async processSyncQueue(): Promise<void> {
    if (this.isSyncing) {
      return; // Prevent concurrent sync
    }

    this.isSyncing = true;
    let successCount = 0;
    let failCount = 0;

    try {
      while (this.syncQueue.hasItems() && this.connectionStatus.isOnline) {
        const op = this.syncQueue.peek();
        if (!op) break;

        try {
          await this.syncOperationToServer(op);
          this.syncQueue.dequeue(); // Success - remove from queue
          successCount++;
        } catch (error) {
          // Server rejected - log and stop processing
          console.error(`OfflineStorageAdapter: Sync failed for ${op.type} ${op.collection}:${op.id}:`, error);
          failCount++;
          break; // Stop on first failure to preserve order
        }
      }

      if (successCount > 0 || failCount > 0) {
        console.log(`OfflineStorageAdapter: Sync complete - ${successCount} succeeded, ${failCount} failed, ${this.syncQueue.length} remaining`);
      }
    } finally {
      this.isSyncing = false;
    }
  }

  /**
   * Sync a single operation to the server
   */
  private async syncOperationToServer(op: SyncOperation): Promise<void> {
    const payload = createDataOperationPayload(this.context, '' as UUID, {
      operation: op.type,
      collection: op.collection,
      id: op.id,
      data: op.data,
    });

    const result = await this.router.routeToServer<StorageResult<unknown>>('data', payload, '' as UUID);

    if (!result.success) {
      throw new Error(result.error || `Sync failed for ${op.type}`);
    }
  }

  /**
   * Forward an operation to the server with proper payload construction
   */
  private async forwardToServer(
    operation: 'read' | 'create' | 'update' | 'delete' | 'query',
    params: {
      collection: string;
      id?: UUID;
      data?: unknown;
      query?: StorageQuery;
      sessionId: UUID;
    }
  ): Promise<StorageResult<unknown>> {
    try {
      const payload = createDataOperationPayload(this.context, params.sessionId, {
        operation,
        collection: params.collection,
        id: params.id,
        data: params.data,
        query: params.query,
      });

      return await this.router.routeToServer<StorageResult<unknown>>('data', payload, params.sessionId);
    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: `Server error: ${errorMessage}` };
    }
  }

  /**
   * Subscribe to server events for cache invalidation
   *
   * Auto-wired from OFFLINE_CACHEABLE_COLLECTIONS (Constants.ts).
   * When server pushes changes, update local IndexedDB cache.
   *
   * ⚠️ NO per-collection code here - all driven by the constant
   */
  private subscribeToServerEvents(): void {
    // Auto-subscribe to all cacheable collections
    for (const collection of OFFLINE_CACHEABLE_COLLECTIONS) {
      // Subscribe to created events
      Events.subscribe(`data:${collection}:created`, async (data: { id: UUID; [key: string]: unknown }) => {
        if (data && data.id) {
          await IndexedDBBackend.create(collection, data as any);
        }
      });

      // Subscribe to updated events
      Events.subscribe(`data:${collection}:updated`, async (data: { id: UUID; [key: string]: unknown }) => {
        if (data && data.id) {
          await IndexedDBBackend.update(collection, data.id, data as any);
        }
      });

      // Subscribe to deleted events
      Events.subscribe(`data:${collection}:deleted`, async (data: { id: UUID }) => {
        if (data && data.id) {
          await IndexedDBBackend.delete(collection, data.id);
        }
      });
    }

    console.log(`OfflineStorageAdapter: Auto-wired cache invalidation for ${OFFLINE_CACHEABLE_COLLECTIONS.length} collections`);
  }

  /**
   * Schedule debounced flush of user_states updates.
   * Uses requestIdleCallback when available for non-blocking writes.
   */
  private scheduleUserStateFlush(): void {
    if (this.userStateUpdateScheduled) return;
    this.userStateUpdateScheduled = true;

    // Use setTimeout for debouncing, then requestIdleCallback for non-blocking write
    setTimeout(() => {
      this.userStateUpdateScheduled = false;

      // Flush all pending updates
      const updates = new Map(this.pendingUserStateUpdates);
      this.pendingUserStateUpdates.clear();

      if (updates.size === 0) return;

      // Use requestIdleCallback if available for non-blocking localStorage writes
      const flushFn = async () => {
        for (const [id, data] of updates) {
          await LocalStorageDataBackend.update('user_states', id, data as any);
        }
      };

      if ('requestIdleCallback' in window) {
        (window as any).requestIdleCallback(flushFn, { timeout: 500 });
      } else {
        // Fallback: use microtask to at least batch
        queueMicrotask(flushFn);
      }
    }, OfflineStorageAdapter.USER_STATE_DEBOUNCE_MS);
  }

  /**
   * Get current status for debugging
   */
  getStatus(): {
    isOnline: boolean;
    pendingOperations: number;
    isSyncing: boolean;
  } {
    return {
      isOnline: this.connectionStatus.isOnline,
      pendingOperations: this.syncQueue.length,
      isSyncing: this.isSyncing,
    };
  }

  /**
   * Debug: log current state
   */
  debug(): void {
    console.group('OfflineStorageAdapter Debug');
    console.log('Status:', this.getStatus());
    this.connectionStatus.debug();
    this.syncQueue.debug();
    LocalStorageDataBackend.debug();
    console.groupEnd();
  }
}
