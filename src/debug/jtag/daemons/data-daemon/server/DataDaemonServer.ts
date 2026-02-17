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
import { getDatabasePath, getDatabaseDir } from '../../../system/config/ServerConfig';

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
        basePath: getDatabaseDir(),  // Expand $HOME in path
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
   * PHASE 1: Core initialization (BLOCKING)
   * Sets up database and emits system:ready - minimum for other daemons to proceed.
   */
  protected async initialize(): Promise<void> {
    const coreStart = Date.now();
    this.log.info('🚀 DataDaemonServer: CORE init starting...');

    // Register router for universal event system
    RouterRegistry.register(this.context, this.router);

    // Initialize entity registry before DataDaemon initialization
    await this.initializeEntityRegistry();

    // Initialize the database connection
    await this.dataDaemon.initialize();

    // Initialize static DataDaemon interface for commands to use
    const context = this.createDataContext('data-daemon-server');
    DataDaemon.initialize(this.dataDaemon, context, this.context);

    // CRITICAL: Emit system:ready ASAP so other daemons can proceed
    // This unblocks UserDaemon, RoomMembershipDaemon, TrainingDaemon, etc.
    const { Events } = await import('../../../system/core/shared/Events');
    const { SYSTEM_EVENTS } = await import('../../../system/core/shared/EventConstants');
    await Events.emit(SYSTEM_EVENTS.READY, { daemon: 'data' });

    const coreMs = Date.now() - coreStart;
    this.log.info(`✅ DataDaemonServer: CORE init complete (${coreMs}ms) - system:ready emitted`);
    this.log.info(`   Archive handles, CodeDaemon, SystemDaemon will init in background`);
  }

  /**
   * PHASE 2: Deferred initialization (NON-BLOCKING)
   * Sets up archive databases, CodeDaemon, SystemDaemon - runs after daemon is READY.
   */
  protected async initializeDeferred(): Promise<void> {
    this.log.info('🔄 DataDaemonServer: DEFERRED init starting...');
    const deferredStart = Date.now();

    // Register custom JSON file adapters for config collections
    await this.registerJsonFileAdapters();

    // Register database handles for multi-database operations (archiving, etc.)
    await this.registerDatabaseHandles();
    this.log.debug('Database handles registered');

    // NOTE: Rust data-daemon worker connection removed - ORM uses ORMRustClient → continuum-core directly
    // RustWorkerStorageAdapter was dead code (never invoked by ORM)

    // Initialize CodeDaemon for code/read operations
    const { initializeCodeDaemon } = await import('../../code-daemon/server/CodeDaemonServer');
    await initializeCodeDaemon(this.context);
    this.log.debug('Code daemon initialized');

    // Initialize SystemDaemon for efficient system config access
    const { SystemDaemon } = await import('../../system-daemon/shared/SystemDaemon');
    await SystemDaemon.initialize(this.context);
    this.log.debug('System daemon initialized');

    // Initialize governance notifications (vote events → chat messages)
    const { initializeGovernanceNotifications } = await import('../../../system/governance/GovernanceNotifications');
    initializeGovernanceNotifications();
    this.log.debug('Governance notifications initialized');

    // Initialize sentinel escalation (sentinel lifecycle → persona inbox)
    const { initializeSentinelEscalation } = await import('../../../system/sentinel/SentinelEscalationService');
    initializeSentinelEscalation();
    this.log.debug('Sentinel escalation service initialized');

    // Initialize sentinel trigger service (auto-execute sentinels on event/cron/immediate)
    const { initializeSentinelTriggers } = await import('../../../system/sentinel/SentinelTriggerService');
    await initializeSentinelTriggers();
    this.log.debug('Sentinel trigger service initialized');

    const deferredMs = Date.now() - deferredStart;
    this.log.info(`✅ DataDaemonServer: DEFERRED init complete (${deferredMs}ms)`);
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
   * Register JSON file adapters for config collections
   *
   * This allows collections like 'logging_config' to use JSON files
   * instead of SQLite, enabling both:
   * - Command access: ./jtag data/read --collection=logging_config
   * - Manual editing: vim .continuum/logging.json
   */
  private async registerJsonFileAdapters(): Promise<void> {
    const { SingleJsonFileAdapter } = await import('./SingleJsonFileAdapter');

    // Register logging_config collection -> .continuum/logging.json
    const loggingAdapter = new SingleJsonFileAdapter();
    await loggingAdapter.initialize({
      type: 'file' as any,  // Use 'file' type, actual behavior determined by adapter class
      namespace: 'logging',
      options: {
        filePath: '.continuum/logging.json',
        collection: 'logging_config',
        singletonId: 'singleton',
        prettyPrint: true
      }
    });
    DataDaemon.registerCollectionAdapter('logging_config', loggingAdapter);

    this.log.info('Registered JSON file adapters: logging_config');
  }

  /**
   * Register database handles for multi-database operations
   * Creates 'primary' (main DB) and 'archive' (archive DB) handles
   */
  private async registerDatabaseHandles(): Promise<void> {
    const { DatabaseHandleRegistry } = await import('./DatabaseHandleRegistry');
    const { DATABASE_PATHS } = await import('../../../system/data/config/DatabaseConfig');
    const fs = await import('fs');
    const path = await import('path');

    const registry = DatabaseHandleRegistry.getInstance();

    // Register 'primary' handle pointing to main database
    // Note: emitEvents=false for archive operations to prevent UI spam during bulk moves
    const primaryDbPath = getDatabasePath();  // Expand $HOME in path
    this.log.info(`Registering 'primary' handle: ${primaryDbPath}`);

    const primaryHandle = await registry.open('sqlite', {
      filename: primaryDbPath
    }, { emitEvents: false });

    // Register alias so archive code can use 'primary' instead of UUID
    registry.registerAlias('primary', primaryHandle);

    // Create archive directory if it doesn't exist
    const archiveDir = path.join(path.dirname(primaryDbPath), 'archive');
    if (!fs.existsSync(archiveDir)) {
      fs.mkdirSync(archiveDir, { recursive: true });
      this.log.info(`Created archive directory: ${archiveDir}`);
    }

    // Find or create active archive file
    // Use source database name with -NNN suffix (e.g., database-001.sqlite)
    const sourceDbName = path.basename(primaryDbPath, '.sqlite');
    const archiveFiles = fs.readdirSync(archiveDir)
      .filter((f: string) => f.startsWith(`${sourceDbName}-`) && f.endsWith('.sqlite'))
      .sort()
      .reverse(); // Most recent first

    let archiveDbPath: string;

    if (archiveFiles.length > 0) {
      // Use most recent archive file
      archiveDbPath = path.join(archiveDir, archiveFiles[0]);
      this.log.info(`Using existing archive file: ${archiveDbPath}`);
    } else {
      // Create first archive file
      archiveDbPath = path.join(archiveDir, `${sourceDbName}-001.sqlite`);
      this.log.info(`Creating new archive file: ${archiveDbPath}`);
    }

    // Register 'archive' handle with event suppression
    // Archive operations should not emit events to prevent UI spam
    const archiveHandle = await registry.open('sqlite', {
      filename: archiveDbPath
    }, { emitEvents: false });

    // Register alias so archive code can use 'archive' instead of UUID
    registry.registerAlias('archive', archiveHandle);

    this.log.info(`Registered 'archive' handle: ${archiveDbPath} (emitEvents=false)`);
  }
  
  // DEPRECATED: connectRustDataWorker() removed
  // ORM now uses ORMRustClient → continuum-core DataModule directly
  // RustWorkerStorageAdapter was dead code (never invoked by ORM)
  // Vector search migrated to continuum-core DataModule

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