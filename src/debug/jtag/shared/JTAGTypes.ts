/**
 * Emergency JTAG - Shared Types
 * 
 * Common interfaces and types used across browser and server contexts.
 * Defines the standard JTAG packet format for all communications.
 */

/**
 * Standard JTAG Message Format
 * Universal container for all JTAG communications
 */
export interface JTAGMessage<T = any> {
  // Packet Metadata
  id: string;                           // Unique packet identifier
  type: JTAGMessageType;               // Message type (determines payload structure)
  version: string;                     // Protocol version (e.g., "1.0.0")
  timestamp: string;                   // ISO timestamp when packet was created
  
  // Routing Information  
  source: JTAGContext;                 // Where packet originated
  target?: JTAGContext;                // Optional specific target
  route?: string[];                    // Routing path history
  
  // Core Data
  payload: T;                          // Type-safe payload data
  
  // Promise/Event Handling
  communication?: {
    type: 'request' | 'response' | 'event' | 'broadcast';
    requestId?: string;                // For linking request/response pairs
    responseRequired?: boolean;        // Expects a response packet
    eventName?: string;                // For event-based communication
    resolvePromise?: boolean;          // Should resolve a pending promise
    rejectPromise?: boolean;           // Should reject a pending promise
  };
  
  // Transport Metadata
  transport?: {
    type: JTAGTransportType;           // How packet was sent
    priority: number;                  // Routing priority (0-10)
    retries?: number;                  // Retry attempts
    timeout?: number;                  // Timeout in milliseconds
    expectsResponse?: boolean;         // Transport should wait for response
  };
  
  // Optional Fields
  correlationId?: string;              // Link related packets
  parentId?: string;                   // Parent packet for responses
  metadata?: Record<string, any>;      // Custom metadata
}

/**
 * Standard Message Types - Strongly Typed
 */
export const JTAG_MESSAGE_TYPES = {
  LOG: 'log',
  SCREENSHOT: 'screenshot', 
  EXEC: 'exec',
  HEALTH: 'health',
  CONNECT: 'connect',
  RESPONSE: 'response',
  ERROR: 'error',
  PING: 'ping',
  PONG: 'pong',
  STATIC_FILE: 'static-file'
} as const;

export type JTAGMessageType = typeof JTAG_MESSAGE_TYPES[keyof typeof JTAG_MESSAGE_TYPES];

/**
 * Execution Contexts
 */
export const JTAG_CONTEXTS = {
  BROWSER: 'browser',
  SERVER: 'server', 
  EXTERNAL: 'external',
  DAEMON: 'daemon',
  MCP: 'mcp'
} as const;

export type JTAGContext = typeof JTAG_CONTEXTS[keyof typeof JTAG_CONTEXTS];

/**
 * Typed Payload Interfaces for Each Packet Type
 */

// Log Packet Payload
export interface JTAGLogPayload {
  level: 'log' | 'warn' | 'error' | 'critical' | 'trace' | 'probe' | 'test';
  message: string;
  component: string;
  data?: any;
  stackTrace?: string;
}

// Screenshot Packet Payload  
export interface JTAGScreenshotPayload {
  filename: string;
  selector?: string;
  options?: JTAGScreenshotOptions;
  dataUrl?: string;     // Base64 image data
  filepath?: string;    // Server file path
  urgent?: boolean;     // Priority flag
}

// Exec Packet Payload
export interface JTAGExecPayload {
  code: string;
  options?: JTAGExecOptions;
  result?: any;         // For response packets
  executionTime?: number;
}

// Health Packet Payload
export interface JTAGHealthPayload {
  status: 'ok' | 'warning' | 'error';
  uptime?: number;
  memory?: number;
  connections?: number;
  details?: Record<string, any>;
}

// Connect Packet Payload
export interface JTAGConnectPayload {
  clientId: string;
  clientType: JTAGContext;
  capabilities?: string[];
  version?: string;
}

// Response Packet Payload
export interface JTAGResponsePayload<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  duration?: number;
}

// Error Packet Payload
export interface JTAGErrorPayload {
  code: string;
  message: string;
  details?: any;
  stackTrace?: string;
}

