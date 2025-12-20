/**
 * Data Daemon - Universal Storage Orchestrator
 * 
 * Heavy abstraction for organizational data with pluggable storage strategies:
 * - SQL backends: PostgreSQL, SQLite with relational concepts (tables, joins, indices)
 * - NoSQL backends: MongoDB, Redis with document concepts (collections, queries)
 * - File backends: JSON, Binary, Structured with filesystem organization
 * - Network backends: Distributed storage with consistency models
 * 
 * Supports both relational and document paradigms through unified interface
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { BaseEntity } from '../../../system/data/entities/BaseEntity';
import type {
  DataStorageAdapter,
  DataRecord,
  StorageQuery,
  StorageResult,
  StorageAdapterConfig,
  CollectionStats,
  StorageOperation,
  RecordData
} from './DataStorageAdapter';

// Import entity registry for proper entity instantiation during validation
import { getRegisteredEntity } from '../server/SqliteStorageAdapter';

// Import universal events for automatic event emission
import { Events } from '../../../system/core/shared/Events';
import { getDataEventName, DATA_EVENTS } from '../../../system/core/shared/EventConstants';

// Import paginated query management
import { PaginatedQueryManager } from './PaginatedQuery';
import type { OpenPaginatedQueryParams, PaginatedQueryHandle, PaginatedQueryPage } from './PaginatedQuery';

// Removed complex decorator dependency - using simple field validation instead

/**
 * Storage Strategy Configuration
 */
export interface StorageStrategyConfig {
  readonly strategy: 'sql' | 'nosql' | 'file' | 'memory' | 'network' | 'hybrid';
  readonly backend: string; // 'postgres', 'sqlite', 'mongodb', 'redis', 'json', etc.
  readonly namespace: string;
  readonly options?: Record<string, any>;
  readonly features?: {
    readonly enableTransactions?: boolean;
    readonly enableIndexing?: boolean;
    readonly enableReplication?: boolean;
    readonly enableSharding?: boolean;
    readonly enableCaching?: boolean;
  };
}

/**
 * Data Operation Context
 */
export interface DataOperationContext {
  readonly sessionId: UUID;
  readonly timestamp: string;
  readonly source: string;
  readonly transactionId?: UUID;
  readonly consistency?: 'eventual' | 'strong' | 'session';
}

/**
 * Entity Constructor Type with BaseEntity static methods
 */
type EntityConstructor = (new (...args: unknown[]) => BaseEntity) & typeof BaseEntity;

/**
 * Schema Validation Result
 */
export interface SchemaValidationResult {
  success: boolean;
  errors?: string[];
}

/**
 * Universal Data Daemon - Storage Strategy Abstraction
 *
 * Orchestrates data operations across any storage backend while maintaining
 * consistent interface regardless of underlying SQL/NoSQL/File strategy
 *
 * CLEAN DOMAIN-OWNED INTERFACE (like CommandDaemon.execute):
 *
 * OLD PATTERN (scattered, context-heavy):
 * const context = { sessionId, timestamp, source };
 * const result = await this.dataDaemon.create('users', userData, context);
 *
 * NEW PATTERN (domain-owned, auto-context):
 * const result = await DataDaemon.store<UserData>('users', userData);
 * const users = await DataDaemon.query<UserData>({ collection: 'users', filter: {...} });
 * const success = await DataDaemon.remove('users', userId);
 */
export class DataDaemon {
  private adapter: DataStorageAdapter;
  private config: StorageStrategyConfig;
  private isInitialized: boolean = false;
  private paginatedQueryManager: PaginatedQueryManager;

  // Track which collections have had their schema ensured (generic caching at daemon level)
  private ensuredSchemas: Set<string> = new Set();

  constructor(config: StorageStrategyConfig, adapter: DataStorageAdapter) {
    this.config = config;
    this.adapter = adapter;
    this.paginatedQueryManager = new PaginatedQueryManager();
  }
  
