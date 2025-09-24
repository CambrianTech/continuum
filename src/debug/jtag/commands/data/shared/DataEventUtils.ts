/**
 * Data Event Utilities - Universal CRUD Event Subscriptions
 *
 * Provides convenience methods for subscribing to all CRUD operations
 * with wildcard pattern matching.
 */

import { Events } from '../../../system/core/client/shared/Events';
import { getDataEventName } from './DataEventConstants';

/**
 * CRUD operation actions
 */
export type CrudAction = 'created' | 'updated' | 'deleted';

/**
 * CRUD event handler with action type
 */
export interface CrudEventHandler<T> {
  (eventData: T, action: CrudAction): void;
}

/**
 * Subscribe to all CRUD operations for a collection using wildcard pattern
 *
 * @example
 * subscribeToAllCrudEvents('User', (user, action) => {
 *   console.log(`User ${action}:`, user);
 *   if (action === 'created') scroller.add(user);
 *   if (action === 'updated') scroller.update(user.id, user);
 *   if (action === 'deleted') scroller.remove(user.id);
 * });
 */
export function subscribeToAllCrudEvents<T>(
  collection: string,
  handler: CrudEventHandler<T>
): () => void {
  console.log(`🎧 DataEventUtils: Setting up universal CRUD subscription for ${collection}`);

  // Subscribe to individual CRUD events until wildcard infrastructure is complete
  const unsubscribeFunctions: (() => void)[] = [];

  (['created', 'updated', 'deleted'] as CrudAction[]).forEach(action => {
    const eventName = getDataEventName(collection, action);
    const unsubscribe = Events.subscribe<T>(eventName, (eventData: T) => {
      console.log(`🔥 DataEventUtils: CRUD event received - ${collection}.${action}`, eventData);

      try {
        handler(eventData, action);
      } catch (error) {
        console.error(`❌ DataEventUtils: Error in CRUD handler for ${collection}.${action}:`, error);
      }
    });

    unsubscribeFunctions.push(unsubscribe);
  });

  // Return function that unsubscribes from all
  return () => {
    unsubscribeFunctions.forEach(unsubscribe => unsubscribe());
    console.log(`🔌 DataEventUtils: Unsubscribed from all CRUD events for ${collection}`);
  };
}


/**
 * Subscribe to specific CRUD action for a collection
 *
 * @example subscribeToSpecificCrudEvent('User', 'updated', (user) => scroller.update(user.id, user))
 */
export function subscribeToSpecificCrudEvent<T>(
  collection: string,
  action: CrudAction,
  handler: (eventData: T) => void
): () => void {
  const eventName = getDataEventName(collection, action);

  console.log(`🎧 DataEventUtils: Setting up specific CRUD subscription for ${eventName}`);

  return Events.subscribe<T>(eventName, handler);
}

/**
 * Subscribe to specific CRUD actions using regex-like patterns
 *
 * @example
 * subscribeToSelectedCrudEvents('User', ['updated', 'deleted'], handler)
 * subscribeToSelectedCrudEvents('ChatMessage', ['created'], handler)
 */
export function subscribeToSelectedCrudEvents<T>(
  collection: string,
  actions: CrudAction[],
  handler: CrudEventHandler<T>
): () => void {
  console.log(`🎧 DataEventUtils: Setting up selective CRUD subscription for ${collection}:${actions.join('|')}`);

  const unsubscribeFunctions: (() => void)[] = [];

  actions.forEach(action => {
    const eventName = getDataEventName(collection, action);
    const unsubscribe = Events.subscribe<T>(eventName, (eventData: T) => {
      console.log(`🔥 DataEventUtils: Selective CRUD event received - ${collection}.${action}`, eventData);

      try {
        handler(eventData, action);
      } catch (error) {
        console.error(`❌ DataEventUtils: Error in selective CRUD handler for ${collection}.${action}:`, error);
      }
    });

    unsubscribeFunctions.push(unsubscribe);
  });

  // Return function that unsubscribes from all
  return () => {
    unsubscribeFunctions.forEach(unsubscribe => unsubscribe());
    console.log(`🔌 DataEventUtils: Unsubscribed from selective CRUD events ${collection}:${actions.join('|')}`);
  };
}

/**
 * Create a unified entity handler that works with EntityScroller
 *
 * @example
 * const unsubscribe = createEntityCrudHandler('User', userScroller);
 */
export function createEntityCrudHandler<T extends { id: string }>(
  collection: string,
  scroller: {
    add: (entity: T) => void;
    update: (id: string, entity: T) => void;
    remove: (id: string) => void;
  }
): () => void {
  return subscribeToAllCrudEvents<T>(collection, (entity: T, action: CrudAction) => {
    console.log(`🔧 DataEventUtils: EntityScroller ${action} for ${collection}/${entity.id}`);

    switch (action) {
      case 'created':
        scroller.add(entity);
        break;
      case 'updated':
        scroller.update(entity.id, entity);
        break;
      case 'deleted':
        scroller.remove(entity.id);
        break;
      default:
        console.warn(`⚠️ DataEventUtils: Unknown action ${action} for ${collection}`);
    }
  });
}