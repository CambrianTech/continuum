/**
 * CommunicationProtocol - Shared types between server and browser client
 * 
 * This file defines the complete communication protocol that both the server
 * and browser client must adhere to. By sharing these types, we ensure:
 * 
 * 1. Type safety across the client-server boundary
 * 2. Zero drift between server expectations and client implementations
 * 3. Single source of truth for all communication protocols
 * 4. Automatic validation and serialization/deserialization
 * 5. Easy protocol evolution with version compatibility
 * 
 * Usage:
 * - Server: Import to validate incoming messages and structure responses
 * - Browser: Import to ensure message structure and handle responses
 * - Testing: Import to mock communication and validate protocols
 */

/**
 * Base message structure for all client-server communication
 */
export interface BaseMessage {
  type: string;
  timestamp: string;
  requestId?: string;
  sessionId?: string | null;
  clientId?: string | null;
  data?: unknown; // Optional data payload for message content
}

/**
 * Base response structure for all server responses
 */
export interface BaseResponse {
  success: boolean;
  timestamp: string;
  requestId?: string;
  sessionId?: string | null;
  data?: any;
  error?: string;
  metadata?: {
    executionTime?: number;
    version?: string;
    source?: string;
  };
}

/**
 * WebSocket connection message types
 * Shared between WebSocketDaemon (server) and BrowserWebSocketDaemon (client)
 */
// Import shared event enums - single source of truth!
import { WebSocketEvent } from './EventTypes';

export type WebSocketMessageType = WebSocketEvent;

/**
 * Client initialization message
 * Sent when browser establishes WebSocket connection
 */
export interface ClientInitMessage extends BaseMessage {
  type: 'client_init';
  data: {
    userAgent: string;
    url: string;
    version: string;
    capabilities?: string[];
    sessionType?: string;
    owner?: string;
  };
}

/**
 * Connection confirmation response
 * Sent by server after successful client initialization
 */
export interface ConnectionConfirmedResponse extends BaseResponse {
  data: {
    clientId: string;
    serverId: string;
    serverVersion: string;
    supportedFeatures: string[];
    heartbeatInterval: number;
  };
}

/**
 * Command execution message
 * Sent by client to execute server commands
 */
export interface ExecuteCommandMessage extends BaseMessage {
  type: 'execute_command';
  data: {
    command: string;
    params: any;
    requestId: string;
    timeout?: number;
    priority?: 'low' | 'normal' | 'high';
  };
}

/**
 * Command execution response
 * Sent by server with command results
 */
export interface CommandResponse extends BaseResponse {
  type: 'command_response';
  data: {
    command: string;
    result: any;
    executionTime: number;
    warnings?: string[];
  };
}

/**
 * Command execution error
 * Sent by server when command fails
 */
export interface CommandError extends BaseResponse {
  type: 'command_error';
  success: false;
  data: {
    command: string;
    errorType: 'validation' | 'execution' | 'timeout' | 'permission' | 'unknown';
    errorCode?: string;
    context?: any;
  };
}

/**
 * Session ready notification
 * Sent by server when session is established
 */
export interface SessionReadyMessage extends BaseMessage {
  type: 'session_ready';
  data: {
    sessionId: string;
    sessionType: string;
    owner: string;
    capabilities: string[];
    logs: {
      server: string;
      browser: string;
    };
    settings?: any;
  };
}

/**
 * Console log forwarding message
 * Sent by client to forward console logs
 */
export interface ConsoleLogMessage extends BaseMessage {
  type: 'console_log';
  data: {
    level: 'log' | 'info' | 'warn' | 'error' | 'debug';
    message: string;
    args: any[];
    stackTrace?: string;
    sourceLocation?: string;
    url: string;
    userAgent: string;
    viewport?: {
      width: number;
      height: number;
    };
  };
}

/**
 * Heartbeat message for connection monitoring
 */
export interface HeartbeatMessage extends BaseMessage {
  type: 'connection_heartbeat';
  data: {
    clientStatus: 'active' | 'idle' | 'background';
    clientTime: string;
    metrics?: {
      memoryUsage?: number;
      cpuUsage?: number;
      networkLatency?: number;
    };
  };
}

/**
 * System health status message
 */
export interface SystemHealthMessage extends BaseMessage {
  type: 'system_health';
  data: {
    status: 'healthy' | 'degraded' | 'failed';
    components: Array<{
      component: string;
      status: 'healthy' | 'degraded' | 'failed';
      lastCheck: number;
      details?: string;
      metrics?: any;
    }>;
    overall: string;
    recommendations?: string[];
  };
}

