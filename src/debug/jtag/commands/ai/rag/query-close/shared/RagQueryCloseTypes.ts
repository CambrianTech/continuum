/**
 * RAG Query Close Command Types
 *
 * Closes a query handle and cleans up resources
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

/**
 * Parameters for closing a query handle
 */
export interface RagQueryCloseParams extends CommandParams {
  // Query handle to close
  queryHandle: UUID;
}

/**
 * Result of closing a query handle
 */
export interface RagQueryCloseResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;
  readonly closed: boolean;  // True if handle was found and closed
}
