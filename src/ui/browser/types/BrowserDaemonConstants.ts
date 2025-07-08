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

import type { 
  ConsoleMessageType, 
  WebSocketMessageType, 
  CommandMessageType,
  WidgetMessageType,
  SessionMessageType,
  HealthMessageType,
  SystemEventType 
} from './BrowserDaemonMessageTypes';

/**
 * Console Daemon Message Constants
 * Compile-time checked against ConsoleMessageType
 */
export const CONSOLE_MESSAGES = {
  // Core console operations
  CAPTURE: 'console:capture' as const,
  ENABLE: 'console:enable' as const,
  DISABLE: 'console:disable' as const,
  
  // Configuration and state
  SET_SESSION: 'console:set_session' as const,
  GET_STATUS: 'console:get_status' as const,
  SET_LOG_LEVEL: 'console:set_log_level' as const,
  
  // Advanced features
  FLUSH_QUEUE: 'console:flush_queue' as const,
  CLEAR_HISTORY: 'console:clear_history' as const,
  EXPORT_LOGS: 'console:export_logs' as const,
} as const;

/**
 * WebSocket Daemon Message Constants
 * Compile-time checked against WebSocketMessageType
 */
export const WEBSOCKET_MESSAGES = {
  // Connection management
  CONNECT: 'websocket:connect' as const,
  DISCONNECT: 'websocket:disconnect' as const,
  RECONNECT: 'websocket:reconnect' as const,
  
  // Message handling
  SEND: 'websocket:send' as const,
  RECEIVE: 'websocket:receive' as const,
  BROADCAST: 'websocket:broadcast' as const,
  
  // Command execution
  EXECUTE_COMMAND: 'websocket:execute_command' as const,
  CANCEL_COMMAND: 'websocket:cancel_command' as const,
  
  // Event subscription
  SUBSCRIBE: 'websocket:subscribe' as const,
  UNSUBSCRIBE: 'websocket:unsubscribe' as const,
  
  // Status and monitoring
  STATUS: 'websocket:status' as const,
  HEALTH_CHECK: 'websocket:health_check' as const,
  GET_METRICS: 'websocket:get_metrics' as const,
} as const;

/**
 * Command Daemon Message Constants
 * Compile-time checked against CommandMessageType
 */
export const COMMAND_MESSAGES = {
  // Command execution
  EXECUTE: 'command:execute' as const,
  CANCEL: 'command:cancel' as const,
  RETRY: 'command:retry' as const,
  
  // Command discovery
  LIST_COMMANDS: 'command:list_commands' as const,
  GET_DEFINITION: 'command:get_definition' as const,
  VALIDATE_PARAMS: 'command:validate_params' as const,
  
  // Command history
  GET_HISTORY: 'command:get_history' as const,
  CLEAR_HISTORY: 'command:clear_history' as const,
  
  // Status and monitoring
  STATUS: 'command:status' as const,
  GET_METRICS: 'command:get_metrics' as const,
} as const;

/**
 * Widget Daemon Message Constants
 * Compile-time checked against WidgetMessageType
 */
export const WIDGET_MESSAGES = {
  // Widget lifecycle
  REGISTER: 'widget:register' as const,
  UNREGISTER: 'widget:unregister' as const,
  LOAD: 'widget:load' as const,
  UNLOAD: 'widget:unload' as const,
  
  // Widget discovery
  DISCOVER: 'widget:discover' as const,
  LIST_WIDGETS: 'widget:list_widgets' as const,
  GET_DEFINITION: 'widget:get_definition' as const,
  
  // Widget communication
  SEND_MESSAGE: 'widget:send_message' as const,
  BROADCAST: 'widget:broadcast' as const,
  
  // Widget state
  GET_STATE: 'widget:get_state' as const,
  SET_STATE: 'widget:set_state' as const,
  
  // Status and monitoring
  STATUS: 'widget:status' as const,
  HEALTH_CHECK: 'widget:health_check' as const,
} as const;

/**
 * Session Daemon Message Constants
 * Compile-time checked against SessionMessageType
 */
