/**
 * Widget Constants
 * 
 * Centralized constants to eliminate magic strings in widget system.
 */

// Theme names
export const THEME_NAMES = {
  BASIC: 'basic',
  CYBERPUNK: 'cyberpunk', 
  ANIME: 'anime',
  CUSTOM: 'custom'
} as const;

// Default configuration values
export const WIDGET_DEFAULTS = {
  THEME: THEME_NAMES.CYBERPUNK,
  ENVIRONMENT: 'browser',
  USER_ID: 'current_user',
  PERMISSIONS: ['read', 'write'] as const,
  CAPABILITIES: ['screenshot', 'file/save', 'ai_integration'] as const,
  SHADOW_MODE: 'open'
} as const;

// Database operation types
export const DATABASE_OPERATIONS = {
  STORE: 'store',
  RETRIEVE: 'retrieve',
  DELETE: 'delete',
  LIST: 'list'
} as const;

// Router operation types  
export const ROUTER_OPERATIONS = {
  BROADCAST: 'broadcast',
  SUBSCRIBE: 'subscribe',
  UNSUBSCRIBE: 'unsubscribe'
} as const;

// Academy operation types
export const ACADEMY_OPERATIONS = {
  QUERY: 'query',
  ANALYZE: 'analyze',
  GENERATE: 'generate'
} as const;

// Event types
export const WIDGET_EVENTS = {
  DATA_UPDATED: 'data_updated',
  THEME_CHANGED: 'theme_changed',
  WIDGET_READY: 'widget_ready',
  WIDGET_ERROR: 'widget_error'
} as const;

// Channel names
export const WIDGET_CHANNELS = {
  WIDGET_EVENTS: 'widget_events',
  THEME_UPDATES: 'theme_updates',
  DATA_SYNC: 'data_sync'
} as const;

// Default personas
export const AI_PERSONAS = {
  GENERAL_ASSISTANT: 'general_assistant',
  CHAT_ASSISTANT: 'chat_assistant',
  CODE_ASSISTANT: 'code_assistant',
  CREATIVE_ASSISTANT: 'creative_assistant'
} as const;

// Default directories
export const WIDGET_DIRECTORIES = {
  WIDGET_DATA: 'widget_data',
  SCREENSHOTS: 'screenshots',
  EXPORTS: 'exports',
  THEMES: 'themes'
} as const;

// Storage keys
export const STORAGE_KEYS = {
  CURRENT_THEME: 'current_theme',
  WIDGET_STATE: 'widget_state',
  USER_PREFERENCES: 'user_preferences'
} as const;

// JTAG daemon names (existing daemons from the system)
export const DAEMON_NAMES = {
  COMMAND: 'command',
  DATA: 'data', 
  WIDGET: 'widget',
  HEALTH: 'health',
  CONSOLE: 'console',
  CHAT: 'chat',
  EVENTS: 'events',
  SESSION: 'session'
} as const;

// Widget attribute names - SINGLE SOURCE OF TRUTH
// Use these instead of string literals to avoid 'entity-id' vs 'data-entity-id' bugs
export const WIDGET_ATTRS = {
  /** Entity ID - the primary identifier for the widget's data */
  ENTITY_ID: 'entity-id',

  /** Data entity ID - HTML5 data attribute form (for backwards compatibility) */
  DATA_ENTITY_ID: 'data-entity-id',

  /** Room attribute - for chat widgets pinned to specific rooms */
  ROOM: 'room',

  /** Compact mode attribute */
  COMPACT: 'compact',

  /** Activity ID - for collaborative activities */
  ACTIVITY_ID: 'activity-id',

  /** Message ID - for message elements */
  MESSAGE_ID: 'message-id',

  /** Pool ID - for element pooling */
  POOL_ID: 'data-pool-id',
} as const;

/**
 * Get entity ID from a widget element, checking all possible attribute names
 *
 * @param element - The widget element to get entity ID from
 * @returns The entity ID or undefined if not found
 */
export function getWidgetEntityId(element: HTMLElement): string | undefined {
  return element.getAttribute(WIDGET_ATTRS.ENTITY_ID)
    || element.getAttribute(WIDGET_ATTRS.DATA_ENTITY_ID)
    || undefined;
}

/**
 * Set entity ID on a widget element using the standard attribute name
 *
 * @param element - The widget element to set entity ID on
 * @param entityId - The entity ID to set (or undefined to remove)
 */
export function setWidgetEntityId(element: HTMLElement, entityId: string | undefined): void {
  if (entityId) {
    element.setAttribute(WIDGET_ATTRS.ENTITY_ID, entityId);
  } else {
    element.removeAttribute(WIDGET_ATTRS.ENTITY_ID);
    element.removeAttribute(WIDGET_ATTRS.DATA_ENTITY_ID);
  }
}

// Type exports for strict typing
export type ThemeName = typeof THEME_NAMES[keyof typeof THEME_NAMES];
export type DatabaseOperation = typeof DATABASE_OPERATIONS[keyof typeof DATABASE_OPERATIONS];
export type RouterOperation = typeof ROUTER_OPERATIONS[keyof typeof ROUTER_OPERATIONS];
export type AcademyOperation = typeof ACADEMY_OPERATIONS[keyof typeof ACADEMY_OPERATIONS];
export type WidgetEvent = typeof WIDGET_EVENTS[keyof typeof WIDGET_EVENTS];
export type WidgetChannel = typeof WIDGET_CHANNELS[keyof typeof WIDGET_CHANNELS];
export type AIPersona = typeof AI_PERSONAS[keyof typeof AI_PERSONAS];
export type WidgetDirectory = typeof WIDGET_DIRECTORIES[keyof typeof WIDGET_DIRECTORIES];
export type DaemonName = typeof DAEMON_NAMES[keyof typeof DAEMON_NAMES];
export type WidgetAttr = typeof WIDGET_ATTRS[keyof typeof WIDGET_ATTRS];