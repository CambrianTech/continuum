/**
 * Event System Constants - Universal constants for cross-environment event routing
 * 
 * Single source of truth for all event-related constants, priorities, and configuration.
 * Used by EventsDaemon, MessagePriorityStrategy, and all event-related components.
 */

/**
 * Event message priorities for router handling
 */
export const EVENT_PRIORITIES = {
  CRITICAL: 'CRITICAL', // Always delivered immediately, bypasses health checks
  HIGH: 'HIGH',         // Delivered immediately if system healthy
  NORMAL: 'NORMAL',     // Queued for batch delivery
  LOW: 'LOW'           // Lowest priority, delivered when system idle
} as const;

/**
 * Event daemon endpoint paths
 */
export const EVENT_ENDPOINTS = {
  BRIDGE: 'event-bridge',    // Cross-environment event bridging
  STATS: 'stats',           // Event bridge statistics
  HEALTH: 'health'          // Event system health check
} as const;

/**
 * Event bridge configuration
 */
export const EVENT_BRIDGE_CONFIG = {
  DEDUPLICATION_WINDOW_MS: 5000,    // Window for preventing duplicate events
  MAX_RETRY_ATTEMPTS: 3,            // Maximum retries for failed cross-env routing
  ROUTING_TIMEOUT_MS: 10000,        // Timeout for cross-environment routing
  QUEUE_FLUSH_INTERVAL_MS: 100      // How often to flush queued events
} as const;

/**
 * Event scope types for room/user/system scoping
 */
export const EVENT_SCOPES = {
  SYSTEM: 'system',    // System-wide events
  ROOM: 'room',       // Room-scoped events
  USER: 'user',       // User-scoped events  
  GLOBAL: 'global'    // Global cross-system events
} as const;

/**
 * Environment prefixes for cross-context routing
 */
export const ENVIRONMENT_PREFIXES = {
  BROWSER: 'browser',
  SERVER: 'server'
} as const;

/**
 * Common event names used throughout the system
 */
export const COMMON_EVENT_NAMES = {
  // Chat events
  CHAT_MESSAGE_SENT: 'chat-message-sent',
  CHAT_MESSAGE_RECEIVED: 'chat-message-received',
  CHAT_PARTICIPANT_JOINED: 'chat-participant-joined',
  CHAT_PARTICIPANT_LEFT: 'chat-participant-left',

  // System events
  SYSTEM_STATUS_CHANGE: 'system-status-change',
  SYSTEM_HEALTH_UPDATE: 'system-health-update',

  // Session events
  SESSION_CREATED: 'session-created',
  SESSION_DESTROYED: 'session-destroyed',

  // Command events
  COMMAND_EXECUTED: 'command-executed',
  COMMAND_FAILED: 'command-failed',

  // Canvas events (collaborative drawing)
  CANVAS_STROKE_ADDED: 'canvas:stroke:added',
  CANVAS_STROKE_DELETED: 'canvas:stroke:deleted',
  CANVAS_CLEARED: 'canvas:cleared',
  CANVAS_CURSOR_MOVED: 'canvas:cursor:moved'
} as const;

/**
 * Event metadata keys for bridge tracking
 */
export const EVENT_METADATA_KEYS = {
  BRIDGED: '_bridged',                    // Marks event as already bridged
  ORIGINAL_CONTEXT: '_originalContext',   // Original context UUID
  BRIDGE_TIMESTAMP: '_bridgeTimestamp',   // When event was bridged
  BRIDGE_HOP_COUNT: '_bridgeHopCount'     // Number of bridge hops (prevent loops)
} as const;

/**
 * Type definitions for better type safety
 */
export type EventPriority = typeof EVENT_PRIORITIES[keyof typeof EVENT_PRIORITIES];
export type EventEndpoint = typeof EVENT_ENDPOINTS[keyof typeof EVENT_ENDPOINTS];
export type EventScope = typeof EVENT_SCOPES[keyof typeof EVENT_SCOPES];
export type EnvironmentPrefix = typeof ENVIRONMENT_PREFIXES[keyof typeof ENVIRONMENT_PREFIXES];
export type CommonEventName = typeof COMMON_EVENT_NAMES[keyof typeof COMMON_EVENT_NAMES];
export type EventMetadataKey = typeof EVENT_METADATA_KEYS[keyof typeof EVENT_METADATA_KEYS];

