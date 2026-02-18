/**
 * EntityCacheService - Single source of truth for ALL entity data
 *
 * Positronic Pattern:
 * 1. Events update cache immediately (optimistic)
 * 2. Widgets subscribe to cache (pure views)
 * 3. DB queries fill cache on miss (lazy loading)
 * 4. Cache never discards event-added data until DB confirms
 *
 * This replaces scattered per-widget caching (ChatMessageCache, EntityScroller cache)
 * with a unified, centralized approach.
 */

import { Events } from '../core/shared/Events';
import type { BaseEntity } from '../data/entities/BaseEntity';

// Cache change notification type
export interface CacheChange<T extends BaseEntity> {
  type: 'created' | 'updated' | 'deleted' | 'populated';
  entity?: T;
  entityId?: string;
  collection: string;
}

// Subscriber callback type
type CacheSubscriber<T extends BaseEntity> = (
  entities: T[],
  change: CacheChange<T>
) => void;

// Entity subscriber callback type
type EntitySubscriber<T extends BaseEntity> = (entity: T | null) => void;

// Collection store with entities and subscribers
interface CollectionStore<T extends BaseEntity> {
  entities: Map<string, T>;
  subscribers: Set<CacheSubscriber<T>>;
  entitySubscribers: Map<string, Set<EntitySubscriber<T>>>;
}

/**
 * EntityCacheService Implementation
 *
 * Central cache for all entity data. Widgets subscribe to this cache
 * instead of loading data directly from the database.
 */
class EntityCacheServiceImpl {
  // Per-collection stores
  private collections: Map<string, CollectionStore<BaseEntity>> = new Map();

  // Track entities added via real-time events (not from DB)
  // These survive DB refresh since they're more recent than DB data
  private eventAddedIds: Map<string, Set<string>> = new Map();

  // Notification batching (microtask-based like ContentStateService)
  private pendingNotifications: Map<string, CacheChange<BaseEntity>[]> = new Map();
  private notifyScheduled = false;

  // ==================== CACHE ACCESS ====================

  /**
   * Get entity from cache (never hits DB)
   */
  get<T extends BaseEntity>(collection: string, id: string): T | undefined {
    const store = this.collections.get(collection) as CollectionStore<T> | undefined;
    return store?.entities.get(id);
  }

  /**
   * Get all cached entities for collection
   */
  getAll<T extends BaseEntity>(collection: string): T[] {
    const store = this.collections.get(collection) as CollectionStore<T> | undefined;
    if (!store) return [];
    return Array.from(store.entities.values());
  }

  /**
   * Get entities matching filter (cached only)
   */
  query<T extends BaseEntity>(collection: string, filter: (e: T) => boolean): T[] {
    return this.getAll<T>(collection).filter(filter);
  }

  /**
   * Check if entity exists in cache
   */
  has(collection: string, id: string): boolean {
    return this.collections.get(collection)?.entities.has(id) ?? false;
  }

  /**
   * Get count of cached entities
   */
  count(collection: string): number {
    return this.collections.get(collection)?.entities.size ?? 0;
  }

  /**
   * Check if entity was added via event (not from DB)
   */
  isEventAdded(collection: string, id: string): boolean {
    return this.eventAddedIds.get(collection)?.has(id) ?? false;
  }

  // ==================== CACHE POPULATION ====================

  /**
   * Fill cache from DB (merge, don't replace event-added entities)
   * Called when loading data from database queries.
   */
  populate<T extends BaseEntity>(collection: string, entities: T[]): void {
    const store = this.ensureStore<T>(collection);
    const eventAdded = this.eventAddedIds.get(collection) ?? new Set();

    for (const entity of entities) {
      const id = entity.id;

      // If entity is now in DB, it's no longer "event-only"
      eventAdded.delete(id);

      // Add/update in cache
      store.entities.set(id, entity);
    }

    this.eventAddedIds.set(collection, eventAdded);
    this.scheduleNotify(collection, { type: 'populated', collection });
  }

  /**
   * Populate with diff - only add new entities, preserve event-added ones
   * Used by EntityScroller-style refresh operations.
   */
  populateWithDiff<T extends BaseEntity>(
    collection: string,
    entities: T[],
    filter?: (e: T) => boolean
  ): void {
    const store = this.ensureStore<T>(collection);
    const eventAdded = this.eventAddedIds.get(collection) ?? new Set();

    const newIds = new Set(entities.map(e => e.id));

    // Get existing IDs that match the filter (if provided)
    const existingIds = new Set<string>();
    for (const [id, entity] of store.entities) {
      if (!filter || filter(entity as T)) {
        existingIds.add(id);
      }
    }

    // Remove entities no longer in DB result (but keep event-added ones)
    for (const id of existingIds) {
      if (!newIds.has(id) && !eventAdded.has(id)) {
        store.entities.delete(id);
      }
    }

    // Add new entities
    for (const entity of entities) {
      const id = entity.id;
      eventAdded.delete(id); // No longer event-only if in DB
      store.entities.set(id, entity);
    }

    this.eventAddedIds.set(collection, eventAdded);
    this.scheduleNotify(collection, { type: 'populated', collection });
  }

