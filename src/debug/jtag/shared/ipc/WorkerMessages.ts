/**
 * Worker IPC Protocol - Generic Message Transport
 *
 * This is the SOURCE OF TRUTH for IPC message format between TypeScript daemons
 * and Rust workers. Keep in sync with workers/shared/src/messages.rs
 *
 * CRITICAL: Any changes here must be reflected in Rust types using serde
 *
 * DESIGN PRINCIPLE: This is a GENERIC transport layer (like JTAGPayload).
 * It does NOT know about specific worker message types (logging, cognition, LoRA).
 * Workers own their own message schemas - this layer just transports them.
 */

// ============================================================================
// Generic Message Envelope
// ============================================================================

/**
 * Base message structure for all worker communication.
 * Generic over payload type T for type safety.
 */
export interface WorkerMessage<T = unknown> {
  id: string;              // UUID for correlation and deduplication
  type: string;            // Message type (opaque to transport layer)
  timestamp: string;       // ISO 8601 timestamp
  payload: T;              // Generic payload (worker-specific data)
}

/**
 * Request message from TypeScript daemon to Rust worker.
 * Optionally includes userId for context/auth.
 */
export interface WorkerRequest<T = unknown> extends WorkerMessage<T> {
  userId?: string;         // Optional user context
}

/**
 * Response message from Rust worker to TypeScript daemon.
 * Includes success/error state and correlation ID.
 */
export interface WorkerResponse<T = unknown> extends WorkerMessage<T> {
  requestId: string;       // Original request ID (correlation)
  success: boolean;        // Whether operation succeeded
  error?: string;          // Error message if failed
  errorType?: ErrorType;   // Categorized error type
  stack?: string;          // Optional stack trace for debugging
}

/**
 * Standard error types for worker operations.
 */
export type ErrorType = 'validation' | 'timeout' | 'internal' | 'not_found';

// ============================================================================
// Type Guards
// ============================================================================

/**
 * Type guard to check if message is a request.
 */
export function isWorkerRequest<T = unknown>(
  msg: WorkerMessage<T>
): msg is WorkerRequest<T> {
  return 'userId' in msg || !('requestId' in msg);
}

/**
 * Type guard to check if message is a response.
 */
export function isWorkerResponse<T = unknown>(
  msg: WorkerMessage<T>
): msg is WorkerResponse<T> {
  return 'requestId' in msg && 'success' in msg;
}
