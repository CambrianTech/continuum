/**
 * Generate Command - Shared Types
 *
 * Generate a new command from a CommandSpec JSON definition
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Generate Command Parameters
 */
export interface GenerateParams extends CommandParams {
  // CommandSpec object, file path string, or JSON string defining the command to generate
  spec: string | object;
  // Return an example CommandSpec template instead of generating
  template?: boolean;
}

/**
 * Factory function for creating GenerateParams
 */
export const createGenerateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<GenerateParams>, 'context' | 'sessionId'> & Pick<GenerateParams, 'spec'>
): GenerateParams => createPayload(context, sessionId, data);

/**
 * Generate Command Result
 */
export interface GenerateResult extends CommandResult {
  success: boolean;
  // Array of file paths that were created
  filesCreated: string[];
  // Base directory path where command was generated
  commandPath: string;
  // Example CommandSpec template (only when template=true)
  templateSpec?: object;
  error?: string;
}

/**
 * Factory function for creating GenerateResult with defaults
 */
export const createGenerateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<GenerateResult>, 'context' | 'sessionId'>
): GenerateResult => createPayload(context, sessionId, {
  success: false,
  filesCreated: [],
  commandPath: '',
  ...data
});

/**
 * Smart Generate-specific inheritance from params
 * Auto-inherits common fields from params
 * Only specify what changed: success, error, and result-specific fields
 */
export const createGenerateResultFromParams = (
  params: GenerateParams,
  differences: Omit<Partial<GenerateResult>, 'context' | 'sessionId'>
): GenerateResult => transformPayload(params, {
  success: false,
  filesCreated: [],
  commandPath: '',
  ...differences
});

/**
 * Generate — Type-safe command executor
 *
 * Usage:
 *   import { Generate } from '...shared/GenerateTypes';
 *   const result = await Generate.execute({ ... });
 */
export const Generate = {
  execute(params: CommandInput<GenerateParams>): Promise<GenerateResult> {
    return Commands.execute<GenerateParams, GenerateResult>('development/generate', params as Partial<GenerateParams>);
  },
  commandName: 'development/generate' as const,
} as const;
