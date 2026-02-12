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
