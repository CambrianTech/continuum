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
