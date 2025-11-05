/**
 * Exec Command Shared Types
 * 
 * Types and interfaces used across all execution contexts for the exec command.
 */

import { CommandExecution } from '../../../../types/shared/CommandTypes';

/**
 * Parameters for the exec command
 */
export interface ExecParameters {
  /** Command name to execute */
  command?: string;
  /** Arguments for the command */
  args?: string[];
  /** Full CommandExecution object */
  execution?: CommandExecution;
}

/**
 * Exec command result metadata
 */
export interface ExecMetadata {
  executionTime: number;
  processor: string;
  timestamp: string;
  originalCommand: string;
  requestId?: string;
}

/**
 * Exec command result
 */
export interface ExecResult {
  execution: CommandExecution;
  result: any;
  metadata: ExecMetadata;
}

/**
 * Validation for exec parameters
 */
export function validateExecParameters(params: any): params is ExecParameters {
  return (
    typeof params === 'object' &&
    params !== null &&
    (
      typeof params.command === 'string' ||
      (params.execution && typeof params.execution === 'object')
    )
  );
}