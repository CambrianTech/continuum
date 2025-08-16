/**
 * Data Daemon Base - Shared Daemon Foundation
 * 
 * Abstract base class for both browser and server data daemons.
 * Follows JTAG daemon pattern with proper message handling.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import type { DataRecord, StorageQuery, StorageResult } from './DataStorageAdapter';
import { type DataOperationContext } from './DataDaemon';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';

/**
 * Data operation payload types
 */
export interface DataOperationPayload extends JTAGPayload {
  readonly operation: 'create' | 'read' | 'query' | 'update' | 'delete' | 'list_collections' | 'get_stats' | 'batch';
  readonly collection?: string;
  readonly id?: UUID;
  readonly data?: any;
  readonly query?: StorageQuery;
  readonly operations?: any[];
}

export const createDataOperationPayload = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<DataOperationPayload>, 'context' | 'sessionId'>
): DataOperationPayload => createPayload(context, sessionId, {
  operation: data.operation ?? 'read',
  ...data
});

/**
 * Abstract Data Daemon Base - Shared Foundation
 * 
 * Provides common data daemon functionality for browser and server implementations.
 */
export abstract class DataDaemonBase extends DaemonBase {
  public readonly subpath: string = 'data';
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('data-daemon', context, router);
  }
  
  /**
   * Handle incoming data operation messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as DataOperationPayload;
    
    try {
      let result: StorageResult<any>;
      
      switch (payload.operation) {
        case 'create':
          result = await this.handleCreate(payload);
          break;
        case 'read':
          result = await this.handleRead(payload);
          break;
        case 'query':
          result = await this.handleQuery(payload);
          break;
        case 'update':
          result = await this.handleUpdate(payload);
          break;
        case 'delete':
          result = await this.handleDelete(payload);
          break;
        case 'list_collections':
          result = await this.handleListCollections(payload);
          break;
        case 'get_stats':
          result = await this.handleGetStats(payload);
          break;
        case 'batch':
          result = await this.handleBatch(payload);
          break;
        default:
          result = {
            success: false,
            error: `Unknown data operation: ${payload.operation}`
          };
      }
      
      // Wrap StorageResult in BaseResponsePayload
      return createBaseResponse(result.success, this.context, payload.sessionId, {
        ...result
      });
      
    } catch (error: any) {
      return createBaseResponse(false, this.context, payload.sessionId, {
        error: `Data daemon error: ${error.message}`
      });
    }
  }
  
  /**
   * Abstract data operation handlers - implemented by browser/server
   */
  protected abstract handleCreate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>>;
  protected abstract handleRead(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>>;
  protected abstract handleQuery(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>[]>>;
  protected abstract handleUpdate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>>;
  protected abstract handleDelete(payload: DataOperationPayload): Promise<StorageResult<boolean>>;
  protected abstract handleListCollections(payload: DataOperationPayload): Promise<StorageResult<string[]>>;
  protected abstract handleGetStats(payload: DataOperationPayload): Promise<StorageResult<any>>;
  protected abstract handleBatch(payload: DataOperationPayload): Promise<StorageResult<any[]>>;
  
  /**
   * Create data operation context
   */
  protected createDataContext(source: string = 'data-daemon'): DataOperationContext {
    return {
      source,
      timestamp: new Date().toISOString(),
      namespace: this.context.uuid,
      consistency: 'strong'
    };
  }
}