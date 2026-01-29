/**
 * RAG Query Close Command Types
 *
 * Closes a query handle and cleans up resources
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

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

/**
 * RagQueryClose — Type-safe command executor
 *
 * Usage:
 *   import { RagQueryClose } from '...shared/RagQueryCloseTypes';
 *   const result = await RagQueryClose.execute({ ... });
 */
export const RagQueryClose = {
  execute(params: CommandInput<RagQueryCloseParams>): Promise<RagQueryCloseResult> {
    return Commands.execute<RagQueryCloseParams, RagQueryCloseResult>('ai/rag/query-close', params as Partial<RagQueryCloseParams>);
  },
  commandName: 'ai/rag/query-close' as const,
} as const;
