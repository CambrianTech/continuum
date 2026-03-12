/**
 * Agent Stop Command - Shared Types
 *
 * Stops a running agent.
 *
 * Usage:
 *   ./jtag agent/stop --handle="abc12345"
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface AgentStopParams extends CommandParams {
  /** Agent handle from agent/start */
  handle: string;
}

export interface AgentStopResult extends CommandResult {
  handle: string;
  stopped: boolean;
  error?: string;
}

/**
 * AgentStop — Type-safe command executor
 *
 * Usage:
 *   import { AgentStop } from '...shared/AgentStopTypes';
 *   const result = await AgentStop.execute({ ... });
 */
export const AgentStop = {
  execute(params: CommandInput<AgentStopParams>): Promise<AgentStopResult> {
    return Commands.execute<AgentStopParams, AgentStopResult>('agent/stop', params as Partial<AgentStopParams>);
  },
  commandName: 'agent/stop' as const,
} as const;
