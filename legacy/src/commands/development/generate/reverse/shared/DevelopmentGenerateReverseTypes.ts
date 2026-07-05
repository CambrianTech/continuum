/**
 * Development Generate Reverse Command - Shared Types
 *
 * Reverse-engineer a CommandSpec from an existing hand-written command. Reads the Types file, extracts params, results, command name, and description, then outputs a spec JSON that can be saved to generator/specs/ and used to regenerate the command under generator control.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Development Generate Reverse Command Parameters
 */
export interface DevelopmentGenerateReverseParams extends CommandParams {
  // Path to the command directory (e.g., 'commands/ping', 'commands/data/create')
  commandDir: string;
  // Save the generated spec directly to generator/specs/<name>.json instead of returning it
  save?: boolean;
}

/**
 * Factory function for creating DevelopmentGenerateReverseParams
 */
export const createDevelopmentGenerateReverseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to the command directory (e.g., 'commands/ping', 'commands/data/create')
    commandDir: string;
    // Save the generated spec directly to generator/specs/<name>.json instead of returning it
    save?: boolean;
  }
): DevelopmentGenerateReverseParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  save: data.save ?? false,
  ...data
});

/**
 * Development Generate Reverse Command Result
 */
export interface DevelopmentGenerateReverseResult extends CommandResult {
  success: boolean;
  // The reverse-engineered CommandSpec JSON
  spec: object;
  // File path where spec was saved (only when save=true)
  savedTo: string;
  // Warnings about fields that need manual review (e.g., description guessed from comments)
  warnings: string[];
  error?: JTAGError;
}

/**
 * Factory function for creating DevelopmentGenerateReverseResult with defaults
 */
export const createDevelopmentGenerateReverseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The reverse-engineered CommandSpec JSON
    spec?: object;
    // File path where spec was saved (only when save=true)
    savedTo?: string;
    // Warnings about fields that need manual review (e.g., description guessed from comments)
    warnings?: string[];
    error?: JTAGError;
  }
): DevelopmentGenerateReverseResult => createPayload(context, sessionId, {
  spec: data.spec ?? {},
  savedTo: data.savedTo ?? '',
  warnings: data.warnings ?? [],
  ...data
});

/**
 * Smart Development Generate Reverse-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentGenerateReverseResultFromParams = (
  params: DevelopmentGenerateReverseParams,
  differences: Omit<DevelopmentGenerateReverseResult, 'context' | 'sessionId' | 'userId'>
): DevelopmentGenerateReverseResult => transformPayload(params, differences);

/**
 * Development Generate Reverse — Type-safe command executor
 *
 * Usage:
 *   import { DevelopmentGenerateReverse } from '...shared/DevelopmentGenerateReverseTypes';
 *   const result = await DevelopmentGenerateReverse.execute({ ... });
 */
export const DevelopmentGenerateReverse = {
  execute(params: CommandInput<DevelopmentGenerateReverseParams>): Promise<DevelopmentGenerateReverseResult> {
    return Commands.execute<DevelopmentGenerateReverseParams, DevelopmentGenerateReverseResult>('development/generate/reverse', params as Partial<DevelopmentGenerateReverseParams>);
  },
  commandName: 'development/generate/reverse' as const,
} as const;