  /**
   * Clear all entities from a collection
   */
  clear(collection: string): void {
    const store = this.collections.get(collection);
    if (store) {
      store.entities.clear();
    }
    this.eventAddedIds.get(collection)?.clear();
    this.scheduleNotify(collection, { type: 'populated', collection });
  }

  // ==================== EVENT HANDLING (Positronic) ====================

  /**
   * Handle entity created event - add to cache immediately
   * Called when real-time events arrive (before DB persists)
   */
  onEntityCreated<T extends BaseEntity>(collection: string, entity: T): void {
    const store = this.ensureStore<T>(collection);
    const id = entity.id;

    // Track as event-added (survives DB refresh)
    let eventAdded = this.eventAddedIds.get(collection);
    if (!eventAdded) {
      eventAdded = new Set();
      this.eventAddedIds.set(collection, eventAdded);
    }
    eventAdded.add(id);

    // Add to cache
    store.entities.set(id, entity);

    // Notify subscribers
    this.scheduleNotify(collection, { type: 'created', entity, entityId: id, collection });
    this.notifyEntitySubscribers(collection, id, entity);
  }

  /**
   * Handle entity updated event
   */
  onEntityUpdated<T extends BaseEntity>(collection: string, entity: T): void {
    const store = this.collections.get(collection) as CollectionStore<T> | undefined;
    if (!store) return;

    const id = entity.id;

    // Only update if entity exists in cache
    if (store.entities.has(id)) {
      store.entities.set(id, entity);
      this.scheduleNotify(collection, { type: 'updated', entity, entityId: id, collection });
      this.notifyEntitySubscribers(collection, id, entity);
    }
  }

  /**
   * Handle entity deleted event
   */
  onEntityDeleted(collection: string, id: string): void {
    const store = this.collections.get(collection);
    if (!store) return;

    // Remove from cache
    store.entities.delete(id);

    // Remove from event-added tracking
    this.eventAddedIds.get(collection)?.delete(id);

    // Notify subscribers
    this.scheduleNotify(collection, { type: 'deleted', entityId: id, collection });
    this.notifyEntitySubscribers(collection, id, null);
  }

  // ==================== SUBSCRIPTIONS (React-like) ====================

  /**
   * Subscribe to collection changes
   * Callback receives current entities immediately, then on each change
   *
   * Automatically sets up event subscription for the collection if not already done.
   */
  subscribe<T extends BaseEntity>(
    collection: string,
    callback: CacheSubscriber<T>
  ): () => void {
    const store = this.ensureStore<T>(collection);
    store.subscribers.add(callback as CacheSubscriber<BaseEntity>);

    // Auto-subscribe to entity events for this collection (idempotent)
    this.ensureEventSubscription(collection);

    // Immediate callback with current state
    const entities = this.getAll<T>(collection);
    callback(entities, { type: 'populated', collection });

    // Return unsubscribe function
    return () => {
      store.subscribers.delete(callback as CacheSubscriber<BaseEntity>);
    };
  }

  /**
   * Subscribe to specific entity changes
   *
   * Automatically sets up event subscription for the collection if not already done.
   */
  subscribeToEntity<T extends BaseEntity>(
    collection: string,
    id: string,
    callback: EntitySubscriber<T>
  ): () => void {
    const store = this.ensureStore<T>(collection);

    // Auto-subscribe to entity events for this collection (idempotent)
    this.ensureEventSubscription(collection);

    let entitySubs = store.entitySubscribers.get(id);
    if (!entitySubs) {
      entitySubs = new Set();
      store.entitySubscribers.set(id, entitySubs);
    }
    entitySubs.add(callback as EntitySubscriber<BaseEntity>);

    // Immediate callback with current state
    const entity = this.get<T>(collection, id);
    callback(entity ?? null);

    // Return unsubscribe function
    return () => {
      entitySubs?.delete(callback as EntitySubscriber<BaseEntity>);
      if (entitySubs?.size === 0) {
        store.entitySubscribers.delete(id);
      }
    };
  }

  // ==================== STATISTICS ====================

  /**
   * Get cache statistics for debugging
   */
  getStats(): {
    collections: number;
    totalEntities: number;
    eventAddedCount: number;
    perCollection: Record<string, { cached: number; eventAdded: number }>;
  } {
    let totalEntities = 0;
    let eventAddedCount = 0;
    const perCollection: Record<string, { cached: number; eventAdded: number }> = {};

    for (const [name, store] of this.collections) {
      const cached = store.entities.size;
      const eventAdded = this.eventAddedIds.get(name)?.size ?? 0;

      totalEntities += cached;
      eventAddedCount += eventAdded;

      perCollection[name] = { cached, eventAdded };
    }

    return {
      collections: this.collections.size,
      totalEntities,
      eventAddedCount,
      perCollection
    };
  }

