/**
 * Agent List Command - Shared Types
 *
 * Lists all running agents.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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
