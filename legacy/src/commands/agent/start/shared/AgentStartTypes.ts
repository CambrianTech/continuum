/**
 * Agent Start Command - Shared Types
 *
 * Starts an autonomous coding agent.
 *
 * Usage:
 *   ./jtag agent/start --task="List files in current directory" --working_dir="."
 *   ./jtag agent/start --task="Fix the bug in main.ts" --working_dir="/path/to/project" --model="qwen2.5:7b"
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface AgentStartParams extends CommandParams {
  /** The task for the agent to accomplish */
  task: string;
  /** Working directory for file operations */
  working_dir: string;
  /** Model to use (required - see available models via 'candle' provider) */
  model: string;
  /** Maximum iterations before stopping (default: 20) */
  max_iterations?: number;
}

export interface AgentStartResult extends CommandResult {
  /** Handle to track agent progress */
  handle: string;
  /** Current status */
  status: 'running' | 'failed';
  /** Error if failed */
  error?: string;
}

/**
 * AgentStart — Type-safe command executor
 *
 * Usage:
 *   import { AgentStart } from '...shared/AgentStartTypes';
 *   const result = await AgentStart.execute({ ... });
 */
export const AgentStart = {
  execute(params: CommandInput<AgentStartParams>): Promise<AgentStartResult> {
    return Commands.execute<AgentStartParams, AgentStartResult>('agent/start', params as Partial<AgentStartParams>);
  },
  commandName: 'agent/start' as const,
} as const;

/**
 * Factory function for creating AgentStartParams
 */
export const createAgentStartParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentStartParams, 'context' | 'sessionId' | 'userId'>
): AgentStartParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AgentStartResult with defaults
 */
export const createAgentStartResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentStartResult, 'context' | 'sessionId' | 'userId'>
): AgentStartResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart agent/start-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAgentStartResultFromParams = (
  params: AgentStartParams,
  differences: Omit<AgentStartResult, 'context' | 'sessionId' | 'userId'>
): AgentStartResult => transformPayload(params, differences);