export const SESSION_MESSAGES = {
  // Session lifecycle
  CREATE: 'session:create' as const,
  JOIN: 'session:join' as const,
  LEAVE: 'session:leave' as const,
  DESTROY: 'session:destroy' as const,
  
  // Session data
  GET_DATA: 'session:get_data' as const,
  SET_DATA: 'session:set_data' as const,
  UPDATE_DATA: 'session:update_data' as const,
  CLEAR_DATA: 'session:clear_data' as const,
  
  // Session events
  SUBSCRIBE_EVENTS: 'session:subscribe_events' as const,
  UNSUBSCRIBE_EVENTS: 'session:unsubscribe_events' as const,
  
  // Status and monitoring
  STATUS: 'session:status' as const,
  LIST_SESSIONS: 'session:list_sessions' as const,
  GET_METRICS: 'session:get_metrics' as const,
} as const;

/**
 * Health Daemon Message Constants
 * Compile-time checked against HealthMessageType
 */
export const HEALTH_MESSAGES = {
  // Health checks
  CHECK: 'health:check' as const,
  VALIDATE: 'health:validate' as const,
  DIAGNOSE: 'health:diagnose' as const,
  
  // Health monitoring
  START_MONITORING: 'health:start_monitoring' as const,
  STOP_MONITORING: 'health:stop_monitoring' as const,
  GET_REPORT: 'health:get_report' as const,
  
  // Health configuration
  SET_THRESHOLDS: 'health:set_thresholds' as const,
  GET_THRESHOLDS: 'health:get_thresholds' as const,
  
  // Status and metrics
  STATUS: 'health:status' as const,
  GET_METRICS: 'health:get_metrics' as const,
  EXPORT_REPORT: 'health:export_report' as const,
} as const;

/**
 * System Event Constants
 * Compile-time checked against SystemEventType
 */
export const SYSTEM_EVENTS = {
  // Daemon lifecycle
  DAEMON_STARTED: 'system:daemon_started' as const,
  DAEMON_STOPPED: 'system:daemon_stopped' as const,
  DAEMON_ERROR: 'system:daemon_error' as const,
  
  // Connection events
  CONNECTED: 'system:connected' as const,
  DISCONNECTED: 'system:disconnected' as const,
  CONNECTION_ERROR: 'system:connection_error' as const,
  
  // Session events
  SESSION_READY: 'system:session_ready' as const,
  SESSION_ENDED: 'system:session_ended' as const,
  
  // System events
  EMERGENCY_SHUTDOWN: 'system:emergency_shutdown' as const,
  CONFIGURATION_CHANGED: 'system:configuration_changed' as const,
  
  // Performance events
  PERFORMANCE_WARNING: 'system:performance_warning' as const,
  MEMORY_WARNING: 'system:memory_warning' as const,
} as const;

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
 * Type assertions to ensure constants match types
 * These will cause compile errors if constants drift from types
 */
type _ConsoleAssert = typeof CONSOLE_MESSAGES[keyof typeof CONSOLE_MESSAGES] extends ConsoleMessageType ? true : never;
type _WebSocketAssert = typeof WEBSOCKET_MESSAGES[keyof typeof WEBSOCKET_MESSAGES] extends WebSocketMessageType ? true : never;
type _CommandAssert = typeof COMMAND_MESSAGES[keyof typeof COMMAND_MESSAGES] extends CommandMessageType ? true : never;
type _WidgetAssert = typeof WIDGET_MESSAGES[keyof typeof WIDGET_MESSAGES] extends WidgetMessageType ? true : never;
type _SessionAssert = typeof SESSION_MESSAGES[keyof typeof SESSION_MESSAGES] extends SessionMessageType ? true : never;
type _HealthAssert = typeof HEALTH_MESSAGES[keyof typeof HEALTH_MESSAGES] extends HealthMessageType ? true : never;
type _SystemAssert = typeof SYSTEM_EVENTS[keyof typeof SYSTEM_EVENTS] extends SystemEventType ? true : never;

// Force type checking (these lines will error if types don't match)
const _typeChecks: [_ConsoleAssert, _WebSocketAssert, _CommandAssert, _WidgetAssert, _SessionAssert, _HealthAssert, _SystemAssert] = 
  [true, true, true, true, true, true, true];