// Ping/Pong Packet Payload
export interface JTAGPingPayload {
  timestamp: number;
  sequence?: number;
}

/**
 * Strongly Typed Message Interfaces
 */
export type JTAGLogMessage = JTAGMessage<JTAGLogPayload>;
export type JTAGScreenshotMessage = JTAGMessage<JTAGScreenshotPayload>;
export type JTAGExecMessage = JTAGMessage<JTAGExecPayload>;
export type JTAGHealthMessage = JTAGMessage<JTAGHealthPayload>;
export type JTAGConnectMessage = JTAGMessage<JTAGConnectPayload>;
export type JTAGResponseMessage<T = any> = JTAGMessage<JTAGResponsePayload<T>>;
export type JTAGErrorMessage = JTAGMessage<JTAGErrorPayload>;
export type JTAGPingMessage = JTAGMessage<JTAGPingPayload>;
export type JTAGPongMessage = JTAGMessage<JTAGPingPayload>;

/**
 * Union Type for All Message Types
 */
export type JTAGUniversalMessage = 
  | JTAGLogMessage
  | JTAGScreenshotMessage
  | JTAGExecMessage
  | JTAGHealthMessage
  | JTAGConnectMessage
  | JTAGResponseMessage
  | JTAGErrorMessage
  | JTAGPingMessage
  | JTAGPongMessage;

/**
 * JTAG Message Factory
 * Creates properly formatted messages with promise/event support
 */
export class JTAGMessageFactory {
  private static version = '1.0.0';

  static createRequest<T>(
    type: JTAGMessageType,
    source: JTAGContext,
    payload: T,
    options?: {
      target?: JTAGContext;
      timeout?: number;
      priority?: number;
      expectsResponse?: boolean;
    }
  ): JTAGMessage<T> {
    const id = this.generateId();
    return {
      id,
      type,
      version: this.version,
      timestamp: new Date().toISOString(),
      source,
      target: options?.target,
      payload,
      communication: {
        type: 'request',
        requestId: id,
        responseRequired: options?.expectsResponse ?? true
      },
      transport: {
        type: 'websocket',
        priority: options?.priority ?? 5,
        timeout: options?.timeout ?? 30000,
        expectsResponse: options?.expectsResponse ?? true
      }
    };
  }

  static createResponse<T>(
    originalRequest: JTAGMessage,
    payload: T,
    success: boolean = true
  ): JTAGResponseMessage<T> {
    return {
      id: this.generateId(),
      type: 'response',
      version: this.version,
      timestamp: new Date().toISOString(),
      source: originalRequest.target || 'server',
      target: originalRequest.source,
      payload: {
        success,
        data: payload
      } as JTAGResponsePayload<T>,
      communication: {
        type: 'response',
        requestId: originalRequest.communication?.requestId || originalRequest.id,
        resolvePromise: success,
        rejectPromise: !success
      },
      parentId: originalRequest.id,
      correlationId: originalRequest.correlationId
    };
  }

  static createEvent<T>(
    eventName: string,
    source: JTAGContext,
    payload: T,
    options?: {
      target?: JTAGContext;
      priority?: number;
    }
  ): JTAGMessage<T> {
    return {
      id: this.generateId(),
      type: 'log', // Events typically use log type
      version: this.version,
      timestamp: new Date().toISOString(),
      source,
      target: options?.target,
      payload,
      communication: {
        type: 'event',
        eventName
      },
      transport: {
        type: 'websocket',
        priority: options?.priority ?? 3
      }
    };
  }

  static createBroadcast<T>(
    type: JTAGMessageType,
    source: JTAGContext,
    payload: T
  ): JTAGMessage<T> {
    return {
      id: this.generateId(),
      type,
      version: this.version,
      timestamp: new Date().toISOString(),
      source,
      payload,
      communication: {
        type: 'broadcast'
      },
      transport: {
        type: 'websocket',
        priority: 1
      }
    };
  }

