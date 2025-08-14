/**
 * Data Daemon - Modern JSON-based Persistence
 * 
 * Microarchitecture approach: Simple document store without SQL complexity
 * Single responsibility: Data persistence and retrieval for all JTAG entities
 * Size: ~100-150 lines maximum per microarchitecture principles
 * 
 * What it does:
 * - JSON document storage/retrieval
 * - Collection-based organization 
 * - CRUD operations with strong typing
 * 
 * What it does NOT do:
 * - Complex queries/joins → Use multiple simple queries
 * - Schema validation → Types handle this
 * - Transactions → Keep operations atomic
 * - Caching → Keep it simple
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { 
  DataResult, 
  DataParams, 
  DataOperation,
  DataCreateParams,
  DataReadParams,
  DataUpdateParams,
  DataDeleteParams,
  DataListParams,
  DataExistsParams
} from './DataTypes';
import { 
  createDataCreateResult,
  createDataReadResult,
  createDataUpdateResult,
  createDataDeleteResult,
  createDataListResult,
  createDataExistsResult
} from './DataTypes';
import { ValidationError } from '../../../system/core/types/ErrorTypes';

/**
 * Data Daemon - JSON Document Persistence
 * 
 * Core responsibility: Simple, fast JSON-based data operations
 * No SQL complexity - just documents in collections
 */
export abstract class DataDaemon extends DaemonBase {
  public readonly subpath: string = 'data';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('data', context, router);
  }

  /**
   * Initialize data daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🗄️ ${this.toString()}: Data daemon initialized - JSON document store ready`);
  }

  /**
   * Handle incoming data messages - base implementation forwards to server
   */
  async handleMessage(message: JTAGMessage): Promise<DataResult> {
    const operation = message.endpoint as DataOperation;
    const params = message.payload as DataParams;
    
    console.log(`📨 ${this.toString()}: Handling ${operation} operation`);
    
    return await this.routeDataOperation(operation, params);
  }

  /**
   * Route data operations to appropriate handlers
   */
  protected async routeDataOperation(operation: DataOperation, params: DataParams): Promise<DataResult> {
    switch (operation) {
      case 'create':
        return await this.handleCreate(params as DataCreateParams);
      case 'read':
        return await this.handleRead(params as DataReadParams);
      case 'update':
        return await this.handleUpdate(params as DataUpdateParams);
      case 'delete':
        return await this.handleDelete(params as DataDeleteParams);
      case 'list':
        return await this.handleList(params as DataListParams);
      case 'exists':
        return await this.handleExists(params as DataExistsParams);
      default:
        return this.createErrorResponse(`Unknown data operation: ${operation}`, params);
    }
  }

  /**
   * Handle create operation - base forwards to server
   */
  protected async handleCreate(params: DataCreateParams): Promise<DataResult> {
    return await this.forwardToServer('create', params);
  }

  /**
   * Handle read operation - base forwards to server
   */
  protected async handleRead(params: DataReadParams): Promise<DataResult> {
    return await this.forwardToServer('read', params);
  }

  /**
   * Handle update operation - base forwards to server
   */
  protected async handleUpdate(params: DataUpdateParams): Promise<DataResult> {
    return await this.forwardToServer('update', params);
  }

  /**
   * Handle delete operation - base forwards to server
   */
  protected async handleDelete(params: DataDeleteParams): Promise<DataResult> {
    return await this.forwardToServer('delete', params);
  }

  /**
   * Handle list operation - base forwards to server
   */
  protected async handleList(params: DataListParams): Promise<DataResult> {
    return await this.forwardToServer('list', params);
  }

  /**
   * Handle exists operation - base forwards to server
   */
  protected async handleExists(params: DataExistsParams): Promise<DataResult> {
    return await this.forwardToServer('exists', params);
  }

  /**
   * Forward to server (browser implementation)
   */
  protected async forwardToServer(operation: DataOperation, params: DataParams): Promise<DataResult> {
    if (this.context.environment !== 'server') {
      const message = JTAGMessageFactory.createRequest(
        this.context,
        'data',
        operation,
        params,
        `data-${operation}-${Date.now()}`
      );
      return await this.executeRemote(message, 'server') as DataResult;
    }
    
    // Server environment should override the specific handlers
    return this.createErrorResponse(`DataDaemonServer must override handle${operation.charAt(0).toUpperCase() + operation.slice(1)} for actual data operations`, params);
  }

  /**
   * Create error response
   */
  protected createErrorResponse(errorMessage: string, params: DataParams): DataResult {
    const errorResult = createDataReadResult(this.context, params.sessionId, {
      success: false,
      found: false,
      error: new ValidationError('operation', errorMessage)
    });
    
    return errorResult as DataResult;
  }
}