  // ==================== INTERNAL ====================

  /**
   * Ensure collection store exists
   */
  private ensureStore<T extends BaseEntity>(collection: string): CollectionStore<T> {
    let store = this.collections.get(collection);
    if (!store) {
      store = {
        entities: new Map(),
        subscribers: new Set(),
        entitySubscribers: new Map()
      };
      this.collections.set(collection, store);
    }
    // Type assertion required: collections Map stores BaseEntity but we return typed store
    return store as unknown as CollectionStore<T>;
  }

  /**
   * Schedule notification (batched via microtask)
   */
  private scheduleNotify<T extends BaseEntity>(collection: string, change: CacheChange<T>): void {
    let pending = this.pendingNotifications.get(collection);
    if (!pending) {
      pending = [];
      this.pendingNotifications.set(collection, pending);
    }
    pending.push(change as CacheChange<BaseEntity>);

    if (!this.notifyScheduled) {
      this.notifyScheduled = true;
      queueMicrotask(() => this.flushNotifications());
    }
  }

  /**
   * Flush all pending notifications
   */
  private flushNotifications(): void {
    this.notifyScheduled = false;

    for (const [collection, changes] of this.pendingNotifications) {
      const store = this.collections.get(collection);
      if (!store || store.subscribers.size === 0) continue;

      const entities = Array.from(store.entities.values());

      // Use the last change type for the notification
      const lastChange = changes[changes.length - 1];

      for (const subscriber of store.subscribers) {
        try {
          subscriber(entities, lastChange);
        } catch (error) {
          console.error(`EntityCacheService: Subscriber error for ${collection}:`, error);
        }
      }
    }

    this.pendingNotifications.clear();
  }

  /**
   * Notify entity-specific subscribers
   */
  private notifyEntitySubscribers<T extends BaseEntity>(
    collection: string,
    id: string,
    entity: T | null
  ): void {
    const store = this.collections.get(collection);
    const subscribers = store?.entitySubscribers.get(id);
    if (!subscribers) return;

    for (const subscriber of subscribers) {
      try {
        (subscriber as EntitySubscriber<T>)(entity);
      } catch (error) {
        console.error(`EntityCacheService: Entity subscriber error for ${collection}/${id}:`, error);
      }
    }
  }

  /**
   * Initialize event subscriptions for a collection
   * Call this to wire up real-time event updates
   */
  subscribeToEntityEvents(collection: string): () => void {
    const createdHandler = (entity: BaseEntity) => {
      this.onEntityCreated(collection, entity);
    };

    const updatedHandler = (entity: BaseEntity) => {
      this.onEntityUpdated(collection, entity);
    };

    // Handle both full entity and { id } formats (DataDaemon uses both)
    const deletedHandler = (data: BaseEntity | { id: string }) => {
      const id = data.id;
      this.onEntityDeleted(collection, id);
    };

    // Store unsubscribe functions for proper cleanup
    const unsubCreated = Events.subscribe(`data:${collection}:created`, createdHandler);
    const unsubUpdated = Events.subscribe(`data:${collection}:updated`, updatedHandler);
    const unsubDeleted = Events.subscribe(`data:${collection}:deleted`, deletedHandler);

    return () => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
    };
  }

  // Track collections with active event subscriptions
  private eventSubscriptions: Map<string, () => void> = new Map();

  /**
   * Subscribe to events for a collection (idempotent - won't double-subscribe)
   */
  ensureEventSubscription(collection: string): void {
    if (this.eventSubscriptions.has(collection)) {
      return; // Already subscribed
    }
    const unsubscribe = this.subscribeToEntityEvents(collection);
    this.eventSubscriptions.set(collection, unsubscribe);
  }

  /**
   * Subscribe to all entity events using wildcard pattern
   * This is the recommended way to set up global event handling
   */
  subscribeToAllEntityEvents(): () => void {
    // Use wildcard patterns to catch all entity events
    const createdHandler = (entity: BaseEntity & { _collection?: string }) => {
      // Try to determine collection from entity metadata or context
      // For now, we handle this at the populate/ensure level
      console.log('EntityCacheService: Received wildcard created event', entity);
    };

    const updatedHandler = (entity: BaseEntity) => {
      console.log('EntityCacheService: Received wildcard updated event', entity);
    };

    const deletedHandler = (data: BaseEntity | { id: string }) => {
      console.log('EntityCacheService: Received wildcard deleted event', data);
    };

    const unsubCreated = Events.subscribe('data:*:created', createdHandler);
    const unsubUpdated = Events.subscribe('data:*:updated', updatedHandler);
    const unsubDeleted = Events.subscribe('data:*:deleted', deletedHandler);

    return () => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
    };
  }

  /**
   * Unsubscribe from all collection events
   */
  unsubscribeAll(): void {
    for (const unsubscribe of this.eventSubscriptions.values()) {
      unsubscribe();
    }
    this.eventSubscriptions.clear();
  }
}

// Singleton export
export const entityCache = new EntityCacheServiceImpl();
