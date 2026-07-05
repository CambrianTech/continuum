/**
 * Agent List Command - Shared Types
 *
 * Lists all running agents.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface AgentListParams extends CommandParams {
  // No params needed
}

export interface AgentInfo {
  id: string;
  task: string;
  status: 'running' | 'completed' | 'failed' | 'stopped';
  iteration: number;
  startedAt: number;
  completedAt?: number;
  summary?: string;
  error?: string;
}

export interface AgentListResult extends CommandResult {
  agents: AgentInfo[];
}

/**
 * AgentList — Type-safe command executor
 *
 * Usage:
 *   import { AgentList } from '...shared/AgentListTypes';
 *   const result = await AgentList.execute({ ... });
 */
export const AgentList = {
  execute(params: CommandInput<AgentListParams>): Promise<AgentListResult> {
    return Commands.execute<AgentListParams, AgentListResult>('agent/list', params as Partial<AgentListParams>);
  },
  commandName: 'agent/list' as const,
} as const;

/**
 * Factory function for creating AgentListParams
 */
export const createAgentListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentListParams, 'context' | 'sessionId' | 'userId'>
): AgentListParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AgentListResult with defaults
 */
export const createAgentListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentListResult, 'context' | 'sessionId' | 'userId'>
): AgentListResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart agent/list-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAgentListResultFromParams = (
  params: AgentListParams,
  differences: Omit<AgentListResult, 'context' | 'sessionId' | 'userId'>
): AgentListResult => transformPayload(params, differences);

