/**
 * Development Generate Help Command - Shared Types
 *
 * Display comprehensive generator documentation including spec reference, example specs, type reference, access levels, workflow guide, and audit information. This is the primary documentation entry point for AI agents learning to use the generator.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Development Generate Help Command Parameters
 */
export interface DevelopmentGenerateHelpParams extends CommandParams {
  // Specific help topic. Omit for full documentation.
  topic?: 'full' | 'spec' | 'types' | 'examples' | 'audit' | 'workflow';
}

/**
 * Factory function for creating DevelopmentGenerateHelpParams
 */
export const createDevelopmentGenerateHelpParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Specific help topic. Omit for full documentation.
    topic?: 'full' | 'spec' | 'types' | 'examples' | 'audit' | 'workflow';
  }
): DevelopmentGenerateHelpParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  topic: data.topic ?? undefined,
  ...data
});

/**
 * Development Generate Help Command Result
 */
export interface DevelopmentGenerateHelpResult extends CommandResult {
  success: boolean;
  // Formatted help text with spec reference, examples, and workflow guidance
  content: string;
  // The topic that was displayed
  topic: string;
  error?: JTAGError;
}

/**
 * Factory function for creating DevelopmentGenerateHelpResult with defaults
 */
export const createDevelopmentGenerateHelpResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Formatted help text with spec reference, examples, and workflow guidance
    content?: string;
    // The topic that was displayed
    topic?: string;
    error?: JTAGError;
  }
): DevelopmentGenerateHelpResult => createPayload(context, sessionId, {
  content: data.content ?? '',
  topic: data.topic ?? '',
  ...data
});

/**
 * Smart Development Generate Help-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentGenerateHelpResultFromParams = (
  params: DevelopmentGenerateHelpParams,
  differences: Omit<DevelopmentGenerateHelpResult, 'context' | 'sessionId' | 'userId'>
): DevelopmentGenerateHelpResult => transformPayload(params, differences);

/**
 * Development Generate Help — Type-safe command executor
 *
 * Usage:
 *   import { DevelopmentGenerateHelp } from '...shared/DevelopmentGenerateHelpTypes';
 *   const result = await DevelopmentGenerateHelp.execute({ ... });
 */
export const DevelopmentGenerateHelp = {
  execute(params: CommandInput<DevelopmentGenerateHelpParams>): Promise<DevelopmentGenerateHelpResult> {
    return Commands.execute<DevelopmentGenerateHelpParams, DevelopmentGenerateHelpResult>('development/generate/help', params as Partial<DevelopmentGenerateHelpParams>);
  },
  commandName: 'development/generate/help' as const,
} as const;
