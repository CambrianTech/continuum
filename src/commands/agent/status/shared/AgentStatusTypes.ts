/**
 * Agent Status Command - Shared Types
 *
 * Gets the status of an agent by handle.
 *
 * Usage:
 *   ./jtag agent/status --handle="abc12345"
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface AgentStatusParams extends CommandParams {
  /** Agent handle from agent/start */
  handle: string;
}

export interface AgentStatusResult extends CommandResult {
  handle: string;
  task: string;
  status: 'running' | 'completed' | 'failed' | 'stopped' | 'not_found';
  iteration: number;
  startedAt: number;
  completedAt?: number;
  summary?: string;
  error?: string;
  /** Recent events/logs */
  events?: string[];
}

/**
 * AgentStatus — Type-safe command executor
 *
 * Usage:
 *   import { AgentStatus } from '...shared/AgentStatusTypes';
 *   const result = await AgentStatus.execute({ ... });
 */
export const AgentStatus = {
  execute(params: CommandInput<AgentStatusParams>): Promise<AgentStatusResult> {
    return Commands.execute<AgentStatusParams, AgentStatusResult>('agent/status', params as Partial<AgentStatusParams>);
  },
  commandName: 'agent/status' as const,
} as const;

/**
 * Factory function for creating AgentStatusParams
 */
export const createAgentStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentStatusParams, 'context' | 'sessionId' | 'userId'>
): AgentStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AgentStatusResult with defaults
 */
export const createAgentStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentStatusResult, 'context' | 'sessionId' | 'userId'>
): AgentStatusResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart agent/status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAgentStatusResultFromParams = (
  params: AgentStatusParams,
  differences: Omit<AgentStatusResult, 'context' | 'sessionId' | 'userId'>
): AgentStatusResult => transformPayload(params, differences);

