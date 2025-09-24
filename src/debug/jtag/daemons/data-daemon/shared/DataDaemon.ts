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
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
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
 * Entity Constructor Type
 */
type EntityConstructor = new (...args: unknown[]) => BaseEntity;

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
 * const users = await DataDaemon.query<UserData>({ collection: 'users', filters: {...} });
 * const success = await DataDaemon.remove('users', userId);
 */
export class DataDaemon {
  private adapter: DataStorageAdapter;
  private config: StorageStrategyConfig;
  private isInitialized: boolean = false;

  constructor(config: StorageStrategyConfig, adapter: DataStorageAdapter) {
    this.config = config;
    this.adapter = adapter;
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
    context: DataOperationContext
  ): Promise<T> {
    await this.ensureInitialized();
    
    // Validate context and data
    const validationResult = this.validateOperation(collection, data, context);
    if (!validationResult.success && !validationResult.data) {
      throw new Error(validationResult.error ?? 'Unknown error during data storage');
    }

    // Schema validation using decorator metadata
    const schemaResult = this.validateSchema(collection, data);
    if (!schemaResult.success) {
      const errorMessages = schemaResult.errors?.join(', ') || 'Schema validation failed';
      throw new Error(`Schema validation failed for collection "${collection}": ${errorMessages}`);
    }
    
    // Ensure entity has ID before storage
    const entityId = data.id ?? this.generateId();
    const completeEntity = { ...data, id: entityId } as T;

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
      // Return the complete entity (already includes proper ID)
      return result.data.data;
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
    return await this.adapter.read<T>(collection, id);
  }
  
  /**
   * Query with complex filters - SQL WHERE clauses or NoSQL queries
   */
  async query<T extends BaseEntity>(
    query: StorageQuery,
    _context: DataOperationContext // TODO: use context for query consistency
  ): Promise<StorageResult<DataRecord<T>[]>> {
    await this.ensureInitialized();
    return await this.adapter.query<T>(query);
  }
  
