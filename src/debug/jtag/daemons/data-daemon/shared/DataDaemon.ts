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
  StorageQueryWithJoin,
  StorageResult,
  StorageAdapterConfig,
  CollectionStats,
  StorageOperation,
  RecordData,
  CollectionSchema,
  SchemaField,
  SchemaFieldType,
  SchemaIndex
} from './DataStorageAdapter';

// Import entity registry for proper entity instantiation during validation
import { getRegisteredEntity, ENTITY_REGISTRY } from '../server/EntityRegistry';

// Import field metadata for schema extraction (daemon extracts, adapter uses)
import {
  getFieldMetadata,
  hasFieldMetadata,
  getCompositeIndexes,
  type FieldMetadata,
  type FieldType
} from '../../../system/data/decorators/FieldDecorators';

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

  // Per-collection adapter registry - allows different storage backends per collection
  // Example: logging_config uses JSON file, users uses SQLite
  private static collectionAdapters: Map<string, DataStorageAdapter> = new Map();

  /**
   * Register a custom adapter for a specific collection
   *
   * This allows different collections to use different storage backends:
   * - logging_config -> SingleJsonFileAdapter (editable JSON file)
   * - users -> SqliteStorageAdapter (SQLite database)
   *
   * @param collection - Collection name
   * @param adapter - Storage adapter instance (must be initialized)
   */
  static registerCollectionAdapter(collection: string, adapter: DataStorageAdapter): void {
    DataDaemon.collectionAdapters.set(collection, adapter);
    console.log(`📁 DataDaemon: Registered custom adapter for collection '${collection}'`);
  }

  /**
   * Get adapter for a collection (custom or default)
   */
  private getAdapterForCollection(collection: string): DataStorageAdapter {
    return DataDaemon.collectionAdapters.get(collection) || this.adapter;
  }

  /**
   * Static version - get adapter for collection using sharedInstance
   */
  private static getAdapterForCollectionStatic(collection: string): DataStorageAdapter {
    const customAdapter = DataDaemon.collectionAdapters.get(collection);
    if (customAdapter) return customAdapter;
    if (!DataDaemon.sharedInstance) throw new Error('DataDaemon not initialized');
    return DataDaemon.sharedInstance.adapter;
  }

  /**
   * Create DataDaemon instance
   *
   * @param config - Storage strategy configuration
   * @param adapter - Storage adapter instance
   * @param adapterAlreadyInitialized - If true, skip calling adapter.initialize()
   *        Use this when creating temporary DataDaemon instances with adapters
   *        that are already initialized (e.g., from DatabaseHandleRegistry)
   */
  constructor(config: StorageStrategyConfig, adapter: DataStorageAdapter, adapterAlreadyInitialized: boolean = false) {
    this.config = config;
    this.adapter = adapter;
    this.paginatedQueryManager = new PaginatedQueryManager();
    // If adapter is already initialized, mark ourselves as initialized too
    // This prevents re-calling adapter.initialize() with incomplete config
    this.isInitialized = adapterAlreadyInitialized;
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

    // Get adapter for this collection (may be custom adapter like JSON file)
    const adapter = this.getAdapterForCollection(collection);

    // Ensure schema exists via default adapter (DDL).
    // Custom adapters (Rust) handle DML only — schema creation stays in TypeScript.
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

    const result = await adapter.create(record);
    if (result.success && result.data) {
      const entity = result.data.data;

      // Emit created event via universal Events system (unless suppressed)
      // Fire-and-forget: DB write succeeded, event notification is non-blocking
      if (DataDaemon.jtagContext && !suppressEvents) {
        const eventName = getDataEventName(collection, 'created');
        Events.emit(DataDaemon.jtagContext, eventName, entity)
          .catch(err => console.error(`DataDaemon.create event emit failed for ${collection}:`, err));
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

    // Get adapter for this collection (may be custom adapter like JSON file)
    const adapter = this.getAdapterForCollection(collection);

    // Ensure schema exists via default adapter (DDL).
    await this.ensureSchema(collection);

    const result = await adapter.read<T>(collection, id);

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

    // Get adapter for this collection (may be custom adapter like JSON file)
    const adapter = this.getAdapterForCollection(query.collection);

    // Ensure schema exists via default adapter (DDL).
    await this.ensureSchema(query.collection);

    const result = await adapter.query<T>(query);

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

    // Get adapter for this collection (may be custom adapter like JSON file)
    const adapter = this.getAdapterForCollection(collection);

    // Ensure schema exists via default adapter (DDL).
    await this.ensureSchema(collection);

    // Read existing entity to merge with partial update
    // TODO: Performance optimization - Consider adding skipValidation flag for trusted internal updates,
    // or only validating fields that are actually being updated rather than the entire merged entity
    const readResult = await adapter.read(collection, id);
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
    const result = await adapter.update<T>(collection, id, data, incrementVersion);
    if (result.success && result.data) {
      const entity = result.data.data;

      // Emit updated event via universal Events system
      // Fire-and-forget: DB write succeeded, event notification is non-blocking
      if (DataDaemon.jtagContext) {
        const eventName = getDataEventName(collection, 'updated');
        Events.emit(DataDaemon.jtagContext, eventName, entity)
          .catch(err => console.error(`DataDaemon.update event emit failed for ${collection}:`, err));
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
    context: DataOperationContext,
    suppressEvents: boolean = false
  ): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();

    // Get adapter for this collection (may be custom adapter like JSON file)
    const adapter = this.getAdapterForCollection(collection);

    // Ensure schema exists via default adapter (DDL).
    await this.ensureSchema(collection);

    // Read entity before deletion for event emission
    const readResult = await adapter.read(collection, id);
    const entity = readResult.data?.data;

    const result = await adapter.delete(collection, id);

    // Emit deleted event if deletion was successful and we have the entity data
    // Fire-and-forget: DB delete succeeded, event notification is non-blocking
    if (result.success && entity && DataDaemon.jtagContext && !suppressEvents) {
      const eventName = getDataEventName(collection, 'deleted');
      Events.emit(DataDaemon.jtagContext, eventName, entity)
        .catch(err => console.error(`DataDaemon.delete event emit failed for ${collection}:`, err));
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

        // For create/update, emit with entity data (fire-and-forget)
        if (operation.type === 'create' || operation.type === 'update') {
          Events.emit(DataDaemon.jtagContext, eventName, operationResult)
            .catch(err => console.error(`DataDaemon.batch event emit failed:`, err));
        } else if (operation.type === 'delete') {
          Events.emit(DataDaemon.jtagContext, eventName, { id: operation.id })
            .catch(err => console.error(`DataDaemon.batch event emit failed:`, err));
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

    // Emit cleared event if successful (fire-and-forget)
    if (result.success && DataDaemon.jtagContext) {
      Events.emit(DataDaemon.jtagContext, DATA_EVENTS.ALL.CLEARED, { all: true })
        .catch(err => console.error('DataDaemon.clear event emit failed:', err));
    }

    return result;
  }

  /**
   * Clear all data with detailed reporting (instance method)
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    await this.ensureInitialized();
    const result = await this.adapter.clearAll();

    // Emit cleared event if successful with details about what was cleared (fire-and-forget)
    if (result.success && result.data && DataDaemon.jtagContext) {
      Events.emit(DataDaemon.jtagContext, DATA_EVENTS.ALL.CLEARED, {
        all: true,
        tablesCleared: result.data.tablesCleared,
        recordsDeleted: result.data.recordsDeleted
      }).catch(err => console.error('DataDaemon.clearAll event emit failed:', err));
    }

    return result;
  }

  /**
   * Truncate all records from a specific collection
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();
    const result = await this.adapter.truncate(collection);

    // Emit truncated event if successful (fire-and-forget)
    if (result.success && DataDaemon.jtagContext) {
      const eventName = getDataEventName(collection, 'truncated');
      Events.emit(DataDaemon.jtagContext, eventName, { collection })
        .catch(err => console.error(`DataDaemon.truncate event emit failed for ${collection}:`, err));
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
   * Map decorator FieldType to adapter SchemaFieldType
   *
   * ARCHITECTURE: This translation happens in the daemon (which knows entities)
   * so the adapter doesn't need to know about decorator types.
   */
  private mapFieldTypeToSchemaType(fieldType: FieldType): SchemaFieldType {
    switch (fieldType) {
      case 'primary':
      case 'foreign_key':
        return 'uuid';
      case 'date':
        return 'date';
      case 'text':
      case 'enum':
        return 'string';
      case 'json':
        return 'json';
      case 'number':
        return 'number';
      case 'boolean':
        return 'boolean';
      default:
        return 'string';
    }
  }

  /**
   * Extract CollectionSchema from entity decorators
   *
   * ARCHITECTURE: Daemon knows entities and their decorators.
   * It extracts schema and passes it to the adapter in a generic format.
   * Adapter doesn't need to know about ENTITY_REGISTRY or decorators.
   */
  private extractCollectionSchema(collection: string): CollectionSchema | undefined {
    const entityClass = ENTITY_REGISTRY.get(collection) as EntityConstructor | undefined;
    if (!entityClass || !hasFieldMetadata(entityClass)) {
      // No entity registered or no field metadata - return undefined
      // Adapter will use fallback behavior for unregistered collections
      return undefined;
    }

    // Extract field metadata
    const fieldMetadata = getFieldMetadata(entityClass);
    const fields: SchemaField[] = [];

    for (const [fieldName, metadata] of fieldMetadata) {
      fields.push({
        name: fieldName,
        type: this.mapFieldTypeToSchemaType(metadata.fieldType),
        indexed: metadata.options?.index,
        unique: metadata.options?.unique,
        nullable: metadata.options?.nullable,
        maxLength: metadata.options?.maxLength
      });
    }

    // Extract composite indexes
    const compositeIndexes = getCompositeIndexes(entityClass);
    const indexes: SchemaIndex[] = compositeIndexes.map(idx => ({
      name: idx.name,
      fields: idx.fields,
      unique: idx.unique
    }));

    return {
      collection,
      fields,
      indexes: indexes.length > 0 ? indexes : undefined
    };
  }

  /**
   * Ensure collection schema exists (orchestrated by daemon, implemented by adapter)
   *
   * This is the SINGLE point where schema creation is orchestrated.
   * - Extracts schema from entity decorators (daemon's job - knows entities)
   * - Caches which collections are ensured (generic across all adapters)
   * - Passes schema to adapter.ensureSchema() for implementation
   * - Adapter decides how to create schema (SQL table, Mongo collection, etc.)
   */
  private async ensureSchema(collection: string): Promise<void> {
    // Check cache first (generic caching at daemon level)
    if (this.ensuredSchemas.has(collection)) {
      return; // Already ensured this session
    }

    // Extract schema from entity decorators (daemon knows entities, adapter doesn't)
    const schema = this.extractCollectionSchema(collection);

    // Delegate to adapter with schema (adapter knows how to implement in native format)
    const result = await this.adapter.ensureSchema(collection, schema);
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

    // Get total count
    const countQuery: StorageQuery = {
      collection: params.collection,
      filter: params.filter
    };
    const countResult = await this.adapter.query(countQuery);
    const totalCount = countResult.success ? (countResult.data?.length ?? 0) : 0;

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
   * Ensure schema exists on any adapter for a collection.
   * Extracts schema from entity decorators and caches it on the adapter.
   *
   * Use this when bypassing DataDaemon (e.g., per-persona dbHandle adapters)
   * to ensure the adapter's schema manager has the schema before queries.
   */
  static async ensureAdapterSchema(adapter: DataStorageAdapter, collection: string): Promise<void> {
    if (!DataDaemon.sharedInstance) {
      throw new Error('DataDaemon not initialized');
    }
    const schema = DataDaemon.sharedInstance.extractCollectionSchema(collection);
    if (schema) {
      await adapter.ensureSchema(collection, schema);
    }
  }

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
    data: T,
    suppressEvents: boolean = false
  ): Promise<T> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context || !DataDaemon.jtagContext) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }
    // Instance create() handles event emission internally (line 251-253)
    // No duplicate emission here — was previously emitting twice per write
    return await DataDaemon.sharedInstance.create<T>(collection, data, DataDaemon.context, suppressEvents);
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
   * Count records matching query filters using SQL COUNT(*) - CLEAN INTERFACE
   *
   * CRITICAL: Uses SQL aggregation, NOT fetching all rows!
   * For efficiency, always use count() instead of query().length when
   * you only need the count.
   */
  static async count(query: StorageQuery): Promise<StorageResult<number>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    // Ensure schema before counting (same as query() path)
    await DataDaemon.sharedInstance.ensureSchema(query.collection);

    return await DataDaemon.sharedInstance.adapter.count(query);
  }

  /**
   * Query with JOINs for optimal loading - CLEAN INTERFACE
   *
   * Uses queryWithJoin for loading related data in a single query (4.5x faster than N+1).
   *
   * @example
   * const messages = await DataDaemon.queryWithJoin<MessageWithSender>({
   *   collection: 'chat_messages',
   *   filter: { roomId: 'general' },
   *   joins: [{
   *     collection: 'users',
   *     alias: 'sender',
   *     localField: 'senderId',
   *     foreignField: 'id',
   *     type: 'left',
   *     select: ['displayName', 'userType']
   *   }],
   *   limit: 50
   * });
   */
  static async queryWithJoin<T extends RecordData>(query: StorageQueryWithJoin): Promise<StorageResult<DataRecord<T>[]>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    // Ensure schema for main collection and all joined collections
    await DataDaemon.sharedInstance.ensureSchema(query.collection);
    if (query.joins) {
      for (const join of query.joins) {
        await DataDaemon.sharedInstance.ensureSchema(join.collection);
      }
    }

    return await DataDaemon.sharedInstance.adapter.queryWithJoin<T>(query);
  }

  /**
   * Read single record by ID with automatic context injection - CLEAN INTERFACE
   *
   * Returns the entity directly (unwrapped), or null if not found.
   * Consistent with store() and update() which also return T directly.
   *
   * @example
   * const user = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);
   * if (user) { console.log(user.displayName); }
   */
  static async read<T extends BaseEntity>(collection: string, id: UUID): Promise<T | null> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    const result = await DataDaemon.sharedInstance.read<T>(collection, id, DataDaemon.context);
    if (!result.success || !result.data) return null;
    return result.data.data;
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
    // Instance update() handles event emission internally (line 399-402)
    // No duplicate emission here — was previously emitting twice per write
    return await DataDaemon.sharedInstance.update<T>(collection, id, data, DataDaemon.context, incrementVersion);
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

    // Instance delete() handles entity read + event emission internally
    // No duplicate read or emission here — was previously doing both twice per delete
    return await DataDaemon.sharedInstance.delete(collection, id, DataDaemon.context, suppressEvents);
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
    const { getRegisteredEntity } = require('../server/EntityRegistry');
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

    // Ensure schema before vector search
    await DataDaemon.sharedInstance.ensureSchema(options.collection);

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