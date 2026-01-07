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

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

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
    verbose() && console.log(`🗄️ ${this.toString()}: Browser data daemon initialized - forwarding to server`);
  }
  
  /**
   * All browser handlers forward to server via router
   */
  
  protected async handleCreate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    return await this.router.routeToServer<StorageResult<DataRecord<any>>>('data', payload, payload.sessionId);
  }
  
  protected async handleRead(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    return await this.router.routeToServer<StorageResult<DataRecord<any>>>('data', payload, payload.sessionId);
  }
  
  protected async handleQuery(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>[]>> {
    return await this.router.routeToServer<StorageResult<DataRecord<any>[]>>('data', payload, payload.sessionId);
  }
  
  protected async handleUpdate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    return await this.router.routeToServer<StorageResult<DataRecord<any>>>('data', payload, payload.sessionId);
  }
  
  protected async handleDelete(payload: DataOperationPayload): Promise<StorageResult<boolean>> {
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
}