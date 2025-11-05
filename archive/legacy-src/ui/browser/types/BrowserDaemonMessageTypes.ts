/**
 * BrowserDaemonMessageTypes - Centralized message type definitions
 * 
 * Defines all message types for browser daemon system using string literal types
 * for better TypeScript support, IDE autocomplete, and type safety.
 */

/**
 * Console Daemon Message Types
 * Handles console capture and forwarding functionality
 */
export type ConsoleMessageType =
  // Core console operations
  | 'console:capture'
  | 'console:enable'
  | 'console:disable'
  
  // Configuration and state
  | 'console:set_session'
  | 'console:get_status'
  | 'console:set_log_level'
  
  // Advanced features
  | 'console:flush_queue'
  | 'console:clear_history'
  | 'console:export_logs';

/**
 * WebSocket Daemon Message Types
 * Handles WebSocket connection management and communication
 */
export type WebSocketMessageType =
  // Connection management
  | 'websocket:connect'
  | 'websocket:disconnect'
  | 'websocket:reconnect'
  
  // Message handling
  | 'websocket:send'
  | 'websocket:receive'
  | 'websocket:broadcast'
  
  // Command execution
  | 'websocket:execute_command'
  | 'websocket:cancel_command'
  
  // Event subscription
  | 'websocket:subscribe'
  | 'websocket:unsubscribe'
  
  // Status and monitoring
  | 'websocket:status'
  | 'websocket:health_check'
  | 'websocket:get_metrics';

/**
 * Command Daemon Message Types
 * Handles command execution and routing (Phase 4)
 */
export type CommandMessageType =
  // Command execution
  | 'command:execute'
  | 'command:cancel'
  | 'command:retry'
  
  // Command discovery
  | 'command:list_commands'
  | 'command:get_definition'
  | 'command:validate_params'
  
  // Command history
  | 'command:get_history'
  | 'command:clear_history'
  
  // Status and monitoring
  | 'command:status'
  | 'command:get_metrics';

/**
 * Widget Daemon Message Types
 * Handles widget lifecycle and management (Phase 5)
 */
export type WidgetMessageType =
  // Widget lifecycle
  | 'widget:register'
  | 'widget:unregister'
  | 'widget:load'
  | 'widget:unload'
  
  // Widget discovery
  | 'widget:discover'
  | 'widget:list_widgets'
  | 'widget:get_definition'
  
  // Widget communication
  | 'widget:send_message'
  | 'widget:broadcast'
  
  // Widget state
  | 'widget:get_state'
  | 'widget:set_state'
  
  // Status and monitoring
  | 'widget:status'
  | 'widget:health_check';

/**
 * Session Daemon Message Types
 * Handles session state management (Phase 6)
 */
export type SessionMessageType =
  // Session lifecycle
  | 'session:create'
  | 'session:join'
  | 'session:leave'
  | 'session:destroy'
  
  // Session data
  | 'session:get_data'
  | 'session:set_data'
  | 'session:update_data'
  | 'session:clear_data'
  
  // Session events
  | 'session:subscribe_events'
  | 'session:unsubscribe_events'
  
  // Status and monitoring
  | 'session:status'
  | 'session:list_sessions'
  | 'session:get_metrics';

/**
 * Health Daemon Message Types
 * Handles health validation and monitoring (Phase 7)
 */
export type HealthMessageType =
  // Health checks
  | 'health:check'
  | 'health:validate'
  | 'health:diagnose'
  
  // Health monitoring
  | 'health:start_monitoring'
  | 'health:stop_monitoring'
  | 'health:get_report'
  
  // Health configuration
  | 'health:set_thresholds'
  | 'health:get_thresholds'
  
  // Status and metrics
  | 'health:status'
  | 'health:get_metrics'
  | 'health:export_report';

/**
 * System-wide Event Types
 * Events that can be emitted by any daemon
 */
export type SystemEventType =
  // Daemon lifecycle
  | 'system:daemon_started'
  | 'system:daemon_stopped'
  | 'system:daemon_error'
  
  // Connection events
  | 'system:connected'
  | 'system:disconnected'
  | 'system:connection_error'
  
  // Session events
  | 'system:session_ready'
  | 'system:session_ended'
  
  // System events
  | 'system:emergency_shutdown'
  | 'system:configuration_changed'
  
  // Performance events
  | 'system:performance_warning'
  | 'system:memory_warning';

/**
 * All browser daemon message types combined
 * Used for type unions and validation
 */
export type BrowserDaemonMessageType = 
  | ConsoleMessageType
  | WebSocketMessageType
  | CommandMessageType
  | WidgetMessageType
  | SessionMessageType
  | HealthMessageType
  | SystemEventType;

/**
 * Message type validation helpers
 * Uses the constants for runtime validation
 */
