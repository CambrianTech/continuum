/**
 * Data Daemon Server - JTAG Integration
 * 
 * Server-side daemon that integrates DataDaemon with JTAG system.
 * Uses FileStorageAdapter by default, pluggable for other backends.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { DataDaemonBase, type DataOperationPayload } from '../shared/DataDaemonBase';
import { DataDaemon, type StorageStrategyConfig, type DataOperationContext } from '../shared/DataDaemon';
import { FileStorageAdapter } from './FileStorageAdapter';
import type { DataRecord, StorageQuery, StorageResult } from '../shared/DataStorageAdapter';

/**
 * Data Daemon Server - JTAG Server Integration
 */
export class DataDaemonServer extends DataDaemonBase {
  private dataDaemon: DataDaemon;
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
    
    // Create storage configuration - file-based by default
    const storageConfig: StorageStrategyConfig = {
      strategy: 'file',
      backend: 'json',
      namespace: context.uuid, // Use context UUID as namespace
      options: {
        basePath: '.continuum/jtag/data'
      },
      features: {
        enableTransactions: false,
        enableIndexing: false,
        enableReplication: false,
        enableSharding: false,
        enableCaching: true
      }
    };
    
    // Initialize DataDaemon with factory-based approach
    this.dataDaemon = new DataDaemon(storageConfig);
  }
  
  /**
   * Initialize data daemon
   */
  protected async initialize(): Promise<void> {
    await this.dataDaemon.initialize();
    console.log(`🗄️ ${this.toString()}: Data daemon server initialized`);
  }
  
  /**
   * Handle create operation using DataDaemon
   */
  protected async handleCreate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.create(payload.collection!, payload.data, context, payload.id);
  }
  
  /**
   * Handle read operation using DataDaemon
   */
  protected async handleRead(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.read(payload.collection!, payload.id!, context);
  }
  
  /**
   * Handle query operation using DataDaemon
   */
  protected async handleQuery(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>[]>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.query(payload.query!, context);
  }
  
  /**
   * Handle update operation using DataDaemon
   */
  protected async handleUpdate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.update(payload.collection!, payload.id!, payload.data, context);
  }
  
  /**
   * Handle delete operation using DataDaemon
   */
  protected async handleDelete(payload: DataOperationPayload): Promise<StorageResult<boolean>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.delete(payload.collection!, payload.id!, context);
  }
  
  /**
   * Handle list collections operation using DataDaemon
   */
  protected async handleListCollections(payload: DataOperationPayload): Promise<StorageResult<string[]>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.listCollections(context);
  }
  
  /**
   * Handle get stats operation using DataDaemon
   */
  protected async handleGetStats(payload: DataOperationPayload): Promise<StorageResult<any>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.getCollectionStats(payload.collection!, context);
  }
  
  /**
   * Handle batch operations using DataDaemon
   */
  protected async handleBatch(payload: DataOperationPayload): Promise<StorageResult<any[]>> {
    const context = this.createDataContext('data-daemon-server');
    return await this.dataDaemon.batch(payload.operations!, context);
  }
  
  /**
   * Shutdown data daemon and clean up storage
   */
  async shutdown(): Promise<void> {
    await this.dataDaemon.shutdown();
  }
}