/**
 * Data Daemon Browser - Offline-First Storage
 *
 * ALL operations use local-first pattern:
 * - Reads: localStorage first, server fallback
 * - Writes: localStorage immediately, server sync in background
 *
 * No hardcoded collection lists - everything is cached.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { DataDaemonBase, type DataOperationPayload } from '../shared/DataDaemonBase';
import type { DataRecord, StorageResult } from '../shared/DataStorageAdapter';
import { OfflineStorageAdapter } from './OfflineStorageAdapter';

const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

/**
 * Data Daemon Browser - ALL collections use offline-first storage
 */
export class DataDaemonBrowser extends DataDaemonBase {
  private offlineAdapter: OfflineStorageAdapter | null = null;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  protected async initialize(): Promise<void> {
    this.offlineAdapter = new OfflineStorageAdapter(this.context, this.router);
    verbose() && console.log(`🗄️ ${this.toString()}: Browser data daemon initialized (offline-first for all collections)`);
  }

  protected async handleCreate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    if (this.offlineAdapter && payload.collection && payload.data?.id) {
      return await this.offlineAdapter.create(payload.collection, payload.data as any, payload.sessionId) as StorageResult<DataRecord<any>>;
    }
    return await this.router.routeToServer<StorageResult<DataRecord<any>>>('data', payload, payload.sessionId);
  }

  protected async handleRead(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    if (this.offlineAdapter && payload.collection && payload.id) {
      return await this.offlineAdapter.read(payload.collection, payload.id, payload.sessionId) as StorageResult<DataRecord<any>>;
    }
    return await this.router.routeToServer<StorageResult<DataRecord<any>>>('data', payload, payload.sessionId);
  }

  protected async handleQuery(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>[]>> {
    if (this.offlineAdapter && payload.query?.collection) {
      return await this.offlineAdapter.query(payload.query.collection, payload.query, payload.sessionId) as StorageResult<DataRecord<any>[]>;
    }
    return await this.router.routeToServer<StorageResult<DataRecord<any>[]>>('data', payload, payload.sessionId);
  }

  protected async handleUpdate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    if (this.offlineAdapter && payload.collection && payload.id) {
      return await this.offlineAdapter.update(payload.collection, payload.id, payload.data as Record<string, unknown>, payload.sessionId) as StorageResult<DataRecord<any>>;
    }
    return await this.router.routeToServer<StorageResult<DataRecord<any>>>('data', payload, payload.sessionId);
  }

  protected async handleDelete(payload: DataOperationPayload): Promise<StorageResult<boolean>> {
    if (this.offlineAdapter && payload.collection && payload.id) {
      return await this.offlineAdapter.delete(payload.collection, payload.id, payload.sessionId);
    }
    return await this.router.routeToServer<StorageResult<boolean>>('data', payload, payload.sessionId);
  }

  protected async handleListCollections(payload: DataOperationPayload): Promise<StorageResult<string[]>> {
    return await this.router.routeToServer<StorageResult<string[]>>('data', payload, payload.sessionId);
  }

  protected async handleGetStats(payload: DataOperationPayload): Promise<StorageResult<any>> {
    return await this.router.routeToServer<StorageResult<any>>('data', payload, payload.sessionId);
  }

  protected async handleBatch(payload: DataOperationPayload): Promise<StorageResult<any[]>> {
    return await this.router.routeToServer<StorageResult<any[]>>('data', payload, payload.sessionId);
  }

  getOfflineStatus(): { isOnline: boolean; pendingOperations: number; isSyncing: boolean } | null {
    return this.offlineAdapter?.getStatus() ?? null;
  }

  debug(): void {
    console.group('DataDaemonBrowser Debug');
    this.offlineAdapter?.debug();
    console.groupEnd();
  }
}
