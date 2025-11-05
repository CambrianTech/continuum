/**
 * Event Constants - Centralized Event Names
 *
 * All event names in the system are defined here to prevent typos
 * and ensure consistency across emitters and subscribers.
 *
 * Usage:
 *   import { DATA_EVENTS, UI_EVENTS, SYSTEM_EVENTS } from './EventConstants';
 *   await Events.emit(DATA_EVENTS.USERS.CREATED, user);
 *   Events.subscribe(UI_EVENTS.ROOM_SELECTED, callback);
 */

/**
 * CRUD Operation Types
 */
export type CrudOperation = 'created' | 'updated' | 'deleted' | 'truncated' | 'cleared';

/**
 * Data Event Categories - CRUD operations on entities
 * Pattern: data:{collection}:{operation}
 */
export const DATA_EVENTS = {
  // User Events
  USERS: {
    CREATED: 'data:users:created',
    UPDATED: 'data:users:updated',
    DELETED: 'data:users:deleted',
    TRUNCATED: 'data:users:truncated',
  },

  // User State Events
  USER_STATES: {
    CREATED: 'data:user_states:created',
    UPDATED: 'data:user_states:updated',
    DELETED: 'data:user_states:deleted',
    TRUNCATED: 'data:user_states:truncated',
  },

  // Room Events
  ROOMS: {
    CREATED: 'data:rooms:created',
    UPDATED: 'data:rooms:updated',
    DELETED: 'data:rooms:deleted',
    TRUNCATED: 'data:rooms:truncated',
  },

  // Chat Message Events
  CHAT_MESSAGES: {
    CREATED: 'data:chat_messages:created',
    UPDATED: 'data:chat_messages:updated',
    DELETED: 'data:chat_messages:deleted',
    TRUNCATED: 'data:chat_messages:truncated',
  },

  // Content Type Events
  CONTENT_TYPE: {
    CREATED: 'data:ContentType:created',
    UPDATED: 'data:ContentType:updated',
    DELETED: 'data:ContentType:deleted',
    TRUNCATED: 'data:ContentType:truncated',
  },

  // Training Session Events
  TRAINING_SESSION: {
    CREATED: 'data:TrainingSession:created',
    UPDATED: 'data:TrainingSession:updated',
    DELETED: 'data:TrainingSession:deleted',
    TRUNCATED: 'data:TrainingSession:truncated',
  },

  // Global Data Events
  ALL: {
    CLEARED: 'data:*:cleared',  // All collections cleared
  }
} as const;

/**
 * UI State Events - Browser-local only, no server persistence
 * Pattern: {domain}:{action}
 */
export const UI_EVENTS = {
  // Room Selection
  ROOM_SELECTED: 'room:selected',

  // Theme Changes
  THEME_CHANGED: 'theme:changed',

  // Sidebar State
  SIDEBAR_TOGGLED: 'sidebar:toggled',
  SIDEBAR_COLLAPSED: 'sidebar:collapsed',
  SIDEBAR_EXPANDED: 'sidebar:expanded',

  // Widget State
  WIDGET_FOCUSED: 'widget:focused',
  WIDGET_BLURRED: 'widget:blurred',
} as const;

/**
 * System Events - Global lifecycle events
 * Pattern: system:{event}
 */
export const SYSTEM_EVENTS = {
  READY: 'system:ready',
  SHUTDOWN: 'system:shutdown',
  ERROR: 'system:error',
  HEALTH_CHECK: 'system:health:check',
  HEALTH_OK: 'system:health:ok',
  HEALTH_DEGRADED: 'system:health:degraded',
  HEALTH_CRITICAL: 'system:health:critical',
} as const;

/**
 * Generate data event name dynamically from collection and operation
 *
 * @example
 * getDataEventName('users', 'created') // 'data:users:created'
 * getDataEventName('rooms', 'updated') // 'data:rooms:updated'
 */
export function getDataEventName(collection: string, operation: CrudOperation): string {
  return `data:${collection}:${operation}`;
}

/**
 * Parse data event name into collection and operation
 *
 * @example
 * parseDataEventName('data:users:created') // { collection: 'users', operation: 'created' }
 */
export function parseDataEventName(eventName: string): { collection: string; operation: CrudOperation } | null {
  const match = eventName.match(/^data:([^:]+):(.+)$/);
  if (!match) return null;

  return {
    collection: match[1],
    operation: match[2] as CrudOperation
  };
}

/**
 * Check if event is a data event
 */
export function isDataEvent(eventName: string): boolean {
  return eventName.startsWith('data:');
}

/**
 * Check if event is a UI event
 */
export function isUIEvent(eventName: string): boolean {
  return Object.values(UI_EVENTS).includes(eventName as any);
}

/**
 * Check if event is a system event
 */
export function isSystemEvent(eventName: string): boolean {
  return eventName.startsWith('system:');
}

/**
 * All event names as a flat array (for validation/debugging)
 */
export const ALL_EVENT_NAMES = [
  ...Object.values(DATA_EVENTS.USERS),
  ...Object.values(DATA_EVENTS.USER_STATES),
  ...Object.values(DATA_EVENTS.ROOMS),
  ...Object.values(DATA_EVENTS.CHAT_MESSAGES),
  ...Object.values(DATA_EVENTS.CONTENT_TYPE),
  ...Object.values(DATA_EVENTS.TRAINING_SESSION),
  ...Object.values(DATA_EVENTS.ALL),
  ...Object.values(UI_EVENTS),
  ...Object.values(SYSTEM_EVENTS),
] as const;

/**
 * Type-safe event name union
 */
export type EventName = typeof ALL_EVENT_NAMES[number] | string;

/**
 * Event payload types (extend as needed)
 */
export interface EventPayloads {
  // Data events have generic entity payloads (defined by BaseEntity extensions)
  [key: `data:${string}:${CrudOperation}`]: any;

  // UI events
  'room:selected': { roomId: string; roomName: string };
  'theme:changed': { theme: string };
  'sidebar:toggled': { collapsed: boolean };

  // System events
  'system:ready': { timestamp: string };
  'system:error': { error: Error; context: string };
}

/**
 * Type-safe event emission helper
 */
export type EmitPayload<T extends EventName> = T extends keyof EventPayloads
  ? EventPayloads[T]
  : any;