  /**
   * Update record - SQL UPDATE or NoSQL updateOne
   */
  async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    data: Partial<T>,
    context: DataOperationContext,
    incrementVersion: boolean = true
  ): Promise<T> {
    await this.ensureInitialized();
    const result = await this.adapter.update<T>(collection, id, data, incrementVersion);
    if (result.success && result.data) {
      return result.data.data;
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
    return await this.adapter.delete(collection, id);
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
   */
  async batch(
    operations: StorageOperation[],
    context: DataOperationContext
  ): Promise<StorageResult<any[]>> {
    await this.ensureInitialized();
    return await this.adapter.batch(operations);
  }
  
  /**
   * Clear all data from all collections
   */
  async clear(): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();
    return await this.adapter.clear();
  }

  /**
   * Clear all data with detailed reporting (instance method)
   */
  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    await this.ensureInitialized();
    return await this.adapter.clearAll();
  }

  /**
   * Truncate all records from a specific collection
   */
  async truncate(collection: string): Promise<StorageResult<boolean>> {
    await this.ensureInitialized();
    return await this.adapter.truncate(collection);
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
   * Validate entity data - simple field-based validation
   */
  private validateSchema<T extends BaseEntity>(collection: string, data: T): SchemaValidationResult {
    const errors: string[] = [];

    // Basic validation - ensure essential fields exist
    if (!data.id || typeof data.id !== 'string' || data.id.trim() === '') {
      errors.push(`Field "id" is required and must be a non-empty string`);
    }

    // Collection-specific validation
    switch (collection) {
      case 'ChatMessage':
        this.validateChatMessage(data as any, errors);
        break;
      case 'User':
        this.validateUser(data as any, errors);
        break;
      case 'Room':
        this.validateRoom(data as any, errors);
        break;
    }

    if (errors.length > 0) {
      console.error(`❌ DataDaemon: Schema validation failed for "${collection}":`, errors);
      return { success: false, errors };
    }

    console.log(`✅ DataDaemon: Schema validation passed for "${collection}"`);
    return { success: true };
  }

  private validateChatMessage(data: any, errors: string[]): void {
    if (!data.roomId || typeof data.roomId !== 'string') {
      errors.push('ChatMessage.roomId is required and must be a string');
    }
    if (!data.senderId || typeof data.senderId !== 'string') {
      errors.push('ChatMessage.senderId is required and must be a string');
    }
    if (!data.senderName || typeof data.senderName !== 'string') {
      errors.push('ChatMessage.senderName is required and must be a string');
    }
    if (!data.content || typeof data.content !== 'object') {
      errors.push('ChatMessage.content is required and must be an object');
    }
    if (!data.status || typeof data.status !== 'string') {
      errors.push('ChatMessage.status is required and must be a string');
    }
    if (!this.isValidDate(data.timestamp)) {
      errors.push('ChatMessage.timestamp is required and must be a valid Date or ISO date string');
    }
  }

  private validateUser(data: any, errors: string[]): void {
    // Required scalar fields
    if (!data.displayName || typeof data.displayName !== 'string') {
      errors.push('User.displayName is required and must be a string');
    }
    if (!data.shortDescription || typeof data.shortDescription !== 'string') {
      errors.push('User.shortDescription is required and must be a string');
    }
    if (!data.type || !['human', 'agent', 'persona', 'system'].includes(data.type)) {
      errors.push('User.type is required and must be one of: human, agent, persona, system');
    }
    if (!data.status || !['online', 'offline', 'away', 'busy'].includes(data.status)) {
      errors.push('User.status is required and must be one of: online, offline, away, busy');
    }

    // Date field validation
    if (!this.isValidDate(data.lastActiveAt)) {
      errors.push('User.lastActiveAt is required and must be a valid Date or ISO date string');
    }

    // JSON field validation
    if (!data.profile || typeof data.profile !== 'object') {
      errors.push('User.profile is required and must be an object');
    } else {
      if (!data.profile.displayName || typeof data.profile.displayName !== 'string') {
        errors.push('User.profile.displayName is required and must be a string');
      }
      if (!data.profile.avatar || typeof data.profile.avatar !== 'string') {
        errors.push('User.profile.avatar is required and must be a string');
      }
      if (!data.profile.bio || typeof data.profile.bio !== 'string') {
        errors.push('User.profile.bio is required and must be a string');
      }
      if (!data.profile.location || typeof data.profile.location !== 'string') {
        errors.push('User.profile.location is required and must be a string');
      }
      if (!this.isValidDate(data.profile.joinedAt)) {
        errors.push('User.profile.joinedAt is required and must be a valid Date or ISO date string');
      }
    }

    if (!data.capabilities || typeof data.capabilities !== 'object') {
      errors.push('User.capabilities is required and must be an object');
    }

    if (!Array.isArray(data.sessionsActive)) {
      errors.push('User.sessionsActive is required and must be an array');
    }
  }

  /**
   * Flexible date validation - accepts Date objects or valid ISO date strings
   */
  private isValidDate(value: any): boolean {
    if (!value) return false;

    // Accept Date objects
    if (value instanceof Date) {
      return !isNaN(value.getTime());
    }

    // Accept ISO date strings
    if (typeof value === 'string') {
      const dateObj = new Date(value);
      return !isNaN(dateObj.getTime());
    }

    return false;
  }

  private validateRoom(data: any, errors: string[]): void {
    // Required scalar fields
    if (!data.name || typeof data.name !== 'string') {
      errors.push('Room.name is required and must be a string');
    }
    if (!data.displayName || typeof data.displayName !== 'string') {
      errors.push('Room.displayName is required and must be a string');
    }
    if (!data.type || !['public', 'private', 'direct'].includes(data.type)) {
      errors.push('Room.type is required and must be one of: public, private, direct');
    }
    if (!data.status || !['active', 'archived', 'deleted'].includes(data.status)) {
      errors.push('Room.status is required and must be one of: active, archived, deleted');
    }
    if (!data.ownerId || typeof data.ownerId !== 'string') {
      errors.push('Room.ownerId is required and must be a string');
    }

    // Optional date field validation
    if (data.lastMessageAt !== undefined && !this.isValidDate(data.lastMessageAt)) {
      errors.push('Room.lastMessageAt must be a valid Date or ISO date string when provided');
    }

    // JSON field validation
    if (!data.privacy || typeof data.privacy !== 'object') {
      errors.push('Room.privacy is required and must be an object');
    } else {
      if (typeof data.privacy.isPublic !== 'boolean') {
        errors.push('Room.privacy.isPublic is required and must be a boolean');
      }
      if (typeof data.privacy.requiresInvite !== 'boolean') {
        errors.push('Room.privacy.requiresInvite is required and must be a boolean');
      }
      if (typeof data.privacy.allowGuestAccess !== 'boolean') {
        errors.push('Room.privacy.allowGuestAccess is required and must be a boolean');
      }
      if (typeof data.privacy.searchable !== 'boolean') {
        errors.push('Room.privacy.searchable is required and must be a boolean');
      }
    }

    if (!data.settings || typeof data.settings !== 'object') {
      errors.push('Room.settings is required and must be an object');
    }

    if (!Array.isArray(data.members)) {
      errors.push('Room.members is required and must be an array');
    } else {
      data.members.forEach((member: any, index: number) => {
        if (member.joinedAt !== undefined && !this.isValidDate(member.joinedAt)) {
          errors.push(`Room.members[${index}].joinedAt must be a valid Date or ISO date string when provided`);
        }
        if (member.lastReadAt !== undefined && !this.isValidDate(member.lastReadAt)) {
          errors.push(`Room.members[${index}].lastReadAt must be a valid Date or ISO date string when provided`);
        }
      });
    }

    if (!Array.isArray(data.tags)) {
      errors.push('Room.tags is required and must be an array');
    }
  }

  // =============================================
  // CLEAN DOMAIN-OWNED STATIC INTERFACE
  // =============================================

  private static sharedInstance: DataDaemon | undefined;
  private static context: DataOperationContext | undefined;

  /**
   * Initialize static DataDaemon context (called by system)
   */
  static initialize(instance: DataDaemon, context: DataOperationContext): void {
    DataDaemon.sharedInstance = instance;
    DataDaemon.context = context;
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
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }
    return await DataDaemon.sharedInstance.create<T>(collection, data, DataDaemon.context);
  }

  /**
   * Query data with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const users = await DataDaemon.query<UserData>({ collection: 'users', filters: { active: true } });
   * const messages = await DataDaemon.query<ChatMessageData>({
   *   collection: 'messages',
   *   filters: { roomId: 'general' },
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
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.update<T>(collection, id, data, DataDaemon.context, incrementVersion);
  }

  /**
   * Remove data with automatic context injection - CLEAN INTERFACE
   *
   * @example
   * const success = await DataDaemon.remove('users', userId);
   */
  static async remove(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    if (!DataDaemon.sharedInstance || !DataDaemon.context) {
      throw new Error('DataDaemon not initialized - system must call DataDaemon.initialize() first');
    }

    return await DataDaemon.sharedInstance.delete(collection, id, DataDaemon.context);
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