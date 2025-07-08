/**
 * Shared Event Enums - Single Source of Truth
 * 
 * These enums are used by both:
 * - Core server daemons (WebSocketDaemon, CommandProcessor, etc.)
 * - Browser client daemons (BrowserWebSocketDaemon, BrowserConsoleDaemon, etc.)
 * 
 * This ensures perfect type alignment and prevents message routing errors.
 */

/**
 * WebSocket Events - Connection and message handling
 */
export enum WebSocketEvent {
  // Connection lifecycle
  CONNECT = 'websocket:connect',
  DISCONNECT = 'websocket:disconnect', 
  RECONNECT = 'websocket:reconnect',
  CONNECTION_ERROR = 'websocket:connection_error',
  
  // Message handling
  SEND = 'websocket:send',
  RECEIVE = 'websocket:receive',
  BROADCAST = 'websocket:broadcast',
  
  // Command routing
  EXECUTE_COMMAND = 'websocket:execute_command',
  CANCEL_COMMAND = 'websocket:cancel_command',
  
  // Event subscription
  SUBSCRIBE = 'websocket:subscribe',
  UNSUBSCRIBE = 'websocket:unsubscribe',
  
  // Status and monitoring
  STATUS = 'websocket:status',
  HEALTH_CHECK = 'websocket:health_check',
  GET_METRICS = 'websocket:get_metrics'
}

/**
 * Console Events - Log forwarding and capture
 */
export enum ConsoleEvent {
  // Core console operations
  CAPTURE = 'console:capture',
  ENABLE = 'console:enable',
  DISABLE = 'console:disable',
  
  // Configuration and state
  SET_SESSION = 'console:set_session',
  GET_STATUS = 'console:get_status',
  SET_LOG_LEVEL = 'console:set_log_level',
  
  // Advanced features
  FLUSH_QUEUE = 'console:flush_queue',
  CLEAR_HISTORY = 'console:clear_history',
  EXPORT_LOGS = 'console:export_logs',
  
  // Log forwarding (existing)
  LOG = 'console:log',
  BATCH_LOGS = 'console:batch_logs', 
  FLUSH_LOGS = 'console:flush_logs',
  
  // Console management (existing)
  ENABLE_CAPTURE = 'console:enable_capture',
  DISABLE_CAPTURE = 'console:disable_capture',
  GET_BUFFER = 'console:get_buffer',
  CLEAR_BUFFER = 'console:clear_buffer',
  
  // Configuration (existing)
  SET_LEVEL = 'console:set_level',
  GET_CONFIG = 'console:get_config'
}

/**
 * Widget Events - Widget lifecycle and communication
 */
export enum WidgetEvent {
  // Lifecycle
  REGISTER = 'widget:register',
  UNREGISTER = 'widget:unregister',
  LOAD = 'widget:load',
  UNLOAD = 'widget:unload',
  
  // Discovery
  DISCOVER = 'widget:discover',
  LIST_WIDGETS = 'widget:list_widgets',
  GET_DEFINITION = 'widget:get_definition',
  
  // Communication
  SEND_MESSAGE = 'widget:send_message',
  BROADCAST = 'widget:broadcast',
  
  // State management
  GET_STATE = 'widget:get_state',
  SET_STATE = 'widget:set_state',
  UPDATE_STATE = 'widget:update_state',
  
  // Server control events (new)
  SCREENSHOT = 'widget:screenshot',
  REFRESH = 'widget:refresh',
  EXPORT = 'widget:export',
  VALIDATE = 'widget:validate',
  
  // Status
  STATUS = 'widget:status',
  HEALTH_CHECK = 'widget:health_check'
}

/**
 * Command Events - Command execution and management
 */
export enum CommandEvent {
  // Command execution
  EXECUTE = 'command:execute',
  CANCEL = 'command:cancel',
  RETRY = 'command:retry',
  
  // Command discovery
  LIST_COMMANDS = 'command:list_commands',
  GET_DEFINITION = 'command:get_definition',
  VALIDATE_PARAMS = 'command:validate_params',
  
  // Command history
  GET_HISTORY = 'command:get_history',
  CLEAR_HISTORY = 'command:clear_history',
  
  // Status and monitoring
  STATUS = 'command:status',
  GET_METRICS = 'command:get_metrics',
  
  // Execution (existing)
  TIMEOUT = 'command:timeout',
  
  // Discovery (existing)
  DISCOVER = 'command:discover',
  
