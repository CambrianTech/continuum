/**
 * Help Command Types
 *
 * Auto-generates help documentation for any command by querying the list command.
 * Returns both structured data and human-readable help text.
 */

import type { JTAGContext, JTAGPayload, CommandResult } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { CommandSignature } from '../../list/shared/ListTypes';

/**
 * Help command parameters
 */
export interface HelpParams extends JTAGPayload {
  readonly context: JTAGContext;
  readonly sessionId: UUID;

  /**
   * Command name to get help for (e.g., "tree", "data/read", "screenshot")
   * If omitted, shows general help about the help system
   */
  commandName?: string;

  /**
   * Show usage examples (default: true)
   */
  showExamples?: boolean;
}

/**
 * Help command result
 */
export interface HelpResult extends CommandResult {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly success: boolean;

  /** Command signature with full parameter details */
  readonly signature?: CommandSignature;

  /** Human-readable help text */
  readonly helpText: string;

  /** Usage examples if available */
  readonly examples?: string[];

  readonly error?: string;
}

/**
 * Create HelpResult from HelpParams (type-safe factory)
 */
export function createHelpResultFromParams(
  params: HelpParams,
  data: Partial<Omit<HelpResult, 'context' | 'sessionId'>>
): HelpResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: data.success ?? false,
    signature: data.signature,
    helpText: data.helpText ?? '',
    examples: data.examples,
    error: data.error
  };
}
