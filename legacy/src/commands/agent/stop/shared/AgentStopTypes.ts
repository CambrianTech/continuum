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
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating AgentStopParams
 */
export const createAgentStopParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentStopParams, 'context' | 'sessionId' | 'userId'>
): AgentStopParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AgentStopResult with defaults
 */
export const createAgentStopResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AgentStopResult, 'context' | 'sessionId' | 'userId'>
): AgentStopResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart agent/stop-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAgentStopResultFromParams = (
  params: AgentStopParams,
  differences: Omit<AgentStopResult, 'context' | 'sessionId' | 'userId'>
): AgentStopResult => transformPayload(params, differences);

