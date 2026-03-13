/**
 * RAG Query Close Command Types
 *
 * Closes a query handle and cleans up resources
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

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

/**
 * Factory function for creating AiRagQueryCloseParams
 */
export const createRagQueryCloseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<RagQueryCloseParams, 'context' | 'sessionId' | 'userId'>
): RagQueryCloseParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiRagQueryCloseResult with defaults
 */
export const createRagQueryCloseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<RagQueryCloseResult, 'context' | 'sessionId' | 'userId'>
): RagQueryCloseResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/rag/query-close-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createRagQueryCloseResultFromParams = (
  params: RagQueryCloseParams,
  differences: Omit<RagQueryCloseResult, 'context' | 'sessionId' | 'userId'>
): RagQueryCloseResult => transformPayload(params, differences);

