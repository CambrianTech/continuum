/**
 * Data Daemon Browser - Client Forwarding
 * 
 * Browser-side implementation that forwards all data operations to server
 * No local storage - all data operations go through server
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { DataDaemonBase, type DataOperationPayload } from '../shared/DataDaemonBase';
import type { DataRecord, StorageResult } from '../shared/DataStorageAdapter';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';

/**
 * Data Daemon Browser - Pure Forwarding Client
 * 
 * All operations are forwarded to server - no browser-side data storage
 */
export class DataDaemonBrowser extends DataDaemonBase {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize browser data daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🗄️ ${this.toString()}: Browser data daemon initialized - forwarding to server`);
  }
  
  /**
   * All browser handlers forward to server via router
   */
  
  protected async handleCreate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_create_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<DataRecord<any>>;
  }
  
  protected async handleRead(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_read_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<DataRecord<any>>;
  }
  
  protected async handleQuery(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>[]>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_query_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<DataRecord<any>[]>;
  }
  
  protected async handleUpdate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_update_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<DataRecord<any>>;
  }
  
  protected async handleDelete(payload: DataOperationPayload): Promise<StorageResult<boolean>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_delete_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<boolean>;
  }
  
  protected async handleListCollections(payload: DataOperationPayload): Promise<StorageResult<string[]>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_list_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<string[]>;
  }
  
  protected async handleGetStats(payload: DataOperationPayload): Promise<StorageResult<any>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_stats_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<any>;
  }
  
  protected async handleBatch(payload: DataOperationPayload): Promise<StorageResult<any[]>> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      'server/data',
      payload,
      `data_batch_${Date.now()}`
    );
    const response = await this.router.postMessage(message);
    return response as StorageResult<any[]>;
  }
}