/**
 * Agent Stop Command - Shared Types
 *
 * Stops a running agent.
 *
 * Usage:
 *   ./jtag agent/stop --handle="abc12345"
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface AgentStopParams extends CommandParams {
  /** Agent handle from agent/start */
  handle: string;
}

export interface AgentStopResult extends CommandResult {
  handle: string;
  stopped: boolean;
  error?: string;
}
