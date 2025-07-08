/**
 * WebSocketCommunication - Descriptive types for WebSocket client-server communication
 * 
 * MIGRATION STRATEGY: These types are being introduced alongside existing types.
 * We'll gradually migrate to these descriptive names without breaking existing code.
 * 
 * DO NOT import these into existing files yet - they're for future use.
 */

/**
 * Browser WebSocket Connection Establishment
 */
export interface BrowserWebSocketConnectionRequest {
  browserUserAgent: string;
  browserCurrentUrl: string;
  browserVersion: string;
  supportedCapabilities: Array<
    | 'console_log_forwarding'
    | 'command_execution'
    | 'health_monitoring'
    | 'widget_interaction'
    | 'file_operations'
  >;
  requestedSessionType?: 'development' | 'production' | 'testing' | 'persona';
  requestedSessionOwner?: string;
}

export interface ServerWebSocketConnectionConfirmation {
  assignedClientId: string;
  serverInstanceId: string;
  serverVersion: string;
  enabledFeatures: string[];
  heartbeatIntervalMs: number;
  maxMessageSizeBytes: number;
  connectionEstablishedAt: string;
}

/**
 * Command Execution via WebSocket
 */
export interface BrowserToServerCommandExecution {
  commandToExecute: string;
  commandParameters: Record<string, unknown>;
  commandRequestId: string;
  commandTimeoutMs?: number;
  commandPriority: 'background' | 'normal' | 'urgent';
  executionContext: {
    sessionId: string;
    userId?: string;
    browserTab: string;
  };
}

export interface ServerToBrowserCommandResult {
  originalCommandName: string;
  originalRequestId: string;
  executionSuccessful: boolean;
  commandResult?: unknown;
  commandError?: {
    errorType: 'validation' | 'execution' | 'timeout' | 'permission' | 'unknown';
    errorMessage: string;
    errorCode?: string;
    errorContext?: Record<string, unknown>;
  };
  executionTimeMs: number;
  executionWarnings?: string[];
}

/**
 * Console Log Forwarding from Browser
 */
export interface BrowserConsoleLogForwarding {
  consoleLogLevel: 'debug' | 'log' | 'info' | 'warn' | 'error';
  consoleMessage: string;
  consoleArguments: Array<{
    argumentType: 'string' | 'number' | 'boolean' | 'object' | 'function' | 'undefined' | 'null';
    argumentValue: string; // Serialized representation
    originalValue?: unknown; // For simple types
  }>;
  browserContext: {
    sourceFileName?: string;
    sourceLineNumber?: number;
    sourceColumnNumber?: number;
    functionName?: string;
    stackTrace: string;
    currentUrl: string;
    userAgent: string;
    viewportWidth: number;
    viewportHeight: number;
    timestamp: string;
  };
}

/**
 * Session Management via WebSocket
 */
export interface ServerSessionReadyNotification {
  establishedSessionId: string;
  sessionType: 'development' | 'production' | 'testing' | 'persona';
  sessionOwner: string;
  availableCapabilities: string[];
  sessionLogPaths: {
    serverLogFile: string;
    browserLogFile: string;
  };
  sessionConfiguration: Record<string, unknown>;
  sessionCreatedAt: string;
}

export interface BrowserSessionStateUpdate {
  sessionId: string;
  sessionStateChanges: Record<string, unknown>;
  updateTriggeredBy: 'user_interaction' | 'widget_action' | 'system_event';
  updateTimestamp: string;
}

/**
 * Health Monitoring via WebSocket
 */
export interface BrowserHealthStatusReport {
  browserHealthStatus: 'optimal' | 'good' | 'degraded' | 'poor';
  browserPerformanceMetrics: {
    memoryUsageBytes: number;
    cpuUsagePercent?: number;
    networkLatencyMs?: number;
    renderingFpsAverage?: number;
  };
  activeBrowserFeatures: string[];
  detectedIssues?: Array<{
    issueType: string;
    issueDescription: string;
    issueSeverity: 'low' | 'medium' | 'high' | 'critical';
  }>;
  healthCheckTimestamp: string;
}

export interface ServerSystemHealthBroadcast {
  overallSystemHealth: 'healthy' | 'degraded' | 'critical' | 'maintenance';
  affectedComponents: string[];
  healthIssueDescriptions?: string[];
  estimatedResolutionTime?: string;
  userActionRequired?: boolean;
  healthUpdateTimestamp: string;
}

/**
 * WebSocket Connection Monitoring
 */