  private static generateId(): string {
    return `jtag_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
  }
}

export interface JTAGLogEntry {
  timestamp: string;
  context: 'browser' | 'server';
  component: string;
  message: string;
  data?: any;
  type: 'log' | 'warn' | 'error' | 'critical' | 'trace' | 'probe' | 'test';
  correlationId?: string;
}

export interface JTAGConfig {
  context: 'browser' | 'server';
  jtagPort: number;
  logDirectory?: string;
  enableRemoteLogging: boolean;
  enableConsoleOutput: boolean;
  maxBufferSize: number;
  transport?: JTAGTransportConfig;
}

export interface JTAGTransportConfig {
  type: 'websocket' | 'http' | 'continuum-ws' | 'custom';
  fallback?: 'http' | 'queue';
  customTransport?: JTAGTransport;
  retryAttempts?: number;
  retryDelay?: number;
}

// Connection API Types
export interface ContinuumConnectionParams {
  healthCheck?: boolean;        // default: true
  timeout?: number;            // default: 10000 (10s)
  retryAttempts?: number;      // default: 3
  pingInterval?: number;       // default: 30000 (30s)
  transport?: 'auto' | 'websocket' | 'rest' | 'mcp' | 'polling' | 'sse';  // default: 'auto'
}

export interface ContinuumConnection {
  healthy: boolean;
  transport: {
    type: 'websocket' | 'rest' | 'mcp' | 'polling' | 'sse';
    state: 'connected' | 'connecting' | 'disconnected' | 'error';
    endpoint: string;
    latency: number;
  };
  session: {
    id: string;
    uuid: string;
    uptime: number;
  };
}

// Default connection parameters
export const DEFAULT_CONNECTION_PARAMS: Required<ContinuumConnectionParams> = {
  healthCheck: true,
  timeout: 10000,
  retryAttempts: 3,
  pingInterval: 30000,
  transport: 'auto'
};

export interface JTAGServerResponse {
  success: boolean;
  error?: string;
  timestamp: string;
}

export interface JTAGStats {
  totalEntries: number;
  entriesByType: Record<string, number>;
  entriesByComponent: Record<string, number>;
  oldestEntry?: string;
  newestEntry?: string;
}

export interface JTAGScreenshotOptions {
  selector?: string;
  width?: number;
  height?: number;
  fullPage?: boolean;
  quality?: number;
  format?: 'png' | 'jpeg' | 'webp';
  delay?: number;
}

export interface JTAGScreenshotResult {
  success: boolean;
  filepath: string;
  filename: string;
  context: 'browser' | 'server';
  timestamp: string;
  options?: JTAGScreenshotOptions;
  error?: string;
  metadata?: {
    width: number;
    height: number;
    size: number;
    selector?: string;
  };
}

export interface JTAGExecOptions {
  timeout?: number;
  context?: 'browser' | 'server';
  returnValue?: boolean;
  uuid?: string;
}

export interface JTAGExecResult {
  success: boolean;
  result?: any;
  error?: string;
  context: 'browser' | 'server';
  timestamp: string;
  executionTime: number;
  uuid: string;
}

export interface JTAGUUIDInfo {
  uuid: string;
  context: 'browser' | 'server';
  timestamp: string;
  sessionId?: string;
  processId?: number;
  metadata?: Record<string, any>;
}

export const DEFAULT_JTAG_CONFIG: Partial<JTAGConfig> = {
  jtagPort: 9001,
  enableRemoteLogging: true,
  enableConsoleOutput: true,
  maxBufferSize: 1000
};

export const JTAG_LOG_LEVELS = {
  LOG: 'log',
  WARN: 'warn',
  ERROR: 'error', 
  CRITICAL: 'critical',
  TRACE: 'trace', 
  PROBE: 'probe',
  TEST: 'test'
} as const;

export type JTAGLogLevel = typeof JTAG_LOG_LEVELS[keyof typeof JTAG_LOG_LEVELS];

/**
 * JTAG Status Event System
 * Transport-agnostic connection lifecycle events
 */
export const JTAG_STATUS = {
  CONNECTING: 'connecting',
  READY: 'ready',
  DISCONNECTED: 'disconnected', 
  TERMINATED: 'terminated',
  ERROR: 'error'
} as const;

export type JTAGStatus = typeof JTAG_STATUS[keyof typeof JTAG_STATUS];

/**
 * Transport Types
 * Supports any connection mechanism
 */
export const JTAG_TRANSPORT = {
  WEBSOCKET: 'websocket',
  HTTP: 'http',
  REST: 'rest', 
  MCP: 'mcp',
  CONTINUUM_WS: 'continuum-ws',
  POLLING: 'polling',
  SSE: 'sse', // Server-Sent Events
  CUSTOM: 'custom'
} as const;

export type JTAGTransportType = typeof JTAG_TRANSPORT[keyof typeof JTAG_TRANSPORT];

/**
 * Transport-Agnostic Connection State
 */
export interface JTAGConnectionState {
  connected: boolean;
  connectionId?: string;
  endpoint?: string;
  protocol?: string;
  lastActivity?: number;
  metadata?: Record<string, any>;
}

/**
 * Universal Status Event
 * Works with any transport: WebSocket, REST, MCP, HTTP polling, etc.
 */
export interface JTAGStatusEvent {
  status: JTAGStatus;
  uuid: string;
  sessionId: string;
  timestamp: number;
  context: 'browser' | 'server';
  transport: {
    type: JTAGTransportType;
    state?: JTAGConnectionState;
    details?: {
      error?: string;
      httpStatus?: number;
      wsState?: number;
      mcpMethod?: string;
      restEndpoint?: string;
      pollingInterval?: number;
      retryCount?: number;
      reason?: string;
    };
  };
}

export interface JTAGStatusEventListener {
  (event: JTAGStatusEvent): void;
}

/**
 * Legacy WebSocket message interface - now uses universal JTAG message format
 * CRITICAL: These messages cross client-server boundary via WebSocket
 */
export type JTAGWebSocketMessage = JTAGUniversalMessage;

/**
 * WebSocket response interface - now uses universal response message
 */
export type JTAGWebSocketResponse = JTAGResponseMessage;

/**
 * Transport Layer Abstraction
 * Allows JTAG to use different network transports (WebSocket, HTTP, Continuum WS, custom)
 */
export interface JTAGTransport {
  name: string;
  initialize(config: JTAGConfig): Promise<boolean>;
  send<T>(message: JTAGMessage<T>): Promise<JTAGTransportResponse<T>>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  onMessage?(handler: (message: JTAGUniversalMessage) => void): void;
  onDisconnect?(handler: () => void): void;
  
  // Status Event Integration
  onStatusChange?(handler: (status: JTAGStatus, details?: any) => void): void;
  getTransportType(): JTAGTransportType;
  getConnectionState?(): JTAGConnectionState;
}

export interface JTAGTransportResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  timestamp: string;
  messageId?: string;
  transportMeta?: {
    transport: string;
    duration: number;
    retries: number;
  };
}

/**
 * Transport Factory for automatic transport selection and fallback
 */
export interface JTAGTransportFactory {
  createTransport(config: JTAGTransportConfig): JTAGTransport;
  detectHostTransport(): JTAGTransport | null; // Auto-detect Continuum WS or other host transports
  getAvailableTransports(): string[];
}

/**
 * Built-in Transport Types
 */
export interface JTAGWebSocketTransport extends JTAGTransport {
  websocket: WebSocket | any;
  reconnect(): Promise<boolean>;
}

export interface JTAGHTTPTransport extends JTAGTransport {
  baseUrl: string;
  poll(): void;
  stopPolling(): void;
}

export interface JTAGContinuumTransport extends JTAGTransport {
  daemonConnector: any; // Reference to Continuum's daemon system
  routeViaDaemon<T>(message: JTAGMessage<T>): Promise<JTAGTransportResponse<T>>;
}

/**
 * Message Queue for offline/fallback scenarios
 */
export interface JTAGMessageQueue {
  enqueue(message: JTAGUniversalMessage): void;
  dequeue(): JTAGUniversalMessage | null;
  flush(transport: JTAGTransport): Promise<JTAGTransportResponse[]>;
  size(): number;
  clear(): void;
}

