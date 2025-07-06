/**
 * Database Integration - Unified database delegation for all daemons
 * 
 * Consolidates database patterns between ChatRoomDaemon, AcademyDaemon, and others
 * Provides consistent interface to DatabaseDaemon for all subsystems
 */

import { DaemonMessage } from './DaemonProtocol';

export interface DatabaseOperation {
  operation: string;
  collection: string;
  key?: string;
  data?: Record<string, unknown>;
  query?: Record<string, unknown>;
  options?: DatabaseOptions;
}

export interface DatabaseOptions {
  limit?: number;
  offset?: number;
  sort?: Record<string, 'asc' | 'desc'>;
  metadata?: Record<string, unknown>;
  ttl?: number; // Time to live in seconds
}

export interface DatabaseResponse {
  success: boolean;
  data?: unknown;
  error?: string;
  metadata?: {
    operation_id: string;
    timestamp: Date;
    file_path?: string;
    collection: string;
  };
}

/**
 * Base Database Integration Mixin
 * 
 * Provides consistent database delegation patterns for all daemons
 * Use this instead of implementing database logic directly
 */
interface DatabaseDaemon {
  handleMessage(message: DaemonMessage): Promise<DatabaseResponse>;
}

export class DatabaseIntegration {
  private databaseDaemon: DatabaseDaemon;
  private collectionPrefix: string;

  constructor(databaseDaemon: DatabaseDaemon, collectionPrefix: string) {
    this.databaseDaemon = databaseDaemon;
    this.collectionPrefix = collectionPrefix;
  }

