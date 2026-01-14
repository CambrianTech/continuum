/**
 * PersonaTimeline - Global timeline for unified consciousness
 *
 * A unified chronological view of ALL activities across all contexts.
 * This is the foundation for cross-context awareness - preventing "severance"
 * where each room is an isolated cognitive island.
 *
 * Storage: Per-persona SQLite database via ORM (TimelineEventEntity)
 * Uses same longterm.db as Hippocampus for consistency.
 *
 * The timeline enables:
 * - "What was I doing before this?" queries
 * - Cross-context semantic search
 * - Peripheral awareness of other rooms
 * - Temporal continuity of self
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../../../core/types/CrossPlatformUUID';
import type { ContextType, TimelineEventType } from '../../../../data/entities/TimelineEventEntity';
import { TimelineEventEntity } from '../../../../data/entities/TimelineEventEntity';
import { RustEmbeddingClient } from '../../../../core/services/RustEmbeddingClient';
import { SimilarityMetrics } from '../../../../../daemons/data-daemon/shared/VectorSearchTypes';
import { SystemPaths } from '../../../../core/config/SystemPaths';
import { Commands } from '../../../../core/shared/Commands';
import { truncate } from '../../../../../shared/utils/StringUtils';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataOpenParams, DataOpenResult } from '../../../../../commands/data/open/shared/DataOpenTypes';
import type { DataListParams, DataListResult } from '../../../../../commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '../../../../../commands/data/create/shared/DataCreateTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../../../commands/data/update/shared/DataUpdateTypes';
import * as fsPromises from 'fs/promises';
import * as path from 'path';

/**
 * Timeline event - internal interface matching TimelineEventEntity
 */
export interface TimelineEvent {
  id: UUID;
  personaId: UUID;
  timestamp: string; // ISO string for serialization
  contextType: ContextType;
  contextId: UUID;
  contextName: string;
  eventType: TimelineEventType;
  actorId: UUID;
  actorName: string;
  content: string;
  importance: number;
  topics: string[];
  relatedEventIds?: UUID[];
  triggeredById?: UUID;
  embedding?: number[];
  metadata?: Record<string, unknown>;
}

/**
 * Parameters for recording a new timeline event
 */
export interface RecordEventParams {
  contextType: ContextType;
  contextId: UUID;
  contextName: string;
  eventType: TimelineEventType;
  actorId: UUID;
  actorName: string;
  content: string;
  importance?: number;
  topics?: string[];
  relatedEventIds?: UUID[];
  triggeredById?: UUID;
  metadata?: Record<string, unknown>;
}

/**
 * Cross-context memory with source context info
 */
export interface ContextualEvent {
  event: TimelineEvent;
  sourceContextName: string;
  relevanceReason?: string;
}

/**
 * Temporal thread - what was I doing before this context?
 */
export interface TemporalThread {
  beforeThis: TimelineEvent[];    // Recent activity in OTHER contexts
  lastTimeHere?: TimelineEvent;   // Last time I was in THIS context
  activeWorkSummary: string;      // Human-readable summary
}

/**
 * Logger interface for consciousness logging
 */
export interface ConsciousnessLogger {
  debug: (message: string) => void;
  info: (message: string) => void;
  warn: (message: string) => void;
  error: (message: string) => void;
}

/**
 * PersonaTimeline - Manages the global timeline for a persona
 *
 * Stores timeline in per-persona SQLite database using ORM (TimelineEventEntity)
 * Path: .continuum/personas/{uniqueId}/data/longterm.db (same as Hippocampus)
 */
export class PersonaTimeline {
  private readonly personaId: UUID;
  private readonly uniqueId: string;
  private readonly personaName: string;
  private readonly log: ConsciousnessLogger;
  private readonly dbPath: string;
  private readonly legacyJsonPath: string;

  // Database handle for ORM operations
  private dbHandle: string | null = null;
  private initialized = false;
  private initializationPromise: Promise<void> | null = null;

  // Context name cache (contextId -> name)
  private readonly contextNameCache: Map<UUID, string> = new Map();

