/**
 * Agent Status Command - Shared Types
 *
 * Gets the status of an agent by handle.
 *
 * Usage:
 *   ./jtag agent/status --handle="abc12345"
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
