/**
 * IPCProtocol - Generic IPC Message Types
 *
 * Base message types for inter-process communication.
 * Used by LoggerDaemon, cognition layer (persona/tools), and other multiprocess components.
 *
 * Each specialized daemon extends these base types with their own message payloads.
 */

/**
 * Base message sent from parent → child process
 */
export interface IPCMessage {
  /** Message type (command to execute) */
  readonly type: string;

  /** Unique message ID for correlation */
  readonly messageId?: string;

  /** Timestamp when message was sent */
  readonly timestamp?: string;

  /** Arbitrary payload data (specialized by daemon type) */
  readonly data?: unknown;
}

/**
 * Base response sent from child → parent process
 */
export interface IPCResponse {
  /** Response type (usually echoes request type + '-response') */
  readonly type: string;

  /** Correlation ID matching the original request */
  readonly messageId?: string;

  /** Timestamp when response was sent */
  readonly timestamp?: string;

  /** Whether the operation succeeded */
  readonly success: boolean;

  /** Error message if success = false */
  readonly error?: string;

  /** Response payload data (specialized by daemon type) */
  readonly data?: unknown;
}

/**
 * Standard system messages (all processes support these)
 */
export type SystemMessageType =
  | 'health-check'      // Parent checking if child is alive
  | 'health-response'   // Child responding to health check
  | 'shutdown'          // Parent requesting graceful shutdown
  | 'shutdown-ack'      // Child acknowledging shutdown
  | 'ping'              // Simple connectivity test
  | 'pong';             // Response to ping

/**
 * Health check message
 */
export interface HealthCheckMessage extends IPCMessage {
  readonly type: 'health-check';
}

/**
 * Health check response
 */
export interface HealthCheckResponse extends IPCResponse {
  readonly type: 'health-response';
  readonly success: true;
  readonly data: {
    readonly uptime: number;        // Milliseconds since start
    readonly memoryUsage: number;   // Bytes
    readonly queueSize?: number;    // If process has a queue
  };
}

/**
 * Shutdown message
 */
export interface ShutdownMessage extends IPCMessage {
  readonly type: 'shutdown';
  readonly data?: {
    readonly gracePeriodMs?: number; // How long to wait before forcing kill
  };
}

/**
 * Shutdown acknowledgment
 */
export interface ShutdownAckResponse extends IPCResponse {
  readonly type: 'shutdown-ack';
  readonly success: true;
}

/**
 * Ping message (connectivity test)
 */
export interface PingMessage extends IPCMessage {
  readonly type: 'ping';
}

/**
 * Pong response
 */
export interface PongResponse extends IPCResponse {
  readonly type: 'pong';
  readonly success: true;
  readonly data: {
    readonly timestamp: string;
  };
}

/**
 * Helper to create correlation IDs
 */
export function generateMessageId(): string {
  return `msg-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/**
 * Helper to create timestamp
 */
export function generateTimestamp(): string {
  return new Date().toISOString();
}

/**
 * Create a standard IPC message
 */
export function createIPCMessage(type: string, data?: unknown): IPCMessage {
  return {
    type,
    messageId: generateMessageId(),
    timestamp: generateTimestamp(),
    data
  };
}

/**
 * Create a standard IPC response
 */
export function createIPCResponse(
  type: string,
  success: boolean,
  data?: unknown,
  error?: string,
  messageId?: string
): IPCResponse {
  return {
    type,
    messageId,
    timestamp: generateTimestamp(),
    success,
    error,
    data
  };
}

/**
 * Create a success response
 */
export function createSuccessResponse(
  type: string,
  data?: unknown,
  messageId?: string
): IPCResponse {
  return createIPCResponse(type, true, data, undefined, messageId);
}

/**
 * Create an error response
 */
export function createErrorResponse(
  type: string,
  error: string,
  messageId?: string
): IPCResponse {
  return createIPCResponse(type, false, undefined, error, messageId);
}

/**
 * Type guard for IPC messages
 */
export function isIPCMessage(obj: unknown): obj is IPCMessage {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'type' in obj &&
    typeof (obj as IPCMessage).type === 'string'
  );
}

/**
 * Type guard for IPC responses
 */
export function isIPCResponse(obj: unknown): obj is IPCResponse {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'type' in obj &&
    'success' in obj &&
    typeof (obj as IPCResponse).type === 'string' &&
    typeof (obj as IPCResponse).success === 'boolean'
  );
}

/**
 * Type guard for system messages
 */
export function isSystemMessage(message: IPCMessage): boolean {
  const systemTypes: SystemMessageType[] = [
    'health-check',
    'health-response',
    'shutdown',
    'shutdown-ack',
    'ping',
    'pong'
  ];
  return systemTypes.includes(message.type as SystemMessageType);
}
