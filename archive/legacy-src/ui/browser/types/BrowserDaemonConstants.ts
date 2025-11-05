/**
 * BrowserDaemonConstants - Type-safe constants for message types
 * 
 * Provides const assertions for all message types, combining the benefits of:
 * - Strong typing (no typos possible)
 * - IDE autocomplete and intellisense  
 * - Single source of truth
 * - Compile-time validation
 * - Runtime constant values for use in arrays/maps
 */

// Import shared event enums - single source of truth!
import { 
  ConsoleEvent,
  WebSocketEvent, 
  CommandEvent,
  WidgetEvent,
  SessionEvent,
  HealthEvent,
  SystemEvent 
} from '../../../types/shared/EventTypes';

/**
 * Console Daemon Message Constants
 * Now uses shared ConsoleEvent enum - single source of truth!
 */
export const CONSOLE_MESSAGES = ConsoleEvent;

/**
 * WebSocket Daemon Message Constants  
 * Now uses shared WebSocketEvent enum - single source of truth!
 */
export const WEBSOCKET_MESSAGES = WebSocketEvent;

/**
 * Command Daemon Message Constants
 * Now uses shared CommandEvent enum - single source of truth!
 */
export const COMMAND_MESSAGES = CommandEvent;

/**
 * Widget Daemon Message Constants
 * Now uses shared WidgetEvent enum - single source of truth!
 */
export const WIDGET_MESSAGES = WidgetEvent;

/**
 * Session Daemon Message Constants
 * Now uses shared SessionEvent enum - single source of truth!
 */
export const SESSION_MESSAGES = SessionEvent;

/**
 * Health Daemon Message Constants
 * Now uses shared HealthEvent enum - single source of truth!
 */
export const HEALTH_MESSAGES = HealthEvent;

/**
 * System Event Constants
 * Now uses shared SystemEvent enum - single source of truth!
 */
export const SYSTEM_EVENTS = SystemEvent;

/**
 * Utility functions to get arrays of message types
 * Perfect for use in getMessageTypes() methods
 */
export const MESSAGE_TYPE_ARRAYS = {
  console: Object.values(CONSOLE_MESSAGES),
  websocket: Object.values(WEBSOCKET_MESSAGES),
  command: Object.values(COMMAND_MESSAGES),
  widget: Object.values(WIDGET_MESSAGES),
  session: Object.values(SESSION_MESSAGES),
  health: Object.values(HEALTH_MESSAGES),
  system: Object.values(SYSTEM_EVENTS),
} as const;

/**
 * Type assertions are no longer needed since we're using the shared enums directly
 * The enums ARE the single source of truth, eliminating any possibility of drift
 * 
 * Previous approach used complex type assertions to ensure constants matched types,
 * but now we directly use the shared enums which are guaranteed to be consistent.
 */