  // Results (existing)
  SUCCESS = 'command:success',
  ERROR = 'command:error',
  PROGRESS = 'command:progress',
  
  // Queue management (existing)
  QUEUE_STATUS = 'command:queue_status',
  CLEAR_QUEUE = 'command:clear_queue'
}

/**
 * Session Events - Session lifecycle and management
 */
export enum SessionEvent {
  // Session lifecycle
  CREATE = 'session:create',
  JOIN = 'session:join',
  LEAVE = 'session:leave',
  DESTROY = 'session:destroy',
  
  // Session data
  GET_DATA = 'session:get_data',
  SET_DATA = 'session:set_data',
  UPDATE_DATA = 'session:update_data',
  CLEAR_DATA = 'session:clear_data',
  
  // Session events
  SUBSCRIBE_EVENTS = 'session:subscribe_events',
  UNSUBSCRIBE_EVENTS = 'session:unsubscribe_events',
  
  // Status and monitoring
  STATUS = 'session:status',
  LIST_SESSIONS = 'session:list_sessions',
  GET_METRICS = 'session:get_metrics',
  
  // State management (existing)
  READY = 'session:ready',
  UPDATE = 'session:update',
  SYNC = 'session:sync',
  
  // User management (existing)
  USER_JOIN = 'session:user_join',
  USER_LEAVE = 'session:user_leave',
  
  // Events (existing)
  EVENT = 'session:event',
  BROADCAST = 'session:broadcast'
}

/**
 * Health Events - System health and monitoring
 */
export enum HealthEvent {
  // Health checks
  CHECK = 'health:check',
  VALIDATE = 'health:validate',
  DIAGNOSE = 'health:diagnose',
  
  // Health monitoring
  START_MONITORING = 'health:start_monitoring',
  STOP_MONITORING = 'health:stop_monitoring',
  GET_REPORT = 'health:get_report',
  
  // Health configuration
  SET_THRESHOLDS = 'health:set_thresholds',
  GET_THRESHOLDS = 'health:get_thresholds',
  
  // Status and metrics
  STATUS = 'health:status',
  GET_METRICS = 'health:get_metrics',
  EXPORT_REPORT = 'health:export_report',
  
  // Alerts (existing)
  WARNING = 'health:warning',
  ERROR = 'health:error',
  CRITICAL = 'health:critical',
  
  // Performance (existing)
  PERFORMANCE_WARNING = 'health:performance_warning',
  MEMORY_WARNING = 'health:memory_warning',
  CPU_WARNING = 'health:cpu_warning'
}

/**
 * System Events - System-wide events
 */
export enum SystemEvent {
  // Daemon lifecycle
  DAEMON_STARTED = 'system:daemon_started',
  DAEMON_STOPPED = 'system:daemon_stopped',
  DAEMON_ERROR = 'system:daemon_error',
  
  // Connection events
  CONNECTED = 'system:connected',
  DISCONNECTED = 'system:disconnected',
  CONNECTION_ERROR = 'system:connection_error',
  
  // Session events
  SESSION_READY = 'system:session_ready',
  SESSION_ENDED = 'system:session_ended',
  
  // System events
  EMERGENCY_SHUTDOWN = 'system:emergency_shutdown',
  CONFIGURATION_CHANGED = 'system:configuration_changed',
  
  // Performance events
  PERFORMANCE_WARNING = 'system:performance_warning',
  MEMORY_WARNING = 'system:memory_warning',
  
  // System lifecycle (existing)
  STARTUP = 'system:startup',
  SHUTDOWN = 'system:shutdown',
  RESTART = 'system:restart',
  
  // Configuration (existing)
  CONFIG_CHANGE = 'system:config_change',
  RELOAD_CONFIG = 'system:reload_config',
  
  // Errors (existing)
  ERROR = 'system:error',
  CRITICAL_ERROR = 'system:critical_error'
}

/**
 * Union type of all events for type checking
 */
export type AllEvents = 
  | WebSocketEvent
  | ConsoleEvent  
  | WidgetEvent
  | CommandEvent
  | SessionEvent
  | HealthEvent
  | SystemEvent;

/**
 * Event category mapping for routing
 */
export const EventCategories = {
  websocket: Object.values(WebSocketEvent),
  console: Object.values(ConsoleEvent),
  widget: Object.values(WidgetEvent),
  command: Object.values(CommandEvent),
  session: Object.values(SessionEvent),
  health: Object.values(HealthEvent),
  system: Object.values(SystemEvent)
} as const;