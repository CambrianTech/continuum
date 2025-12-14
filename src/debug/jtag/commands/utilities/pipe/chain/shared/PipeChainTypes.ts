/**
 * Pipe Chain Command Types
 * Enables Unix-style command chaining: cmd1 | cmd2 | cmd3
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface PipeChainParams extends CommandParams {
  /** Commands to chain together, separated by pipe (|) */
  commands: string;

  /** How to handle errors in the chain */
  errorHandling?: 'stop' | 'continue' | 'collect';

  /** Maximum execution time for entire chain (ms) */
  timeout?: number;

  /** Whether to show intermediate results */
  showIntermediate?: boolean;

  /** Output format for final result */
  format?: 'json' | 'text' | 'auto';
}

export interface PipeChainResult extends CommandResult {
  success: boolean;
  steps: PipeStepResult[];
  finalOutput: any;
  totalExecutionTime: number;
  error?: string;
}

export interface PipeStepResult {
  command: string;
  success: boolean;
  output: any;
  executionTime: number;
  error?: string;
}

export interface PipeableCommand {
  /** Whether this command can accept stdin input */
  acceptsStdin(): boolean;

  /** Whether this command produces stdout output */
  producesStdout(): boolean;

  /** The output format this command produces */
  getOutputFormat(): 'json' | 'text' | 'binary';

  /** Execute the command with optional stdin input */
  execute(params: any, stdin?: string): Promise<any>;
}

/**
 * Factory function for creating PipeChainParams
 */
export const createPipeChainParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<PipeChainParams, 'context' | 'sessionId'>
): PipeChainParams => createPayload(context, sessionId, data);

/**
 * Factory function for creating PipeChainResult from params
 */
export const createPipeChainResult = (
  params: PipeChainParams,
  differences: Omit<Partial<PipeChainResult>, 'context' | 'sessionId'>
): PipeChainResult => transformPayload(params, {
  success: false,
  steps: [],
  finalOutput: null,
  totalExecutionTime: 0,
  ...differences
});