/**
 * Agent List Command - Shared Types
 *
 * Lists all running agents.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