/**
 * Connection state types
 */
export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'error' | 'reconnecting';

/**
 * Session types
 */
export type SessionType = 'development' | 'production' | 'testing' | 'persona' | 'temporary';

/**
 * Message validation helpers
 */
export class ProtocolValidator {
  /**
   * Validate that a message conforms to the base message structure
   */
  static isValidBaseMessage(obj: any): obj is BaseMessage {
    return (
      typeof obj === 'object' &&
      obj !== null &&
      typeof obj.type === 'string' &&
      typeof obj.timestamp === 'string'
    );
  }

  /**
   * Validate that a response conforms to the base response structure
   */
  static isValidBaseResponse(obj: any): obj is BaseResponse {
    return (
      typeof obj === 'object' &&
      obj !== null &&
      typeof obj.success === 'boolean' &&
      typeof obj.timestamp === 'string'
    );
  }

  /**
   * Validate WebSocket message type
   */
  static isValidWebSocketMessageType(type: string): type is WebSocketMessageType {
    // Use the shared enum values - single source of truth!
    const validTypes: WebSocketMessageType[] = Object.values(WebSocketEvent);
    return validTypes.includes(type as WebSocketMessageType);
  }

  /**
   * Type guard for client init message
   */
  static isClientInitMessage(obj: any): obj is ClientInitMessage {
    return (
      this.isValidBaseMessage(obj) &&
      obj.type === 'client_init' &&
      typeof obj.data === 'object' &&
      obj.data !== null &&
      typeof (obj.data as any).userAgent === 'string' &&
      typeof (obj.data as any).url === 'string' &&
      typeof (obj.data as any).version === 'string'
    );
  }

  /**
   * Type guard for execute command message
   */
  static isExecuteCommandMessage(obj: any): obj is ExecuteCommandMessage {
    return (
      this.isValidBaseMessage(obj) &&
      obj.type === 'execute_command' &&
      typeof obj.data === 'object' &&
      obj.data !== null &&
      typeof (obj.data as any).command === 'string' &&
      typeof (obj.data as any).requestId === 'string'
    );
  }

  /**
   * Type guard for console log message
   */
  static isConsoleLogMessage(obj: any): obj is ConsoleLogMessage {
    return (
      this.isValidBaseMessage(obj) &&
      obj.type === 'console_log' &&
      typeof obj.data === 'object' &&
      obj.data !== null &&
      typeof (obj.data as any).level === 'string' &&
      typeof (obj.data as any).message === 'string' &&
      Array.isArray((obj.data as any).args)
    );
  }

  /**
   * Create a standardized error response
   */
  static createErrorResponse(
    error: string, 
    requestId?: string, 
    errorType: string = 'unknown',
    context?: any
  ): BaseResponse {
    const response: BaseResponse = {
      success: false,
      timestamp: new Date().toISOString(),
      error,
      data: {
        errorType,
        context
      }
    };
    
    // Only assign requestId if it's defined (exactOptionalPropertyTypes compatibility)
    if (requestId !== undefined) {
      response.requestId = requestId;
    }
    
    return response;
  }

  /**
   * Create a standardized success response
   */
  static createSuccessResponse(
    data: any, 
    requestId?: string, 
    metadata?: any
  ): BaseResponse {
    const response: BaseResponse = {
      success: true,
      timestamp: new Date().toISOString(),
      data
    };
    
    // Only assign optional properties if they're defined (exactOptionalPropertyTypes compatibility)
    if (requestId !== undefined) {
      response.requestId = requestId;
    }
    if (metadata !== undefined) {
      response.metadata = metadata;
    }
    
    return response;
  }
}

/**
 * Protocol version information
 */
export const PROTOCOL_VERSION = {
  major: 1,
  minor: 0,
  patch: 0,
  string: '1.0.0'
} as const;

/**
 * Default configuration values
 */
export const PROTOCOL_DEFAULTS = {
  heartbeatInterval: 30000, // 30 seconds
  commandTimeout: 30000,    // 30 seconds
  reconnectDelay: 1000,     // 1 second base delay
  maxReconnectAttempts: 5,
  maxMessageSize: 1024 * 1024, // 1MB
  compressionThreshold: 1024,   // Compress messages > 1KB
} as const;

// All types are already exported above where they're defined.
// No need for duplicate export statements - they cause TS2484 conflicts.