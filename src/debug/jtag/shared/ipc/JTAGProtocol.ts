/**
 * JTAGProtocol - Universal Packet Format for JTAG System
 *
 * This is THE universal packet format used everywhere in JTAG:
 * - Commands (Commands.execute)
 * - Events (Events.emit/subscribe)
 * - Worker IPC (Rust ↔ TypeScript)
 * - Daemon communication
 *
 * DESIGN PRINCIPLE:
 * Rust workers should feel IDENTICAL to TypeScript workers.
 * Promises and events flow the same way regardless of implementation language.
 *
 * MIRRORED IN RUST: workers/shared/jtag_protocol.rs
 * Keep these two files in sync!
 */

// ============================================================================
// Base Message Types (Generic Envelope)
// ============================================================================

/**
 * Base request message - sent TO a worker/daemon/command handler.
 * Generic over payload type T for type safety.
 */
export interface JTAGRequest<T = unknown> {
  /** Unique identifier for this request (UUID v4) */
  id: string;

  /** Message type identifier (e.g., 'write-log', 'ping', 'command:execute') */
  type: string;

  /** ISO 8601 timestamp when request was created */
  timestamp: string;

  /** The actual request data (type-safe via generics) */
  payload: T;

  /** Optional user context (for auth/logging) */
  userId?: string;

  /** Optional session context (for state management) */
  sessionId?: string;
}

/**
 * Base response message - sent FROM a worker/daemon/command handler.
 * Generic over payload type T for type safety.
 */
export interface JTAGResponse<T = unknown> {
  /** Unique identifier for this response (UUID v4) */
  id: string;

  /** Message type identifier (matches request type) */
  type: string;

  /** ISO 8601 timestamp when response was created */
  timestamp: string;

  /** The actual response data (type-safe via generics) */
  payload: T;

  /** Links back to the request that triggered this response */
  requestId: string;

  /** Whether the operation succeeded */
  success: boolean;

  /** Error message if success=false */
  error?: string;

  /** Error category for programmatic handling */
  errorType?: JTAGErrorType;

  /** Stack trace for debugging (only in development) */
  stack?: string;
}

/**
 * Standard error types for JTAG operations.
 * Consistent across all workers/commands/events.
 */
export type JTAGErrorType =
  | 'validation'    // Invalid input data
  | 'timeout'       // Operation timed out
  | 'internal'      // Internal error (bug or unexpected state)
  | 'notFound'      // Resource not found
  | 'unauthorized'  // Permission denied
  | 'unavailable';  // Service unavailable (worker down, etc.)

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create a success response from a request.
 */
export function createSuccessResponse<T>(
  request: JTAGRequest<unknown>,
  payload: T
): JTAGResponse<T> {
  return {
    id: generateId(),
    type: request.type,
    timestamp: new Date().toISOString(),
    payload,
    requestId: request.id,
    success: true,
  };
}

/**
 * Create an error response from a request.
 */
export function createErrorResponse<T>(
  request: JTAGRequest<unknown>,
  error: string,
  errorType: JTAGErrorType,
  payload: T
): JTAGResponse<T> {
  return {
    id: generateId(),
    type: request.type,
    timestamp: new Date().toISOString(),
    payload,
    requestId: request.id,
    success: false,
    error,
    errorType,
  };
}

/**
 * Generate a unique ID (UUID v4).
 * Uses crypto.randomUUID() in Node.js.
 */
function generateId(): string {
  return crypto.randomUUID();
}

// ============================================================================
// Type Guards
// ============================================================================

/**
 * Check if a message is a valid JTAG request.
 */
export function isJTAGRequest(msg: unknown): msg is JTAGRequest {
  if (typeof msg !== 'object' || msg === null) return false;
  const req = msg as Partial<JTAGRequest>;
  return (
    typeof req.id === 'string' &&
    typeof req.type === 'string' &&
    typeof req.timestamp === 'string' &&
    'payload' in req
  );
}

/**
 * Check if a message is a valid JTAG response.
 */
export function isJTAGResponse(msg: unknown): msg is JTAGResponse {
  if (typeof msg !== 'object' || msg === null) return false;
  const res = msg as Partial<JTAGResponse>;
  return (
    typeof res.id === 'string' &&
    typeof res.type === 'string' &&
    typeof res.timestamp === 'string' &&
    'payload' in res &&
    typeof res.requestId === 'string' &&
    typeof res.success === 'boolean'
  );
}

// ============================================================================
// Legacy Aliases (for backwards compatibility)
// ============================================================================

/**
 * @deprecated Use JTAGRequest instead
 */
export type WorkerRequest<T> = JTAGRequest<T>;

/**
 * @deprecated Use JTAGResponse instead
 */
export type WorkerResponse<T> = JTAGResponse<T>;