  /**
   * Universal database delegation method
   * Consolidates all database operations through DatabaseDaemon
   */
  async delegateToDatabaseDaemon(operation: DatabaseOperation): Promise<DatabaseResponse> {
    const fullCollectionName = `${this.collectionPrefix}_${operation.collection}`;
    
    try {
      const message = this.buildDatabaseMessage(operation, fullCollectionName);
      const result = await this.databaseDaemon.handleMessage(message);

      if (!result.success) {
        throw new Error(result.error || 'Database operation failed');
      }

      return {
        success: true,
        data: result.data,
        metadata: {
          operation_id: `${operation.operation}_${Date.now()}`,
          timestamp: new Date(),
          file_path: result.data?.file_path,
          collection: fullCollectionName
        }
      };

    } catch (error) {
      console.error(`❌ Database delegation failed for ${operation.operation}:`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Save data to collection
   */
  async save(collection: string, key: string, data: Record<string, unknown>, metadata?: Record<string, unknown>): Promise<DatabaseResponse> {
    const operation: DatabaseOperation = {
      operation: 'save',
      collection,
      key,
      data
    };
    
    if (metadata) {
      operation.options = { metadata };
    }
    
    return this.delegateToDatabaseDaemon(operation);
  }

  /**
   * Query data from collection
   */
  async query(collection: string, query: Record<string, unknown>, options?: DatabaseOptions): Promise<DatabaseResponse> {
    const operation: DatabaseOperation = {
      operation: 'query',
      collection,
      query
    };
    
    if (options) {
      operation.options = options;
    }
    
    return this.delegateToDatabaseDaemon(operation);
  }

  /**
   * Load single item by key
   */
  async load(collection: string, key: string): Promise<DatabaseResponse> {
    return this.delegateToDatabaseDaemon({
      operation: 'load',
      collection,
      key
    });
  }

  /**
   * Delete item by key
   */
  async delete(collection: string, key: string): Promise<DatabaseResponse> {
    return this.delegateToDatabaseDaemon({
      operation: 'delete',
      collection,
      key
    });
  }

  /**
   * List all items in collection
   */
  async list(collection: string, options?: DatabaseOptions): Promise<DatabaseResponse> {
    const operation: DatabaseOperation = {
      operation: 'list',
      collection
    };
    
    if (options) {
      operation.options = options;
    }
    
    return this.delegateToDatabaseDaemon(operation);
  }

  /**
   * Backup collection
   */
  async backup(collection: string, backupName?: string): Promise<DatabaseResponse> {
    return this.delegateToDatabaseDaemon({
      operation: 'backup',
      collection,
      options: { metadata: { backup_name: backupName } }
    });
  }

  // Private helper methods

  private buildDatabaseMessage(operation: DatabaseOperation, fullCollectionName: string): DaemonMessage {
    switch (operation.operation) {
      case 'save':
        return {
          type: 'save_data',
          data: {
            collection: fullCollectionName,
            key: operation.key,
            data: operation.data,
            metadata: operation.options?.metadata
          }
        };

      case 'query':
        return {
          type: 'query_data',
          data: {
            collection: fullCollectionName,
            filters: operation.query,
            limit: operation.options?.limit,
            offset: operation.options?.offset,
            sort: operation.options?.sort
          }
        };

      case 'load':
        return {
          type: 'load_data',
          data: {
            collection: fullCollectionName,
            key: operation.key
          }
        };

      case 'delete':
        return {
          type: 'delete_data',
          data: {
            collection: fullCollectionName,
            key: operation.key
          }
        };

      case 'list':
        return {
          type: 'list_data',
          data: {
            collection: fullCollectionName,
            limit: operation.options?.limit,
            offset: operation.options?.offset
          }
        };

      case 'backup':
        return {
          type: 'backup_collection',
          data: {
            collection: fullCollectionName,
            backup_name: operation.options?.metadata?.backup_name
          }
        };

      default:
        throw new Error(`Unknown database operation: ${operation.operation}`);
    }
  }
}

/**
 * Chat-specific database integration
 * Consolidates ChatRoomDaemon database patterns
 */
export class ChatDatabaseIntegration extends DatabaseIntegration {
  constructor(databaseDaemon: DatabaseDaemon) {
    super(databaseDaemon, 'chat');
  }

  /**
   * Save chat room
   */
  async saveRoom(room: {
    id: string;
    name: string;
    participants?: { size: number };
    created_at: Date;
  }): Promise<DatabaseResponse> {
    return this.save('rooms', room.id, room, {
      type: 'chat_room',
      name: room.name,
      participant_count: room.participants?.size || 0,
      created_at: room.created_at
    });
  }

  /**
   * Save chat message
   */
  async saveMessage(message: {
    id: string;
    room_id: string;
    sender_id: string;
    timestamp: Date;
  }): Promise<DatabaseResponse> {
    return this.save('messages', message.id, message, {
      type: 'chat_message',
      room_id: message.room_id,
      sender_id: message.sender_id,
      timestamp: message.timestamp
    });
  }

  /**
   * Load room history
   */
  async loadRoomHistory(roomId: string, limit: number = 50, offset: number = 0): Promise<DatabaseResponse> {
    return this.query('messages', 
      { room_id: roomId }, 
      { limit, offset, sort: { timestamp: 'desc' } }
    );
  }

  /**
   * Load all rooms for user
   */
  async loadUserRooms(userId: string): Promise<DatabaseResponse> {
    return this.query('rooms', 
      { participants: { contains: userId } }
    );
  }

  /**
   * Load all rooms
   */
  async loadAllRooms(): Promise<DatabaseResponse> {
    return this.list('rooms');
  }
}

/**
 * Academy-specific database integration
 * Consolidates AcademyDaemon database patterns
 */
export class AcademyDatabaseIntegration extends DatabaseIntegration {
  constructor(databaseDaemon: DatabaseDaemon) {
    super(databaseDaemon, 'academy');
  }

  /**
   * Save optimization record
   */
  async saveOptimizationRecord(record: {
    id: string;
    timestamp: Date;
    impact_metrics?: {
      compression_gained?: number;
      performance_change?: number;
    };
    optimization_steps?: unknown[];
  }): Promise<DatabaseResponse> {
    return this.save('optimization_records', record.id, record, {
      type: 'optimization_record',
      timestamp: record.timestamp,
      compression_gained: record.impact_metrics?.compression_gained,
      performance_change: record.impact_metrics?.performance_change,
      steps_count: record.optimization_steps?.length
    });
  }

  /**
   * Save persona genome
   */
  async savePersonaGenome(personaId: string, genome: {
    knowledge?: {
      domain_expertise?: Array<{ domain: string }>;
    };
  }, identity: {
    uuid: string;
    name: string;
    creator_node: string;
    derivation_type: string;
  }): Promise<DatabaseResponse> {
    return this.save('persona_genomes', personaId, { genome, identity }, {
      type: 'persona_genome',
      uuid: identity.uuid,
      name: identity.name,
      creator_node: identity.creator_node,
      domains: genome.knowledge?.domain_expertise?.map(d => d.domain) || [],
      derivation_type: identity.derivation_type
    });
  }

  /**
   * Save LoRA composition
   */
  async saveLoRAComposition(compositionId: string, composition: {
    primary_layers?: unknown[];
    total_rank: number;
  }, layerIdentities: Array<{
    domain: string;
    uuid: string;
  }>): Promise<DatabaseResponse> {
    return this.save('lora_compositions', compositionId, { composition, layer_identities: layerIdentities }, {
      type: 'lora_composition',
      primary_layers: composition.primary_layers?.length || 0,
      total_rank: composition.total_rank,
      domains: [...new Set(layerIdentities.map(l => l.domain))],
      layer_uuids: layerIdentities.map(l => l.uuid)
    });
  }

  /**
   * Save LoRA layer identity
   */
  async saveLoRALayer(layer: {
    uuid: string;
    domain: string;
    target_capability: string;
    rank: number;
    creator_node: string;
    creation_prompt: string;
  }): Promise<DatabaseResponse> {
    return this.save('lora_layers', layer.uuid, layer, {
      type: 'lora_layer',
      domain: layer.domain,
      target_capability: layer.target_capability,
      rank: layer.rank,
      creator_node: layer.creator_node,
      creation_prompt: layer.creation_prompt
    });
  }

  /**
   * Save training resource
   */
  async saveTrainingResource(resource: {
    uuid: string;
    resource_type: string;
    domain_tags: string[];
    quality_score: number;
    size_bytes: number;
  }): Promise<DatabaseResponse> {
    return this.save('training_resources', resource.uuid, resource, {
      type: 'training_resource',
      resource_type: resource.resource_type,
      domain_tags: resource.domain_tags,
      quality_score: resource.quality_score,
      size_bytes: resource.size_bytes
    });
  }

  /**
   * Save prompt-layer binding
   */
  async savePromptBinding(binding: {
    layer_uuid: string;
    target_behavior: string;
    prompt_metadata?: {
      effectiveness_score?: number;
      domain_specificity?: number;
    };
  }): Promise<DatabaseResponse> {
    return this.save('prompt_bindings', binding.layer_uuid, binding, {
      type: 'prompt_binding',
      target_behavior: binding.target_behavior,
      effectiveness_score: binding.prompt_metadata?.effectiveness_score,
      domain_specificity: binding.prompt_metadata?.domain_specificity
    });
  }

  /**
   * Query personas by domain
   */
  async queryPersonasByDomain(domains: string[]): Promise<DatabaseResponse> {
    return this.query('persona_genomes', {
      domains: { any: domains }
    });
  }

  /**
   * Query LoRA layers by capability
   */
  async queryLayersByCapability(capability: string): Promise<DatabaseResponse> {
    return this.query('lora_layers', {
      target_capability: { contains: capability }
    });
  }

  /**
   * Query optimization history
   */
  async queryOptimizationHistory(dateRange?: { start: Date; end: Date }): Promise<DatabaseResponse> {
    const query: Record<string, unknown> = {};
    
    if (dateRange) {
      query.timestamp = {
        gte: dateRange.start,
        lte: dateRange.end
      };
    }

    return this.query('optimization_records', query, {
      sort: { timestamp: 'desc' },
      limit: 100
    });
  }

  /**
   * Backup all Academy data
   */
  async backupAllAcademyData(): Promise<DatabaseResponse[]> {
    const collections = [
      'optimization_records',
      'persona_genomes', 
      'lora_compositions',
      'lora_layers',
      'training_resources',
      'prompt_bindings'
    ];

    const backupResults: DatabaseResponse[] = [];
    const timestamp = new Date().toISOString().split('T')[0]; // YYYY-MM-DD

    for (const collection of collections) {
      const result = await this.backup(collection, `${collection}_${timestamp}`);
      backupResults.push(result);
    }

    return backupResults;
  }
}

/**
 * Factory for creating database integrations
 */
export class DatabaseIntegrationFactory {
  static createChatIntegration(databaseDaemon: DatabaseDaemon): ChatDatabaseIntegration {
    return new ChatDatabaseIntegration(databaseDaemon);
  }

  static createAcademyIntegration(databaseDaemon: DatabaseDaemon): AcademyDatabaseIntegration {
    return new AcademyDatabaseIntegration(databaseDaemon);
  }

  static createCustomIntegration(databaseDaemon: DatabaseDaemon, prefix: string): DatabaseIntegration {
    return new DatabaseIntegration(databaseDaemon, prefix);
  }
}