/**
 * Event routing utilities
 */
export class EventRoutingUtils {
  /**
   * Normalize endpoint by removing environment and daemon prefixes
   */
  static normalizeEndpoint(endpoint: string): string {
    let normalized = endpoint;
    
    // Remove environment prefix (browser/, server/) if present
    normalized = normalized.replace(/^(browser|server)\//, '');
    
    // Remove daemon prefix (events/) if present  
    normalized = normalized.replace('events/', '');
    
    return normalized;
  }
  
  /**
   * Create cross-environment endpoint path
   */
  static createCrossEnvEndpoint(
    targetEnvironment: EnvironmentPrefix, 
    endpoint: EventEndpoint
  ): string {
    return `${targetEnvironment}/events/${endpoint}`;
  }
  
  /**
   * Check if event is already bridged to prevent infinite loops
   */
  static isEventBridged(eventData: any): boolean {
    return !!(eventData && eventData[EVENT_METADATA_KEYS.BRIDGED]);
  }
  
  /**
   * Add bridge metadata to event data
   */
  static addBridgeMetadata(
    eventData: any, 
    originalContext: string, 
    bridgeTimestamp: string
  ): any {
    return {
      ...eventData,
      [EVENT_METADATA_KEYS.BRIDGED]: true,
      [EVENT_METADATA_KEYS.ORIGINAL_CONTEXT]: originalContext,
      [EVENT_METADATA_KEYS.BRIDGE_TIMESTAMP]: bridgeTimestamp,
      [EVENT_METADATA_KEYS.BRIDGE_HOP_COUNT]: (eventData[EVENT_METADATA_KEYS.BRIDGE_HOP_COUNT] || 0) + 1
    };
  }
}

/**
 * Event system health constants
 */
export const EVENT_HEALTH_CONSTANTS = {
  MAX_QUEUE_SIZE: 1000,           // Maximum events in queue before dropping
  MAX_BRIDGE_HOP_COUNT: 5,        // Maximum bridge hops before dropping (prevent loops)
  HEALTH_CHECK_INTERVAL_MS: 5000, // How often to check event system health
  ERROR_THRESHOLD: 10             // Number of errors before marking unhealthy
} as const;

/**
 * Data event operations
 * CRUD operations that trigger data events
 */
export const DATA_EVENT_OPERATIONS = {
  CREATED: 'created',
  UPDATED: 'updated',
  DELETED: 'deleted',
  READ: 'read'
} as const;

export type DataEventOperation = typeof DATA_EVENT_OPERATIONS[keyof typeof DATA_EVENT_OPERATIONS];

/**
 * Utility for creating type-safe data event names
 * Single source of truth for data:collection:operation event naming
 *
 * @example
 * DataEventNames.forCollection('chat_messages', 'created')
 * // Returns: 'data:chat_messages:created'
 *
 * DataEventNames.created('users')
 * // Returns: 'data:users:created'
 */
export class DataEventNames {
  /**
   * Create data event name from collection and operation
   * Format: data:collection:operation
   */
  static forCollection(collection: string, operation: DataEventOperation): string {
    return `data:${collection}:${operation}`;
  }

  /**
   * Create 'created' event name for collection
   */
  static created(collection: string): string {
    return this.forCollection(collection, DATA_EVENT_OPERATIONS.CREATED);
  }

  /**
   * Create 'updated' event name for collection
   */
  static updated(collection: string): string {
    return this.forCollection(collection, DATA_EVENT_OPERATIONS.UPDATED);
  }

  /**
   * Create 'deleted' event name for collection
   */
  static deleted(collection: string): string {
    return this.forCollection(collection, DATA_EVENT_OPERATIONS.DELETED);
  }

  /**
   * Create 'read' event name for collection
   */
  static read(collection: string): string {
    return this.forCollection(collection, DATA_EVENT_OPERATIONS.READ);
  }

  /**
   * Parse event name to extract collection and operation
   * Returns null if not a valid data event name
   */
  static parse(eventName: string): { collection: string; operation: DataEventOperation } | null {
    const match = eventName.match(/^data:(.+?):(created|updated|deleted|read)$/);
    if (!match) return null;

    return {
      collection: match[1],
      operation: match[2] as DataEventOperation
    };
  }
}