  /**
   * Initialize storage backend
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) return;
    
    const adapterConfig: StorageAdapterConfig = {
      type: this.config.backend as any,
      namespace: this.config.namespace,
      options: {
        ...this.config.options,
        strategy: this.config.strategy,
        features: this.config.features
      }
    };
    
    await this.adapter.initialize(adapterConfig);
    this.isInitialized = true;
  }
  
  /**
   * Create record - Universal interface for SQL INSERT or NoSQL insert
   */
  async create<T extends BaseEntity>(
    collection: string,
    data: T,
    context: DataOperationContext,
    suppressEvents: boolean = false
  ): Promise<T> {
    await this.ensureInitialized();

    // Ensure schema exists (orchestrate table creation via adapter)
    await this.ensureSchema(collection);

    // Validate context and data
    const validationResult = this.validateOperation(collection, data, context);
    if (!validationResult.success && !validationResult.data) {
      throw new Error(validationResult.error ?? 'Unknown error during data storage');
    }

    // Ensure entity has ID before validation
    const entityId = data.id ?? this.generateId();
    const completeData = { ...data, id: entityId };

    // Schema validation using entity registry and BaseEntity factory method - PROPER ARCHITECTURE
    const schemaResult = this.validateSchema(collection, completeData);
    if (!schemaResult.success) {
      const errorMessages = schemaResult.errors?.join(', ') || 'Schema validation failed';
      throw new Error(`Entity validation failed: ${errorMessages}`);
    }

    // Use the validated data as the entity (still need plain object for storage)
    const completeEntity = completeData as T;

    const record: DataRecord<T> = {
      id: entityId,
      collection,
      data: completeEntity,
      metadata: {
        createdAt: context.timestamp,
        updatedAt: context.timestamp,
        version: 1
      }
    };

    const result = await this.adapter.create(record);
    if (result.success && result.data) {
      const entity = result.data.data;

      // Emit created event via universal Events system (unless suppressed)
      if (DataDaemon.jtagContext && !suppressEvents) {
        const eventName = getDataEventName(collection, 'created');
        await Events.emit(DataDaemon.jtagContext, eventName, entity);
      }

      // Return the complete entity (already includes proper ID)
      return entity;
    } else {
      throw new Error(result.error ?? 'Unknown error during data storage');
    }
  }
  
  /**
   * Read single record - Universal interface for SQL SELECT or NoSQL findOne
   */
  async read<T extends BaseEntity>(
    collection: string,
    id: UUID,
    _context: DataOperationContext // TODO: use context for read consistency
  ): Promise<StorageResult<DataRecord<T>>> {
    await this.ensureInitialized();

    // Ensure schema exists before reading (prevents "no such table" errors)
    await this.ensureSchema(collection);

    const result = await this.adapter.read<T>(collection, id);

    // Deserialize entity at the boundary (create new object to avoid readonly mutation)
    if (result.success && result.data) {
      return {
        ...result,
        data: {
          ...result.data,
          data: this.deserializeEntity(result.data.data)
        }
      };
    }

    return result;
  }

  /**
   * Deserialize entity - convert storage types to domain types at boundary
   * SQLite stores timestamps as strings, but domain expects Date objects
   */
  private deserializeEntity<T extends BaseEntity>(entity: T): T {
    const deserialized: any = { ...entity };

    // Convert timestamp fields from string to Date
    if ('timestamp' in deserialized && typeof deserialized.timestamp === 'string') {
      deserialized.timestamp = new Date(deserialized.timestamp);
    }
    if ('createdAt' in deserialized && typeof deserialized.createdAt === 'string') {
      deserialized.createdAt = new Date(deserialized.createdAt);
    }
    if ('updatedAt' in deserialized && typeof deserialized.updatedAt === 'string') {
      deserialized.updatedAt = new Date(deserialized.updatedAt);
    }

    return deserialized as T;
  }

  /**
   * Query with complex filters - SQL WHERE clauses or NoSQL queries
   */
  async query<T extends BaseEntity>(
    query: StorageQuery,
    _context: DataOperationContext // TODO: use context for query consistency
  ): Promise<StorageResult<DataRecord<T>[]>> {
    await this.ensureInitialized();

    // Ensure schema exists before querying (prevents "no such table" errors)
    await this.ensureSchema(query.collection);

    const result = await this.adapter.query<T>(query);

    // Deserialize entities at the boundary (create new array to avoid readonly mutation)
    if (result.success && result.data) {
      return {
        ...result,
        data: result.data.map(record => ({
          ...record,
          data: this.deserializeEntity(record.data)
        }))
      };
    }

    return result;
  }
  
