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
import { DefaultStorageAdapterFactory } from './DefaultStorageAdapterFactory';
import type { DataRecord, StorageQuery, StorageResult } from '../shared/DataStorageAdapter';
import { DATABASE_PATHS, DATABASE_FILES } from '../../../system/data/config/DatabaseConfig';
import { BaseEntity } from '../../../system/data/entities/BaseEntity';
// import { Events } from '../../../system/core/shared/Events';
import { RouterRegistry } from '../../../system/core/shared/RouterRegistry';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';

/**
 * Data Daemon Server - JTAG Server Integration
 */
export class DataDaemonServer extends DataDaemonBase {
  protected log: ComponentLogger;
  private dataDaemon: DataDaemon;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);

    // Create storage configuration - SQLite-based for proper data storage
    const storageConfig: StorageStrategyConfig = {
      strategy: 'sql',
      backend: 'sqlite',
      namespace: context.uuid, // Use context UUID as namespace
      options: {
        basePath: DATABASE_PATHS.DATA_DIR,
        databaseName: DATABASE_FILES.SQLITE_FILENAME,
        foreignKeys: false  // Disable foreign key constraints to avoid constraint violations
      },
      features: {
        enableTransactions: true,
        enableIndexing: true,
        enableReplication: false,
        enableSharding: false,
        enableCaching: true
      }
    };

    // Create adapter via factory - server-side dependency injection
    const factory = new DefaultStorageAdapterFactory();
    const adapterConfig = {
      type: storageConfig.backend as any,
      namespace: storageConfig.namespace,
      options: storageConfig.options
    };
    const adapter = factory.createAdapter(adapterConfig);

    // Initialize DataDaemon with injected adapter
    this.dataDaemon = new DataDaemon(storageConfig, adapter);
  }
  
  /**
   * Initialize data daemon
   */
  protected async initialize(): Promise<void> {
    // Register router for universal event system
    RouterRegistry.register(this.context, this.router);
    this.log.info('Registered router with RouterRegistry');

    // Initialize entity registry before DataDaemon initialization
    await this.initializeEntityRegistry();

    await this.dataDaemon.initialize();

    // Initialize static DataDaemon interface for commands to use
    const context = this.createDataContext('data-daemon-server');
    DataDaemon.initialize(this.dataDaemon, context, this.context);

    this.log.info('Data daemon server initialized with SQLite backend');

    // Initialize CodeDaemon for code/read operations
    const { initializeCodeDaemon } = await import('../../code-daemon/server/CodeDaemonServer');
    await initializeCodeDaemon(this.context);
    this.log.info('Code daemon initialized');

    // Initialize SystemDaemon for efficient system config access
    const { SystemDaemon } = await import('../../system-daemon/shared/SystemDaemon');
    await SystemDaemon.initialize(this.context);
    this.log.info('System daemon initialized');

    // Emit system ready event so other daemons can proceed with initialization
    const { Events } = await import('../../../system/core/shared/Events');
    const { SYSTEM_EVENTS } = await import('../../../system/core/shared/EventConstants');
    await Events.emit(SYSTEM_EVENTS.READY, { daemon: 'data' });
    this.log.info('Emitted system:ready event');
  }

  /**
   * Initialize entity registry for SqliteStorageAdapter
   */
  private async initializeEntityRegistry(): Promise<void> {
    try {
      const { initializeEntityRegistry } = await import('./EntityRegistry');
      initializeEntityRegistry();
      this.log.info('Entity registry initialized');
    } catch (error) {
      this.log.error('Failed to initialize entity registry:', error);
      throw error;
    }
  }
  
  /**
   * Emit CRUD event - centralized event emission for all data operations
   * NOTE: This method is no longer used - event emission now handled by DataDaemon layer
   * via universal Events system (system/core/shared/Events.ts)
   */
  private async emitCrudEvent(operation: 'created' | 'updated' | 'deleted', collection: string, entity: any): Promise<void> {
    // Event emission now handled by DataDaemon layer via universal Events system
    // try {
    //   const eventName = BaseEntity.getEventName(collection, operation);
    //
    //   // Create mock commander object with router for Events.emit()
    //   const mockCommander = {
    //     router: this.router
    //   } as any;
    //
    //   await Events.emit(eventName, entity, this.context, mockCommander);
    //   console.log(`✅ DataDaemonServer: Emitted ${eventName}`);
    // } catch (error) {
    //   console.error(`❌ DataDaemonServer: Failed to emit ${operation} event for ${collection}:`, error);
    // }
  }

  /**
   * Handle create operation using DataDaemon
   */
  protected async handleCreate(payload: DataOperationPayload): Promise<StorageResult<DataRecord<any>>> {
    const context = this.createDataContext('data-daemon-server');
    const entity = await this.dataDaemon.create(payload.collection!, payload.data, context);

    // Event emission handled by DataDaemon layer via universal Events system
    // await this.emitCrudEvent('created', payload.collection!, entity);

    // Return as StorageResult format
    return {
      success: true,
      data: {
        id: entity.id,
        collection: payload.collection!,
        data: entity,
        metadata: {
          createdAt: entity.createdAt?.toISOString() || new Date().toISOString(),
          updatedAt: entity.updatedAt?.toISOString() || new Date().toISOString(),
          version: entity.version || 1
        }
      }
    };
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
    const entity = await this.dataDaemon.update(payload.collection!, payload.id!, payload.data, context);

    // Event emission handled by DataDaemon layer via universal Events system
    // await this.emitCrudEvent('updated', payload.collection!, entity);

    return {
      success: true,
      data: {
        id: entity.id,
        collection: payload.collection!,
        data: entity,
        metadata: {
          createdAt: entity.createdAt.toISOString(),
          updatedAt: entity.updatedAt.toISOString(),
          version: entity.version
        }
      }
    };
  }
  
  /**
   * Handle delete operation using DataDaemon
   */
  protected async handleDelete(payload: DataOperationPayload): Promise<StorageResult<boolean>> {
    const context = this.createDataContext('data-daemon-server');

    // Read entity before deletion for event emission
    const readResult = await this.dataDaemon.read(payload.collection!, payload.id!, context);
    const entity = readResult.data?.data;

    // Perform deletion
    const deleteResult = await this.dataDaemon.delete(payload.collection!, payload.id!, context);

    // Event emission handled by DataDaemon layer via universal Events system
    // if (deleteResult.success && entity) {
    //   await this.emitCrudEvent('deleted', payload.collection!, entity);
    // }

    return deleteResult;
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