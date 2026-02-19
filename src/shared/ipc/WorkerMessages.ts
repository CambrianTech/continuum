/**
 * Worker IPC Protocol - Legacy Re-exports
 *
 * @deprecated This file is maintained for backwards compatibility only.
 * New code should import from JTAGProtocol.ts instead.
 *
 * The universal JTAGProtocol is now the SOURCE OF TRUTH for all IPC:
 * - Commands (Commands.execute)
 * - Events (Events.emit/subscribe)
 * - Worker IPC (Rust ↔ TypeScript)
 * - Daemon communication
 *
 * MIGRATION PATH:
 * - WorkerRequest → JTAGRequest
 * - WorkerResponse → JTAGResponse
 * - ErrorType → JTAGErrorType
 */

import {
  JTAGRequest,
  JTAGResponse,
  JTAGErrorType,
  isJTAGRequest,
  isJTAGResponse,
} from './JTAGProtocol.js';

// ============================================================================
// Legacy Aliases (for backwards compatibility)
// ============================================================================

/**
 * @deprecated Use JTAGRequest instead
 */
export interface WorkerMessage<T = unknown> {
  id: string;
  type: string;
  timestamp: string;
  payload: T;
}

/**
 * @deprecated Use JTAGRequest instead
 */
export type WorkerRequest<T = unknown> = JTAGRequest<T>;

/**
 * @deprecated Use JTAGResponse instead
 */
export type WorkerResponse<T = unknown> = JTAGResponse<T>;

/**
 * @deprecated Use JTAGErrorType instead
 */
export type ErrorType = JTAGErrorType;

// ============================================================================
// Type Guards (legacy)
// ============================================================================

/**
 * @deprecated Use isJTAGRequest instead
 */
export const isWorkerRequest = isJTAGRequest;

/**
 * @deprecated Use isJTAGResponse instead
 */
export const isWorkerResponse = isJTAGResponse;