  // Embedding generation queue - process asynchronously to avoid blocking
  private readonly embeddingQueue: Array<{ eventId: UUID; content: string }> = [];
  private isProcessingEmbeddings = false;
  private readonly EMBEDDING_BATCH_SIZE = 10;
  private readonly EMBEDDING_PROCESS_DELAY_MS = 100;

  // Limit events to prevent unbounded growth
  private readonly MAX_EVENTS = 10000;

  constructor(
    personaId: UUID,
    uniqueId: string,  // e.g., "together", "helper" - matches folder name
    personaName: string,
    logger?: ConsciousnessLogger
  ) {
    this.personaId = personaId;
    this.uniqueId = uniqueId;
    this.personaName = personaName;
    this.log = logger || {
      debug: (msg) => console.debug(`[Timeline:${personaName}] ${msg}`),
      info: (msg) => console.log(`[Timeline:${personaName}] ${msg}`),
      warn: (msg) => console.warn(`[Timeline:${personaName}] ${msg}`),
      error: (msg) => console.error(`[Timeline:${personaName}] ${msg}`)
    };

    // Use SystemPaths as SINGLE SOURCE OF TRUTH (same database as Hippocampus)
    this.dbPath = SystemPaths.personas.longterm(uniqueId);
    this.legacyJsonPath = path.join(SystemPaths.personas.data(uniqueId), 'timeline.json');
  }

  /**
   * Initialize database connection and migrate legacy data if needed
   */
  private async ensureInitialized(): Promise<void> {
    if (this.initialized) return;

    // Use promise to prevent concurrent initialization
    if (this.initializationPromise) {
      return this.initializationPromise;
    }

    this.initializationPromise = this.initialize();
    await this.initializationPromise;
  }

  private async initialize(): Promise<void> {
    try {
      this.log.debug(`Opening timeline database: ${this.dbPath}`);

      const result = await Commands.execute<DataOpenParams, DataOpenResult>(DATA_COMMANDS.OPEN, {
        adapter: 'sqlite',
        config: {
          path: this.dbPath,
          mode: 'readwrite',
          wal: true,           // Write-Ahead Logging (fast writes)
          foreignKeys: true    // Referential integrity
        }
      });

      if (!result.success || !result.dbHandle) {
        throw new Error(result.error || 'Failed to open timeline database');
      }

      this.dbHandle = result.dbHandle;
      this.log.debug(`Timeline database opened: ${this.dbHandle}`);

      // Migrate legacy JSON data if it exists
      await this.migrateLegacyData();

      this.initialized = true;
    } catch (error) {
      this.log.error(`Failed to initialize timeline database: ${error}`);
      throw error;
    }
  }

  /**
   * Migrate legacy timeline.json data to SQLite
   */
  private async migrateLegacyData(): Promise<void> {
    try {
      // Check if legacy JSON file exists
      const data = await fsPromises.readFile(this.legacyJsonPath, 'utf-8');
      const legacyEvents: TimelineEvent[] = JSON.parse(data);

      if (legacyEvents.length === 0) {
        this.log.debug('No legacy timeline data to migrate');
        return;
      }

      this.log.info(`Migrating ${legacyEvents.length} legacy timeline events to SQLite...`);

      // Check if we already have events in the database
      const existingResult = await Commands.execute<DataListParams, DataListResult<TimelineEventEntity>>(DATA_COMMANDS.LIST, {
        dbHandle: this.dbHandle!,
        collection: TimelineEventEntity.collection,
        filter: { personaId: this.personaId },
        limit: 1
      });

      if (existingResult.success && existingResult.items && existingResult.items.length > 0) {
        this.log.debug('Timeline events already exist in database, skipping migration');
        // Rename old file to .migrated
        await fsPromises.rename(this.legacyJsonPath, this.legacyJsonPath + '.migrated');
        return;
      }

      // Migrate each event
      let migrated = 0;
      let failed = 0;
      for (const event of legacyEvents) {
        try {
          const entity = this.eventToEntity(event);
          await Commands.execute<DataCreateParams, DataCreateResult<TimelineEventEntity>>(DATA_COMMANDS.CREATE, {
            dbHandle: this.dbHandle!,
            collection: TimelineEventEntity.collection,
            data: entity
          } as any);
          migrated++;
        } catch (err) {
          failed++;
          this.log.warn(`Failed to migrate event ${event.id}: ${err}`);
        }
      }

      this.log.info(`✅ Migrated ${migrated} timeline events (${failed} failed)`);

      // Rename old file to .migrated
      await fsPromises.rename(this.legacyJsonPath, this.legacyJsonPath + '.migrated');
    } catch (err: any) {
      if (err.code === 'ENOENT') {
        this.log.debug('No legacy timeline.json found');
      } else {
        this.log.warn(`Legacy migration check failed: ${err}`);
      }
    }
  }