export interface WebSocketHeartbeatPing {
  heartbeatSequenceNumber: number;
  clientConnectionStatus: 'active' | 'idle' | 'background' | 'focused';
  clientTimestamp: string;
  clientMetrics?: {
    messagesSent: number;
    messagesReceived: number;
    connectionUptimeMs: number;
  };
}

export interface WebSocketHeartbeatPong {
  heartbeatSequenceNumber: number;
  serverTimestamp: string;
  roundTripLatencyMs: number;
  serverConnectionQuality: 'excellent' | 'good' | 'fair' | 'poor';
  serverRecommendations?: string[];
}

/**
 * Error and Disconnection Handling
 */
export interface WebSocketConnectionError {
  errorType: 'network' | 'protocol' | 'authentication' | 'server_overload' | 'client_error';
  errorMessage: string;
  errorCode?: string;
  errorOccurredAt: string;
  suggestedRecoveryAction?: 'retry_immediately' | 'retry_with_delay' | 'refresh_page' | 'contact_support';
  additionalErrorContext?: Record<string, unknown>;
}

export interface WebSocketDisconnectionNotice {
  disconnectionReason: 'client_requested' | 'server_shutdown' | 'network_failure' | 'idle_timeout' | 'error';
  disconnectionMessage?: string;
  connectionDurationMs: number;
  finalStatistics: {
    totalMessagesSent: number;
    totalMessagesReceived: number;
    averageLatencyMs: number;
  };
  reconnectionAdvice?: string;
}

/**
 * Message Type Discriminators
 */
export type WebSocketClientToServerMessage =
  | { type: 'browser_connection_request'; data: BrowserWebSocketConnectionRequest }
  | { type: 'command_execution_request'; data: BrowserToServerCommandExecution }
  | { type: 'console_log_forwarding'; data: BrowserConsoleLogForwarding }
  | { type: 'session_state_update'; data: BrowserSessionStateUpdate }
  | { type: 'browser_health_report'; data: BrowserHealthStatusReport }
  | { type: 'heartbeat_ping'; data: WebSocketHeartbeatPing };

export type WebSocketServerToClientMessage =
  | { type: 'connection_confirmation'; data: ServerWebSocketConnectionConfirmation }
  | { type: 'command_execution_result'; data: ServerToBrowserCommandResult }
  | { type: 'session_ready_notification'; data: ServerSessionReadyNotification }
  | { type: 'system_health_broadcast'; data: ServerSystemHealthBroadcast }
  | { type: 'heartbeat_pong'; data: WebSocketHeartbeatPong }
  | { type: 'connection_error'; data: WebSocketConnectionError }
  | { type: 'disconnection_notice'; data: WebSocketDisconnectionNotice };

/**
 * Combined WebSocket Message Types
 */
export type WebSocketMessage = 
  | WebSocketClientToServerMessage 
  | WebSocketServerToClientMessage;

/**
 * Type Guards for Runtime Validation
 */
export function isBrowserConnectionRequest(msg: any): msg is { type: 'browser_connection_request'; data: BrowserWebSocketConnectionRequest } {
  return msg?.type === 'browser_connection_request' && 
         typeof msg?.data?.browserUserAgent === 'string' &&
         typeof msg?.data?.browserCurrentUrl === 'string';
}

export function isCommandExecutionRequest(msg: any): msg is { type: 'command_execution_request'; data: BrowserToServerCommandExecution } {
  return msg?.type === 'command_execution_request' &&
         typeof msg?.data?.commandToExecute === 'string' &&
         typeof msg?.data?.commandRequestId === 'string';
}

export function isConsoleLogForwarding(msg: any): msg is { type: 'console_log_forwarding'; data: BrowserConsoleLogForwarding } {
  return msg?.type === 'console_log_forwarding' &&
         typeof msg?.data?.consoleMessage === 'string' &&
         typeof msg?.data?.consoleLogLevel === 'string';
}

/**
 * MIGRATION ALIASES - For gradual transition from existing types
 * These allow existing code to keep working while we migrate
 */

// Alias existing types to new descriptive names for future migration
export type ClientInitMessage = BrowserWebSocketConnectionRequest;
export type ConnectionConfirmedResponse = ServerWebSocketConnectionConfirmation;
export type ExecuteCommandMessage = BrowserToServerCommandExecution;
export type CommandResponse = ServerToBrowserCommandResult;
export type ConsoleLogMessage = BrowserConsoleLogForwarding;
export type SessionReadyMessage = ServerSessionReadyNotification;
export type HeartbeatMessage = WebSocketHeartbeatPing;

// Keep existing CommunicationProtocol.ts working - import this file's types there eventually