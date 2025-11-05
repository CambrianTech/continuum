/**
 * Strongly-typed event names to prevent runtime errors
 * All events should be defined here as constants
 */

export enum SystemEventType {
  // WebSocket events
  WEBSOCKET_CONNECTION_ESTABLISHED = 'websocket:connection_established',
  WEBSOCKET_CONNECTION_CLOSED = 'websocket:connection_closed',
  WEBSOCKET_MESSAGE_RECEIVED = 'websocket:message_received',
  WEBSOCKET_ERROR = 'websocket:error',
  
  // Session events
  SESSION_CREATED = 'session:created',
  SESSION_JOINED = 'session:joined',
  SESSION_CLOSED = 'session:closed',
  SESSION_READY = 'session:ready',
  SESSION_ERROR = 'session:error',
  
  // Daemon lifecycle events
  DAEMON_STARTED = 'daemon:started',
  DAEMON_STOPPED = 'daemon:stopped',
  DAEMON_ERROR = 'daemon:error',
  DAEMON_HEALTH_CHECK = 'daemon:health_check',
  
  // Command events
  COMMAND_EXECUTED = 'command:executed',
  COMMAND_FAILED = 'command:failed',
  COMMAND_QUEUED = 'command:queued',
  
  // Browser events
  BROWSER_LAUNCHED = 'browser:launched',
  BROWSER_CLOSED = 'browser:closed',
  BROWSER_CONSOLE_LOG = 'browser:console_log',
  BROWSER_SCREENSHOT = 'browser:screenshot',
  
  // System events
  SYSTEM_STARTUP = 'system:startup',
  SYSTEM_SHUTDOWN = 'system:shutdown',
  SYSTEM_ERROR = 'system:error',
  SYSTEM_HEALTH = 'system:health'
}

/**
 * Type guard to check if a string is a valid event type
 */
export function isValidEventType(type: string): type is SystemEventType {
  return Object.values(SystemEventType).includes(type as SystemEventType);
}

// Base Session Event Data (for inheritance)
export interface SessionEventData {
  sessionId: string;
  sessionType: string;
  owner: string;
  timestamp?: string;
}

export interface WebSocketEventData {
  connectionId: string;
  metadata?: {
    userAgent: string;
    url: string;
    headers: Record<string, string>;
  };
}

export interface WebSocketConnectionEventData extends WebSocketEventData {
  timestamp: Date;
  source: string;
}

export interface WebSocketConnectionClosedEventData extends WebSocketEventData {
  reason?: string;
}

/**
 * Event categories for filtering and routing
 */
export enum EventCategory {
  WEBSOCKET = 'websocket',
  SESSION = 'session',
  DAEMON = 'daemon',
  COMMAND = 'command',
  BROWSER = 'browser',
  SYSTEM = 'system'
}

/**
 * Get the category of an event type
 */
export function getEventCategory(eventType: SystemEventType): EventCategory {
  const prefix = eventType.split(':')[0];
  switch (prefix) {
    case 'websocket':
      return EventCategory.WEBSOCKET;
    case 'session':
      return EventCategory.SESSION;
    case 'daemon':
      return EventCategory.DAEMON;
    case 'command':
      return EventCategory.COMMAND;
    case 'browser':
      return EventCategory.BROWSER;
    case 'system':
      return EventCategory.SYSTEM;
    default:
      return EventCategory.SYSTEM;
  }
}

/**
 * Event payload interfaces - strongly typed event data
 */
export interface WebSocketConnectionEstablishedPayload {
  timestamp: Date;
  source: string;
  connectionId: string;
  metadata: {
    userAgent: string;
    url: string;
    headers: Record<string, string>;
  };
}

export interface WebSocketConnectionClosedPayload {
  timestamp: Date;
  source: string;
  connectionId: string;
  reason?: string;
}

export interface SessionCreatedPayload {
  sessionId: string;
  serverLogPath: string;
  sessionType: string;
  owner: string;
  focus?: boolean;
  killZombies?: boolean;
}

export interface SessionJoinedPayload {
  sessionId: string;
  sessionType: string;
  owner: string;
  source: string;
  focus?: boolean;
  killZombies?: boolean;
}

export interface SessionClosedPayload {
  sessionId: string;
  reason?: string;
}

export interface BrowserLaunchedPayload {
  sessionId: string;
  browserPid: number;
  debugUrl: string;
  targetId?: string;
}

export interface CommandExecutedPayload {
  commandName: string;
  sessionId?: string;
  duration: number;
  success: boolean;
  error?: string;
}

/**
 * Message types for daemon communication
 */
export enum MessageType {
  // Session management
  CREATE_SESSION = 'create_session',
  JOIN_SESSION = 'join_session',
  CLOSE_SESSION = 'close_session',
  GET_SESSION = 'get_session',
  LIST_SESSIONS = 'list_sessions',
  
  // Connection management
  SEND_TO_CONNECTION = 'send_to_connection',
  BROADCAST_TO_SESSION = 'broadcast_to_session',
  REGISTER_CONNECTION = 'register_connection',
  UNREGISTER_CONNECTION = 'unregister_connection',
  
  // Logging
  SET_SESSION_LOG = 'set_session_log',
  START_CONSOLE_LOGGING = 'start_console_logging',
  STOP_CONSOLE_LOGGING = 'stop_console_logging',
  
  // Health and status
  HEALTH_CHECK = 'health_check',
  STATUS_REQUEST = 'status_request',
  HEARTBEAT = 'heartbeat',
  
  // Command routing
  EXECUTE_COMMAND = 'execute_command',
  ROUTE_COMMAND = 'route_command',
  
  // System control
  SHUTDOWN = 'shutdown',
  RESTART = 'restart',
  RELOAD_CONFIG = 'reload_config'
}