  /**
   * Convert TimelineEvent interface to TimelineEventEntity
   */
  private eventToEntity(event: TimelineEvent): Partial<TimelineEventEntity> {
    return {
      id: event.id,
      personaId: event.personaId,
      timestamp: new Date(event.timestamp),
      contextType: event.contextType,
      contextId: event.contextId,
      contextName: event.contextName,
      eventType: event.eventType,
      actorId: event.actorId,
      actorName: event.actorName,
      content: event.content,
      importance: event.importance,
      topics: event.topics,
      relatedEventIds: event.relatedEventIds,
      triggeredById: event.triggeredById,
      embedding: event.embedding,
      metadata: event.metadata
    };
  }

  /**
   * Convert TimelineEventEntity to TimelineEvent interface
   */
  private entityToEvent(entity: TimelineEventEntity): TimelineEvent {
    return {
      id: entity.id,
      personaId: entity.personaId,
      timestamp: entity.timestamp instanceof Date ? entity.timestamp.toISOString() : String(entity.timestamp),
      contextType: entity.contextType,
      contextId: entity.contextId,
      contextName: entity.contextName,
      eventType: entity.eventType,
      actorId: entity.actorId,
      actorName: entity.actorName,
      content: entity.content,
      importance: entity.importance,
      topics: entity.topics,
      relatedEventIds: entity.relatedEventIds,
      triggeredById: entity.triggeredById,
      embedding: entity.embedding,
      metadata: entity.metadata as Record<string, unknown> | undefined
    };
  }

  /**
   * Record a new event in the timeline
   * This is the primary write method - called whenever something happens
   */
  async recordEvent(params: RecordEventParams): Promise<TimelineEvent> {
    await this.ensureInitialized();

    const event: TimelineEvent = {
      id: generateUUID(),
      personaId: this.personaId,
      timestamp: new Date().toISOString(),
      contextType: params.contextType,
      contextId: params.contextId,
      contextName: params.contextName,
      eventType: params.eventType,
      actorId: params.actorId,
      actorName: params.actorName,
      content: params.content,
      importance: params.importance ?? 0.5,
      topics: params.topics ?? [],
      relatedEventIds: params.relatedEventIds,
      triggeredById: params.triggeredById,
      metadata: params.metadata
    };

    // Validate
    if (!event.personaId || !event.contextId || !event.content) {
      throw new Error('Invalid timeline event: missing required fields');
    }

    // Store in database
    const entity = this.eventToEntity(event);
    const result = await Commands.execute<DataCreateParams, DataCreateResult<TimelineEventEntity>>(DATA_COMMANDS.CREATE, {
      dbHandle: this.dbHandle!,
      collection: TimelineEventEntity.collection,
      data: entity
    } as any);

    if (!result.success) {
      throw new Error(`Failed to store timeline event: ${result.error}`);
    }

    // Update context cache
    this.contextNameCache.set(params.contextId, params.contextName);

    // Queue async embedding generation (non-blocking)
    this.queueEmbeddingGeneration(event.id, params.content);

    this.log.debug(`Recorded: ${params.eventType} in ${params.contextName}`);
    return event;
  }