  /**
   * Update record - SQL UPDATE or NoSQL updateOne
   */
  async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    _context: DataOperationContext,
    incrementVersion: boolean = true
  ): Promise<T> {
    await this.ensureInitialized();

    // Read existing entity to merge with partial update
    // TODO: Performance optimization - Consider adding skipValidation flag for trusted internal updates,
    // or only validating fields that are actually being updated rather than the entire merged entity
    const readResult = await this.adapter.read(collection, id);
    if (!readResult.success || !readResult.data?.data) {
      throw new Error(`Entity not found for update: ${collection}/${id}`);
    }

    // Merge partial update with existing entity data
    const existingEntity = readResult.data.data;
    const mergedData = { ...existingEntity, ...data };

    // Validate the merged data before persisting
    const validationResult = this.validateSchema(collection, mergedData as Record<string, unknown>);
    if (!validationResult.success) {
      const errors = validationResult.errors?.join(', ') ?? 'Unknown validation error';
      console.error(`❌ DataDaemon: Update validation failed for ${collection}/${id}:`, errors);
      throw new Error(`Data validation failed for update: ${errors}`);
    }

    // Validation passed - proceed with update
    const result = await this.adapter.update<T>(collection, id, data, incrementVersion);
    if (result.success && result.data) {
      const entity = result.data.data;

      // Emit updated event via universal Events system
      if (DataDaemon.jtagContext) {
        const eventName = getDataEventName(collection, 'updated');
        await Events.emit(DataDaemon.jtagContext, eventName, entity);
      }

      return entity;
    } else {
      throw new Error(result.error ?? 'Unknown error during data update');
    }
  }
  
  /**
   * Delete record - SQL DELETE or NoSQL deleteOne
   */
  async delete(
    collection: string,
    id: UUID,
    context: DataOperationContext
  ): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();

    // Read entity before deletion for event emission
    const readResult = await this.adapter.read(collection, id);
    const entity = readResult.data?.data;

    const result = await this.adapter.delete(collection, id);

    // Emit deleted event if deletion was successful and we have the entity data
    if (result.success && entity && DataDaemon.jtagContext) {
      const eventName = getDataEventName(collection, 'deleted');
      await Events.emit(DataDaemon.jtagContext, eventName, entity);
    }

    return result;
  }
  
  /**
   * List collections/tables
   */
  async listCollections(context: DataOperationContext): Promise<StorageResult<string[]>> {
    await this.ensureInitialized();
    return await this.adapter.listCollections();
  }
  
  /**
   * Collection statistics - Record counts, schema info, indices
   */
  async getCollectionStats(
    collection: string,
    context: DataOperationContext
  ): Promise<StorageResult<CollectionStats>> {
    await this.ensureInitialized();
    return await this.adapter.getCollectionStats(collection);
  }
  
  /**
   * Batch operations - Transactions for SQL, bulk operations for NoSQL
   *
   * Emits individual CRUD events for each operation in the batch
   * to keep widgets in sync per-entity, not just "batch done"
   */
  async batch(
    operations: StorageOperation[],
    context: DataOperationContext
  ): Promise<StorageResult<any[]>> {
    await this.ensureInitialized();
    const result = await this.adapter.batch(operations);

    // Emit individual events for each successful operation in the batch
    if (result.success && result.data && DataDaemon.jtagContext) {
      for (let i = 0; i < operations.length; i++) {
        const operation = operations[i];
        const operationResult = result.data[i];

        // Map operation type to event operation name
        const eventOperationMap: Record<string, 'created' | 'updated' | 'deleted'> = {
          'create': 'created',
          'update': 'updated',
          'delete': 'deleted'
        };

        const eventOperation = eventOperationMap[operation.type];
        if (!eventOperation) continue; // Skip read operations

        const eventName = getDataEventName(operation.collection, eventOperation);

        // For create/update, emit with entity data
        if (operation.type === 'create' || operation.type === 'update') {
          await Events.emit(DataDaemon.jtagContext, eventName, operationResult);
        } else if (operation.type === 'delete') {
          await Events.emit(DataDaemon.jtagContext, eventName, { id: operation.id });
        }
      }
    }

    return result;
  }
  
  /**
   * Clear all data from all collections
   */
  async clear(): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();
    const result = await this.adapter.clear();

    // Emit cleared event if successful
    if (result.success && DataDaemon.jtagContext) {
      await Events.emit(DataDaemon.jtagContext, DATA_EVENTS.ALL.CLEARED, { all: true });
    }

    return result;
  }

  /**
   * Clear all data with detailed reporting (instance method)
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    await this.ensureInitialized();
    const result = await this.adapter.clearAll();

    // Emit cleared event if successful with details about what was cleared
    if (result.success && result.data && DataDaemon.jtagContext) {
      await Events.emit(DataDaemon.jtagContext, DATA_EVENTS.ALL.CLEARED, {
        all: true,
        tablesCleared: result.data.tablesCleared,
        recordsDeleted: result.data.recordsDeleted
      });
    }

    return result;
  }

  /**
   * Truncate all records from a specific collection
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();
    const result = await this.adapter.truncate(collection);

    // Emit truncated event if successful
    if (result.success && DataDaemon.jtagContext) {
      const eventName = getDataEventName(collection, 'truncated');
      await Events.emit(DataDaemon.jtagContext, eventName, { collection });
    }

    return result;
  }

  /**
   * Storage maintenance - VACUUM, reindex, cleanup
   */
  async maintenance(): Promise<void> {
    await this.ensureInitialized();
    await this.adapter.cleanup();
  }
  
  /**
   * Close daemon and storage connections
   */
  async close(): Promise<void> {
    if (this.isInitialized) {
      await this.adapter.close();
      this.isInitialized = false;
    }
  }
  
  /**
   * Shutdown daemon (alias for close)
   */
  async shutdown(): Promise<void> {
    await this.close();
  }
  
  /**
   * Ensure daemon is initialized before operations
   */
  private async ensureInitialized(): Promise<void> {
    if (!this.isInitialized) {
      await this.initialize();
    }
  }
  
  /**
   * Generate unique ID for records
   */
  private generateId(): UUID {
    return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as UUID;
  }
  
  /**
   * Validate operation parameters
   */
  private validateOperation(collection: string, data: any, context: DataOperationContext): StorageResult<any> {
    if (!collection || collection.trim() === '') {
      return {
        success: false,
        error: 'Collection name is required and cannot be empty'
      };
    }
    
    if (data === undefined || data === null) {
      return {
        success: false,
        error: 'Data is required and cannot be null or undefined'
      };
    }
    
    if (!context.sessionId || context.sessionId.trim() === '') {
      return {
        success: false,
        error: 'DataOperationContext.sessionId is required'
      };
    }
    
    if (!context.timestamp) {
      return {
        success: false,
        error: 'DataOperationContext.timestamp is required'
      };
    }

    return { success: true, data: null };
  }

  /**
   * Ensure collection schema exists (orchestrated by daemon, implemented by adapter)
   *
   * This is the SINGLE point where schema creation is orchestrated.
   * - Caches which collections are ensured (generic across all adapters)
   * - Delegates to adapter.ensureSchema() for actual implementation
   * - Adapter decides how to create schema (SQL table, Mongo collection, etc.)
   */
  private async ensureSchema(collection: string): Promise<void> {
    // Check cache first (generic caching at daemon level)
    if (this.ensuredSchemas.has(collection)) {
      return; // Already ensured this session
    }

    // Delegate to adapter (adapter knows table names, column names, SQL)
    const result = await this.adapter.ensureSchema(collection);
    if (!result.success) {
      throw new Error(`Failed to ensure schema for ${collection}: ${result.error}`);
    }

    // Cache success (generic - works for any adapter type)
    this.ensuredSchemas.add(collection);
  }

  /**
   * Validate entity data - generic validation using BaseEntity.validate()
   * ARCHITECTURE: Data daemon only knows BaseEntity, never specific entity types
   * Uses entity registry to create proper instances for validation
   *
   * NOTE: If no entity is registered, validation is SKIPPED (allows custom collections)
   * This matches SqliteStorageAdapter behavior which handles unregistered collections
   */
  private validateSchema<T extends BaseEntity>(collection: string, data: Record<string, unknown>): SchemaValidationResult {
    // Get entity class from registry - works generically with ANY registered entity type
    const EntityClass = getRegisteredEntity(collection) as EntityConstructor;
    if (!EntityClass) {
      // No entity registered - skip validation (custom collection manages its own schema)
      // This allows collections like "memories" that use direct SQL schema creation
      console.log(`⚠️ DataDaemon: No entity registered for "${collection}" - skipping validation (custom collection)`);
      return { success: true };
    }

    // Create proper entity instance using BaseEntity factory method
    const entityResult = EntityClass.create(data);
    if (!entityResult.success || !entityResult.entity) {
      console.error(`❌ DataDaemon: Entity creation failed for "${collection}":`, entityResult.error);
      return { success: false, errors: [entityResult.error || 'Entity creation failed'] };
    }

    // Call the entity's validation method to enforce validation rules
    const validationResult = entityResult.entity.validate();
    if (!validationResult.success) {
      console.error(`❌ DataDaemon: Entity validation failed for "${collection}":`, validationResult.error);
      return { success: false, errors: [validationResult.error || 'Validation failed'] };
    }

    return { success: true };
  }

  // =============================================
  // PAGINATED QUERY INTERFACE
  // =============================================

  /**
   * Open a paginated query and get a handle
   *
   * Uses entity's pagination config for defaults:
   * - Sorting field and direction
   * - Page size
   * - Cursor field
   */
  async openPaginatedQuery(params: OpenPaginatedQueryParams): Promise<PaginatedQueryHandle> {
    await this.ensureInitialized();

    // Get entity class to read pagination config
    const EntityClass = getRegisteredEntity(params.collection) as EntityConstructor;
    if (!EntityClass) {
      throw new Error(`No entity class registered for collection "${params.collection}"`);
    }

    const paginationConfig = EntityClass.getPaginationConfig();

    // Use entity defaults if not provided
    const orderBy = params.orderBy ?? [{
      field: paginationConfig.defaultSortField,
      direction: paginationConfig.defaultSortDirection
    }];
    const pageSize = params.pageSize ?? paginationConfig.defaultPageSize;

    // Get total count using efficient COUNT(*) query
    const countQuery: StorageQuery = {
      collection: params.collection,
      filter: params.filter
    };
    const countResult = await this.adapter.count(countQuery);
    const totalCount = countResult.success ? (countResult.data ?? 0) : 0;

    // Open query handle
    const handle = this.paginatedQueryManager.openQuery({
      collection: params.collection,
      filter: params.filter,
      orderBy,
      pageSize
    }, totalCount);

    return handle;
  }

  /**
   * Get next page from paginated query
   */
  async getNextPage<T extends BaseEntity>(queryId: UUID): Promise<PaginatedQueryPage<T>> {
    await this.ensureInitialized();

    const state = this.paginatedQueryManager.getQueryState(queryId);
    if (!state) {
      throw new Error(`Query handle ${queryId} not found`);
    }

    if (!state.hasMore) {
      // Return empty page if no more data
      return {
        items: [],
        pageNumber: state.currentPage,
        hasMore: false,
        totalCount: state.totalCount
      };
    }

    // Get entity class to read pagination config
    const EntityClass = getRegisteredEntity(state.collection) as EntityConstructor;
    if (!EntityClass) {
      throw new Error(`No entity class registered for collection "${state.collection}"`);
    }
    const paginationConfig = EntityClass.getPaginationConfig();

    // Build query with cursor if we have one
    const query: StorageQuery = {
      collection: state.collection,
      filter: state.filter,
      sort: state.orderBy?.map(o => ({ field: o.field, direction: o.direction })),
      limit: state.pageSize,
      ...(state.currentCursor && {
        cursor: {
          field: paginationConfig.cursorField,
          value: state.currentCursor,
          direction: state.orderBy?.[0]?.direction === 'desc' ? 'before' : 'after'
        }
      })
    };

    const result = await this.adapter.query<T>(query);
    if (!result.success || !result.data) {
      throw new Error(`Failed to query ${state.collection}: ${result.error}`);
    }

    const items = result.data.map(record => record.data);
    const hasMore = items.length === state.pageSize && (state.currentPage + 1) * state.pageSize < state.totalCount;

    // Update cursor from last item - keep native type for storage adapter
    let nextCursor: any;
    if (items.length > 0) {
      const lastItem = items[items.length - 1];
      nextCursor = (lastItem as any)[paginationConfig.cursorField];
      // Don't convert to string - let storage adapter handle native comparison
    }

    // Update state
    this.paginatedQueryManager.updateQueryState(queryId, nextCursor, hasMore);

    return {
      items,
      pageNumber: state.currentPage,
      hasMore,
      totalCount: state.totalCount
    };
  }

  /**
   * Close paginated query and free resources
   */
  closePaginatedQuery(queryId: UUID): void {
    this.paginatedQueryManager.closeQuery(queryId);
  }

  /**
   * Get active query handles (for debugging)
   */
  getActiveQueries(): UUID[] {
    return this.paginatedQueryManager.getActiveQueries();
  }

  // =============================================
  // CLEAN DOMAIN-OWNED STATIC INTERFACE
  // =============================================

  private static sharedInstance: DataDaemon | undefined;
  private static context: DataOperationContext | undefined;
  public static jtagContext: JTAGContext | undefined;

  /**
   * Initialize static DataDaemon context (called by system)
   */
  static initialize(
    instance: DataDaemon,
    context: DataOperationContext,
    jtagContext: JTAGContext
  ): void {
    DataDaemon.sharedInstance = instance;
    DataDaemon.context = context;
    DataDaemon.jtagContext = jtagContext;
  }

  /**
   * Store data with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.store<UserData>('users', userData);
   * const result = await DataDaemon.store<ChatMessageData>('messages', messageData, customId);
   */
  static async store<T extends BaseEntity>(
    collection: string,
    data: T
  ): Promise<T> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context || !DataDaemon.jtagContext) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }
    const entity = await DataDaemon.sharedInstance.create<T>(collection, data, DataDaemon.context);

    // ✨ Dual event emission - trigger BOTH local AND remote subscribers
    const eventName = BaseEntity.getEventName(collection, 'created');

    // 1. Emit to WebSocket clients (browser, remote CLI clients)
    if (DataDaemon.jtagContext) {
      // Events.emit() now triggers both remote AND local subscribers automatically
      // (includes checkWildcardSubscriptions() internally - see Events.ts:145)
      await Events.emit(DataDaemon.jtagContext, eventName, entity);
    }

    // console.log(`✅ DataDaemon.store: Event ${eventName} broadcast to both local and remote subscribers`);

    return entity;
  }

  /**
   * Query data with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const users = await DataDaemon.query<UserData>({ collection: 'users', filter: { active: true } });
   * const messages = await DataDaemon.query<ChatMessageData>({
   *   collection: 'messages',
   *   filter: { roomId: 'general' },
   *   sort: [{ field: 'timestamp', direction: 'desc' }],
   *   limit: 50
   * });
   */
  static async query<T extends BaseEntity>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.query<T>(query, DataDaemon.context);
  }

  /**
   * Read single record by ID with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const user = await DataDaemon.read<UserData>('users', userId);
   */
  static async read<T extends BaseEntity>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.read<T>(collection, id, DataDaemon.context);
  }

  /**
   * Update data with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.update<UserData>('users', userId, { lastActive: now() });
   */
  static async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    incrementVersion: boolean = true
  ): Promise<T> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context || !DataDaemon.jtagContext) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    const entity = await DataDaemon.sharedInstance.update<T>(collection, id, data, DataDaemon.context, incrementVersion);

    // ✨ Universal event emission - works anywhere!
    const eventName = BaseEntity.getEventName(collection, 'updated');
    await Events.emit(DataDaemon.jtagContext, eventName, entity);

    return entity;
  }

  /**
   * Remove data with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const success = await DataDaemon.remove('users', userId);
   */
  static async remove(collection: string, id: UUID, suppressEvents = false): Promise<StorageResult<boolean>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context || !DataDaemon.jtagContext) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    // Read entity before deletion for event emission
    const readResult = await DataDaemon.sharedInstance.read(collection, id, DataDaemon.context);
    const entity = readResult.data?.data;

    const deleteResult = await DataDaemon.sharedInstance.delete(collection, id, DataDaemon.context);

    // ✨ Universal event emission - works anywhere!
    // Skip if suppressEvents is true (for internal operations like archiving)
    if (deleteResult.success && entity && !suppressEvents) {
      const eventName = BaseEntity.getEventName(collection, 'deleted');
      await Events.emit(DataDaemon.jtagContext, eventName, entity);
    }

    return deleteResult;
  }

  /**
   * List collections with automatic context injection - CLEAN INTERFACE
   */
  static async listCollections(): Promise<StorageResult<string[]>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.listCollections(DataDaemon.context);
  }

  /**
   * Clear all data from all collections - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.clear();
   */
  static async clear(): Promise<StorageResult<boolean>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.clear();
  }

  /**
   * Clear all data with detailed reporting - CLEAN INTERFACE
   *
   * Provides comprehensive clearing with detailed statistics about what was removed.
   * Preserves database structure while removing all records. Perfect for reseeding.
   *
   * @example
   * const result = await DataDaemon.clearAll();
   * console.log(`Cleared ${result.data.recordsDeleted} records from ${result.data.tablesCleared.length} tables`);
   */
  static async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.clearAll();
  }

  /**
   * Truncate all records from a specific collection - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.truncate('users');
   */
  static async truncate(collection: string): Promise<StorageResult<boolean>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.truncate(collection);
  }

  /**
   * Batch operations with automatic context injection - CLEAN INTERFACE
   */
  static async batch(operations: StorageOperation[]): Promise<StorageResult<any[]>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.batch(operations, DataDaemon.context);
  }

  /**
   * Open paginated query with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const handle = await DataDaemon.openPaginatedQuery({
   *   collection: 'chat_messages',
   *   filter: { roomId: 'abc123' }
   * });
   */
  static async openPaginatedQuery(params: OpenPaginatedQueryParams): Promise<PaginatedQueryHandle> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.openPaginatedQuery(params);
  }

  /**
   * Get next page from paginated query - CLEAN INTERFACE
   *
   * @example
   * const page = await DataDaemon.getNextPage<ChatMessageEntity>(queryHandle);
   */
  static async getNextPage<T extends BaseEntity>(queryId: UUID): Promise<PaginatedQueryPage<T>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.getNextPage<T>(queryId);
  }

  /**
   * Close paginated query - CLEAN INTERFACE
   *
   * @example
   * DataDaemon.closePaginatedQuery(queryHandle);
   */
  static closePaginatedQuery(queryId: UUID): void {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    DataDaemon.sharedInstance.closePaginatedQuery(queryId);
  }

  /**
   * Get active query handles (for debugging) - CLEAN INTERFACE
   */
  static getActiveQueries(): UUID[] {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return DataDaemon.sharedInstance.getActiveQueries();
  }

  /**
   * Get description field for a collection - CLEAN INTERFACE
   *
   * Returns the field name marked as description in entity metadata,
   * or null if no description field is defined.
   *
   * @example
   * const descField = DataDaemon.getDescriptionFieldForCollection('users');
   * // Returns 'displayName' if UserEntity has @TextField({description: true})
   */
  static getDescriptionFieldForCollection(collection: string): string | null {
    // Import locally to avoid circular dependencies
    const { getRegisteredEntity } = require('../server/SqliteStorageAdapter');
    const { getDescriptionField } = require('../../../system/data/decorators/FieldDecorators');

    const EntityClass = getRegisteredEntity(collection);
    if (!EntityClass) {
      return null; // No entity registered for this collection
    }

    return getDescriptionField(EntityClass);
  }

  // =============================================
  // VECTOR SEARCH INTERFACE
  // =============================================

  /**
   * Perform vector similarity search - CLEAN INTERFACE
   *
   * @example
   * const results = await DataDaemon.vectorSearch<MemoryData>({
   *   collection: 'memories',
   *   queryText: 'user prefers detailed explanations',
   *   k: 10,
   *   similarityThreshold: 0.7
   * });
   */
  static async vectorSearch<T extends RecordData>(
    options: import('./VectorSearchTypes').VectorSearchOptions
  ): Promise<StorageResult<import('./VectorSearchTypes').VectorSearchResponse<T>>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    // Check if adapter supports vector search
    const adapter = (DataDaemon.sharedInstance as any).adapter as any;
    if (!adapter.vectorSearch) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector search'
      };
    }

    return await adapter.vectorSearch(options);
  }

  /**
   * Generate embedding for text - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.generateEmbedding({
   *   text: 'We should use TypeScript for type safety',
   *   model: { name: 'all-minilm', dimensions: 384, provider: 'ollama' }
   * });
   */
  static async generateEmbedding(
    request: import('./VectorSearchTypes').GenerateEmbeddingRequest
  ): Promise<StorageResult<import('./VectorSearchTypes').GenerateEmbeddingResponse>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    // Check if adapter supports embedding generation
    const adapter = (DataDaemon.sharedInstance as any).adapter as any;
    if (!adapter.generateEmbedding) {
      return {
        success: false,
        error: 'Current storage adapter does not support embedding generation'
      };
    }

    return await adapter.generateEmbedding(request);
  }

  /**
   * Index vector for a record - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.indexVector({
   *   collection: 'memories',
   *   id: memoryId,
   *   embedding: [0.123, -0.456, 0.789, ...],
   *   metadata: { embeddingModel: 'all-minilm', generatedAt: new Date().toISOString() }
   * });
   */
  static async indexVector(
    request: import('./VectorSearchTypes').IndexVectorRequest
  ): Promise<StorageResult<boolean>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    const adapter = (DataDaemon.sharedInstance as any).adapter as any;
    if (!adapter.indexVector) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector indexing'
      };
    }

    return await adapter.indexVector(request);
  }

  /**
   * Backfill vectors for existing records - CLEAN INTERFACE
   *
   * @example
   * const result = await DataDaemon.backfillVectors({
   *   collection: 'memories',
   *   textField: 'content',
   *   batchSize: 100
   * }, (progress) => {
   *   console.log(`Processed ${progress.processed}/${progress.total} records`);
   * });
   */
  static async backfillVectors(
    request: import('./VectorSearchTypes').BackfillVectorsRequest,
    onProgress?: (progress: import('./VectorSearchTypes').BackfillVectorsProgress) => void
  ): Promise<StorageResult<import('./VectorSearchTypes').BackfillVectorsProgress>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    const adapter = (DataDaemon.sharedInstance as any).adapter as any;
    if (!adapter.backfillVectors) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector backfilling'
      };
    }

    return await adapter.backfillVectors(request, onProgress);
  }

  /**
   * Get vector index statistics - CLEAN INTERFACE
   *
   * @example
   * const stats = await DataDaemon.getVectorIndexStats('memories');
   */
  static async getVectorIndexStats(
    collection: string
  ): Promise<StorageResult<import('./VectorSearchTypes').VectorIndexStats>> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    const adapter = (DataDaemon.sharedInstance as any).adapter as any;
    if (!adapter.getVectorIndexStats) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector index stats'
      };
    }

    return await adapter.getVectorIndexStats(collection);
  }

  /**
   * Get vector search capabilities - CLEAN INTERFACE
   */
  static async getVectorSearchCapabilities(): Promise<import('./VectorSearchTypes').VectorSearchCapabilities | null> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    const adapter = (DataDaemon.sharedInstance as any).adapter as any;
    if (!adapter.getVectorSearchCapabilities) {
      return null;
    }

    return await adapter.getVectorSearchCapabilities();
  }
}

/**
 * Storage Strategy Factory - Plugin System
 * 
 * Creates appropriate adapters based on storage strategy:
 * - SQL strategies → SQL adapters
 * - NoSQL strategies → Document adapters  
 * - File strategies → Filesystem adapters
 * - Network strategies → Distributed adapters
 */
export interface StorageStrategyFactory {
  createAdapter(config: StorageStrategyConfig): DataStorageAdapter;
  getSupportedStrategies(): readonly string[];
  getSupportedBackends(strategy: string): readonly string[];
}