/**
 * Data Event Constants - Shared between server and browser
 *
 * Generic data events that work for all entity types
 */

/**
 * Generate data event name from collection
 */
export const getDataEventName = (collection: string, action: 'created' | 'updated' | 'deleted') =>
  `data:${collection}:${action}` as const;

/**
 * Data Created Event Structure
 * This is the EXACT structure that both sender and receiver must use
 * Generic for any entity type
 */
export interface DataCreatedEvent<T = any> {
  data: T; // The entity directly - same structure as data/list returns
}