  /**
   * Get recent events across ALL contexts (global view)
   */
  async getRecent(limit: number = 50): Promise<TimelineEvent[]> {
    await this.ensureInitialized();

    const result = await Commands.execute<DataListParams, DataListResult<TimelineEventEntity>>(DATA_COMMANDS.LIST, {
      dbHandle: this.dbHandle!,
      collection: TimelineEventEntity.collection,
      filter: { personaId: this.personaId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit
    });

    if (!result.success || !result.items) {
      return [];
    }

    return result.items.map((entity: TimelineEventEntity) => this.entityToEvent(entity));
  }

  /**
   * Get recent events for a specific context
   */
  async getForContext(contextId: UUID, limit: number = 50): Promise<TimelineEvent[]> {
    await this.ensureInitialized();

    const result = await Commands.execute<DataListParams, DataListResult<TimelineEventEntity>>(DATA_COMMANDS.LIST, {
      dbHandle: this.dbHandle!,
      collection: TimelineEventEntity.collection,
      filter: { personaId: this.personaId, contextId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit
    });

    if (!result.success || !result.items) {
      return [];
    }

    return result.items.map((entity: TimelineEventEntity) => this.entityToEvent(entity));
  }

  /**
   * Get events from OTHER contexts (cross-context awareness)
   * This is the key method for preventing "severance"
   */
  async getCrossContext(
    currentContextId: UUID,
    options?: {
      limit?: number;
      minImportance?: number;
      eventTypes?: TimelineEventType[];
      since?: Date;
    }
  ): Promise<ContextualEvent[]> {
    await this.ensureInitialized();

    // Build filter
    const filter: Record<string, any> = {
      personaId: this.personaId,
      contextId: { $ne: currentContextId }  // Exclude current context
    };

    if (options?.minImportance !== undefined) {
      filter.importance = { $gte: options.minImportance };
    }

    if (options?.eventTypes && options.eventTypes.length > 0) {
      filter.eventType = { $in: options.eventTypes };
    }

    if (options?.since) {
      filter.timestamp = { $gte: options.since.toISOString() };
    }

    const result = await Commands.execute<DataListParams, DataListResult<TimelineEventEntity>>(DATA_COMMANDS.LIST, {
      dbHandle: this.dbHandle!,
      collection: TimelineEventEntity.collection,
      filter,
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: options?.limit || 20
    });

    if (!result.success || !result.items) {
      return [];
    }

    return result.items.map((entity: TimelineEventEntity) => {
      const event = this.entityToEvent(entity);
      return {
        event,
        sourceContextName: event.contextName,
        relevanceReason: `From ${event.contextName}`
      };
    });
  }

  /**
   * Get my activity in other contexts (what was I doing before?)
   * Returns only events where I (the persona) was the actor
   */
  async getMyRecentActivity(
    excludeContextId: UUID,
    limit: number = 10
  ): Promise<TimelineEvent[]> {
    await this.ensureInitialized();

    const result = await Commands.execute<DataListParams, DataListResult<TimelineEventEntity>>(DATA_COMMANDS.LIST, {
      dbHandle: this.dbHandle!,
      collection: TimelineEventEntity.collection,
      filter: {
        personaId: this.personaId,
        actorId: this.personaId,
        contextId: { $ne: excludeContextId }
      },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit
    });

    if (!result.success || !result.items) {
      return [];
    }

    return result.items.map((entity: TimelineEventEntity) => this.entityToEvent(entity));
  }

  /**
   * Build temporal thread - what was I doing before this context?
   * This provides temporal continuity across context switches
   */
  async getTemporalThread(currentContextId: UUID): Promise<TemporalThread> {
    // Get my recent activity in OTHER contexts
    const beforeThis = await this.getMyRecentActivity(currentContextId, 5);

    // Get last time I was in THIS context
    const lastHereEvents = await this.getForContext(currentContextId, 1);
    const lastTimeHere = lastHereEvents.length > 0 ? lastHereEvents[0] : undefined;

    // Build summary
    const summary = this.buildThreadSummary(beforeThis, lastTimeHere);

    return {
      beforeThis,
      lastTimeHere,
      activeWorkSummary: summary
    };
  }

  /**
   * Get peripheral summary - what's happening in other contexts?
   */
  async getPeripheralSummary(currentContextId: UUID): Promise<string> {
    // Get recent events from other contexts
    const otherContextEvents = await this.getCrossContext(currentContextId, {
      limit: 30,
      since: new Date(Date.now() - 30 * 60 * 1000) // Last 30 minutes
    });

    if (otherContextEvents.length === 0) {
      return 'Other contexts: Quiet';
    }

    // Group by context
    const byContext = new Map<string, ContextualEvent[]>();
    for (const ce of otherContextEvents) {
      const ctxName = ce.sourceContextName;
      if (!byContext.has(ctxName)) {
        byContext.set(ctxName, []);
      }
      byContext.get(ctxName)!.push(ce);
    }

    // Build summary for each context
    const summaries: string[] = [];
    for (const [contextName, events] of byContext) {
      const mostRecent = events[0];
      const timeAgo = this.formatTimeAgo(new Date(mostRecent.event.timestamp));
      const count = events.length;
      summaries.push(`${contextName}: ${count} event${count > 1 ? 's' : ''}, last ${timeAgo}`);
    }

    return summaries.join('\n');
  }

  /**
   * Get context name from cache
   */
  async getContextName(contextId: UUID): Promise<string> {
    if (this.contextNameCache.has(contextId)) {
      return this.contextNameCache.get(contextId)!;
    }

    await this.ensureInitialized();

    // Look up from database
    const result = await Commands.execute<DataListParams, DataListResult<TimelineEventEntity>>(DATA_COMMANDS.LIST, {
      dbHandle: this.dbHandle!,
      collection: TimelineEventEntity.collection,
      filter: { personaId: this.personaId, contextId },
      limit: 1
    });

    if (result.success && result.items && result.items.length > 0) {
      const entity = result.items[0];
      this.contextNameCache.set(contextId, entity.contextName);
      return entity.contextName;
    }

    return 'Unknown';
  }

  // === Private helpers ===

  private buildThreadSummary(
    beforeThis: TimelineEvent[],
    lastTimeHere?: TimelineEvent
  ): string {
    const parts: string[] = [];

    if (beforeThis.length > 0) {
      const recentContexts = [...new Set(beforeThis.map(e => e.contextName))];
      parts.push(`Before this: Active in ${recentContexts.join(', ')}`);

      const lastAction = beforeThis[0];
      parts.push(`Last action: ${truncate(lastAction.content, 100)}...`);
    }

    if (lastTimeHere) {
      const timeAgo = this.formatTimeAgo(new Date(lastTimeHere.timestamp));
      parts.push(`Last here: ${timeAgo}`);
    }

    return parts.length > 0 ? parts.join('\n') : 'No recent activity elsewhere';
  }

  private formatTimeAgo(date: Date): string {
    const ms = Date.now() - date.getTime();
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) return `${days} day${days > 1 ? 's' : ''} ago`;
    if (hours > 0) return `${hours} hour${hours > 1 ? 's' : ''} ago`;
    if (minutes > 0) return `${minutes} minute${minutes > 1 ? 's' : ''} ago`;
    return 'just now';
  }

  // === Async Embedding Generation ===

  /**
   * Queue an event for async embedding generation
   * Non-blocking - embedding happens in background
   */
  private queueEmbeddingGeneration(eventId: UUID, content: string): void {
    // Skip empty or very short content
    if (!content || content.length < 10) {
      return;
    }

    this.embeddingQueue.push({ eventId, content });

    // Start processing if not already running
    if (!this.isProcessingEmbeddings) {
      // Use setImmediate to process in next tick (non-blocking)
      setImmediate(() => this.processEmbeddingQueue());
    }
  }

  /**
   * Process embedding queue in batches
   * Uses RustEmbeddingClient for fast ONNX-based embeddings (~5ms each)
   */
  private async processEmbeddingQueue(): Promise<void> {
    if (this.isProcessingEmbeddings || this.embeddingQueue.length === 0) {
      return;
    }

    this.isProcessingEmbeddings = true;

    try {
      const embeddingClient = RustEmbeddingClient.instance;

      // Check if Rust embedding worker is available
      if (!await embeddingClient.isAvailable()) {
        this.log.debug('Rust embedding worker unavailable, skipping embedding generation');
        this.embeddingQueue.length = 0; // Clear queue
        return;
      }

      // Process in batches
      while (this.embeddingQueue.length > 0) {
        const batch = this.embeddingQueue.splice(0, this.EMBEDDING_BATCH_SIZE);
        const texts = batch.map(item => item.content);

        try {
          const startTime = Date.now();
          const embeddings = await embeddingClient.generate(texts);
          const duration = Date.now() - startTime;

          this.log.debug(`Generated ${embeddings.length} embeddings in ${duration}ms`);

          // Update events with embeddings
          for (let i = 0; i < batch.length; i++) {
            await this.updateEventEmbedding(batch[i].eventId, embeddings[i]);
          }
        } catch (error) {
          this.log.warn(`Embedding generation failed for batch: ${error}`);
          // Continue with next batch
        }

        // Small delay between batches to avoid overwhelming the system
        if (this.embeddingQueue.length > 0) {
          await new Promise(resolve => setTimeout(resolve, this.EMBEDDING_PROCESS_DELAY_MS));
        }
      }
    } finally {
      this.isProcessingEmbeddings = false;
    }
  }

  /**
   * Update an event with its embedding
   */
  private async updateEventEmbedding(eventId: UUID, embedding: number[]): Promise<void> {
    await this.ensureInitialized();

    try {
      await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        dbHandle: this.dbHandle!,
        collection: TimelineEventEntity.collection,
        id: eventId,
        data: { embedding }
      } as any);
    } catch (err) {
      this.log.warn(`Failed to update embedding for event ${eventId}: ${err}`);
    }
  }

  // === Semantic Cross-Context Search ===

  /**
   * Semantic search across all contexts
   * Finds events semantically similar to query, excluding current context
   *
   * This is the key method for "no severance" - finds relevant knowledge
   * from other rooms/activities based on meaning, not just keywords.
   */
  async semanticCrossContextSearch(
    query: string,
    currentContextId: UUID,
    options?: {
      limit?: number;
      minSimilarity?: number;
      since?: Date;
    }
  ): Promise<ContextualEvent[]> {
    const limit = options?.limit ?? 10;
    const minSimilarity = options?.minSimilarity ?? 0.5;

    console.log(`[SemanticSearch] Query: "${query.slice(0, 50)}..." currentContextId: ${currentContextId}`);

    try {
      await this.ensureInitialized();

      // Use vector search via Commands.execute
      const filter: Record<string, any> = {
        personaId: this.personaId,
        contextId: { $ne: currentContextId }
      };

      if (options?.since) {
        filter.timestamp = { $gte: options.since.toISOString() };
      }

      const result = await Commands.execute<any, any>('data/vector-search', {
        dbHandle: this.dbHandle!,
        collection: TimelineEventEntity.collection,
        queryText: query,
        k: limit,
        similarityThreshold: minSimilarity,
        filter
      });

      if (!result.success || !result.results) {
        console.log(`[SemanticSearch] ❌ Vector search failed: ${result.error}`);
        // Fallback to filter-based search
        return this.getCrossContext(currentContextId, {
          limit,
          since: options?.since
        });
      }

      const events = result.results.map((r: any) => {
        const entity = r.data as TimelineEventEntity;
        const event = this.entityToEvent(entity);
        const score = r.score || r.similarity || 0;
        return {
          event,
          sourceContextName: event.contextName,
          relevanceReason: `Semantically similar (${(score * 100).toFixed(0)}% match)`
        };
      });

      console.log(`[SemanticSearch] ✅ Found ${events.length} cross-context events above threshold ${minSimilarity}`);
      return events;

    } catch (error) {
      console.log(`[SemanticSearch] ❌ Error: ${error}`);
      this.log.warn(`Semantic search failed: ${error}`);
      return [];
    }
  }

  /**
   * Get semantically relevant events for RAG context
   * Combines cross-context and current context events by relevance
   */
  async getSemanticContext(
    currentContextId: UUID,
    currentMessage: string,
    options?: {
      crossContextLimit?: number;
      currentContextLimit?: number;
      minSimilarity?: number;
    }
  ): Promise<{
    crossContext: ContextualEvent[];
    currentContext: TimelineEvent[];
  }> {
    const crossContextLimit = options?.crossContextLimit ?? 5;
    const currentContextLimit = options?.currentContextLimit ?? 10;

    // Get semantically relevant events from other contexts
    const crossContext = await this.semanticCrossContextSearch(
      currentMessage,
      currentContextId,
      {
        limit: crossContextLimit,
        minSimilarity: options?.minSimilarity ?? 0.5,
        since: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000) // Last 7 days
      }
    );

    // Get recent events from current context
    const currentContext = await this.getForContext(currentContextId, currentContextLimit);

    return { crossContext, currentContext };
  }
}