export class MessageTypeValidator {
  private static readonly CONSOLE_VALUES = [
    'console:capture', 'console:enable', 'console:disable',
    'console:set_session', 'console:get_status', 'console:set_log_level',
    'console:flush_queue', 'console:clear_history', 'console:export_logs'
  ] as const;

  private static readonly WEBSOCKET_VALUES = [
    'websocket:connect', 'websocket:disconnect', 'websocket:reconnect',
    'websocket:send', 'websocket:receive', 'websocket:broadcast',
    'websocket:execute_command', 'websocket:cancel_command',
    'websocket:subscribe', 'websocket:unsubscribe',
    'websocket:status', 'websocket:health_check', 'websocket:get_metrics'
  ] as const;

  private static readonly COMMAND_VALUES = [
    'command:execute', 'command:cancel', 'command:retry',
    'command:list_commands', 'command:get_definition', 'command:validate_params',
    'command:get_history', 'command:clear_history',
    'command:status', 'command:get_metrics'
  ] as const;

  private static readonly WIDGET_VALUES = [
    'widget:register', 'widget:unregister', 'widget:load', 'widget:unload',
    'widget:discover', 'widget:list_widgets', 'widget:get_definition',
    'widget:send_message', 'widget:broadcast',
    'widget:get_state', 'widget:set_state',
    'widget:status', 'widget:health_check'
  ] as const;

  private static readonly SESSION_VALUES = [
    'session:create', 'session:join', 'session:leave', 'session:destroy',
    'session:get_data', 'session:set_data', 'session:update_data', 'session:clear_data',
    'session:subscribe_events', 'session:unsubscribe_events',
    'session:status', 'session:list_sessions', 'session:get_metrics'
  ] as const;

  private static readonly HEALTH_VALUES = [
    'health:check', 'health:validate', 'health:diagnose',
    'health:start_monitoring', 'health:stop_monitoring', 'health:get_report',
    'health:set_thresholds', 'health:get_thresholds',
    'health:status', 'health:get_metrics', 'health:export_report'
  ] as const;

  private static readonly SYSTEM_VALUES = [
    'system:daemon_started', 'system:daemon_stopped', 'system:daemon_error',
    'system:connected', 'system:disconnected', 'system:connection_error',
    'system:session_ready', 'system:session_ended',
    'system:emergency_shutdown', 'system:configuration_changed',
    'system:performance_warning', 'system:memory_warning'
  ] as const;

  /**
   * Check if a message type is valid for console daemon
   */
  static isConsoleMessage(type: string): type is ConsoleMessageType {
    return this.CONSOLE_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is valid for websocket daemon
   */
  static isWebSocketMessage(type: string): type is WebSocketMessageType {
    return this.WEBSOCKET_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is valid for command daemon
   */
  static isCommandMessage(type: string): type is CommandMessageType {
    return this.COMMAND_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is valid for widget daemon
   */
  static isWidgetMessage(type: string): type is WidgetMessageType {
    return this.WIDGET_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is valid for session daemon
   */
  static isSessionMessage(type: string): type is SessionMessageType {
    return this.SESSION_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is valid for health daemon
   */
  static isHealthMessage(type: string): type is HealthMessageType {
    return this.HEALTH_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is a system event
   */
  static isSystemEvent(type: string): type is SystemEventType {
    return this.SYSTEM_VALUES.includes(type as any);
  }

  /**
   * Check if a message type is valid for any browser daemon
   */
  static isValidBrowserDaemonMessage(type: string): type is BrowserDaemonMessageType {
    return (
      this.isConsoleMessage(type) ||
      this.isWebSocketMessage(type) ||
      this.isCommandMessage(type) ||
      this.isWidgetMessage(type) ||
      this.isSessionMessage(type) ||
      this.isHealthMessage(type) ||
      this.isSystemEvent(type)
    );
  }

  /**
   * Get the daemon type for a given message type
   */
  static getDaemonTypeForMessage(type: string): string | null {
    if (this.isConsoleMessage(type)) return 'console';
    if (this.isWebSocketMessage(type)) return 'websocket';
    if (this.isCommandMessage(type)) return 'command';
    if (this.isWidgetMessage(type)) return 'widget';
    if (this.isSessionMessage(type)) return 'session';
    if (this.isHealthMessage(type)) return 'health';
    if (this.isSystemEvent(type)) return 'system';
    return null;
  }

  /**
   * Get all message types for a specific daemon
   */
  static getMessageTypesForDaemon(daemonType: string): readonly string[] {
    switch (daemonType) {
      case 'console': return this.CONSOLE_VALUES;
      case 'websocket': return this.WEBSOCKET_VALUES;
      case 'command': return this.COMMAND_VALUES;
      case 'widget': return this.WIDGET_VALUES;
      case 'session': return this.SESSION_VALUES;
      case 'health': return this.HEALTH_VALUES;
      case 'system': return this.SYSTEM_VALUES;
      default: return [];
    }
  }
}