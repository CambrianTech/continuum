/**
 * Propose Command Types - AI Command Generation
 *
 * Allows AI personas to propose and generate new commands in their isolated sandbox.
 * Commands are generated to per-persona directories and loaded dynamically.
 *
 * Flow:
 * 1. AI proposes a CommandSpec JSON
 * 2. Command is generated to .continuum/personas/{personaId}/commands/
 * 3. Commands are namespaced: persona:{uniqueId}/command-name
 * 4. AI can test their command in isolation
 * 5. Human can promote to main codebase if valuable
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Parameter specification for generated command
 */
export interface ParamSpec {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'object' | 'array';
  optional?: boolean;
  description?: string;
  default?: unknown;
}

/**
 * Result field specification for generated command
 */
export interface ResultSpec {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'object' | 'array';
  description?: string;
}

/**
 * Example usage specification
 */
export interface ExampleSpec {
  description: string;
  params: Record<string, unknown>;
  expectedOutput?: string;
}

/**
 * Full command specification for generation
 */
export interface CommandSpec {
  /** Command name (e.g., "analyze-code", "summarize-thread") */
  name: string;

  /** Human-readable description */
  description: string;

  /** Parameter definitions */
  params: ParamSpec[];

  /** Result field definitions */
  results: ResultSpec[];

  /** Usage examples */
  examples?: ExampleSpec[];

  /** Execution environment */
  environment?: 'server' | 'browser' | 'both';

  /** Implementation notes for the AI */
  implementationNotes?: string;
}

export interface ProposeCommandParams extends CommandParams {
  /** The command specification to generate */
  spec: CommandSpec;

  /** If true, overwrite existing command */
  force?: boolean;

  /** If true, just validate spec without generating */
  dryRun?: boolean;
}

export interface ProposeCommandResult extends CommandResult {
  success: boolean;

  /** Generated command namespace (persona:uniqueId/command-name) */
  commandNamespace?: string;

  /** Path where command was generated */
  generatedPath?: string;

  /** Validation errors if any */
  errors?: string[];

  /** Warnings (non-fatal) */
  warnings?: string[];

  /** Next steps for the AI */
  nextSteps?: string[];
}

export const createProposeCommandResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ProposeCommandResult>, 'context' | 'sessionId'>
): ProposeCommandResult => createPayload(context, sessionId, {
  success: false,
  ...data
});

/**
 * ProposeCommand — Type-safe command executor
 *
 * Usage:
 *   import { ProposeCommand } from '...shared/ProposeCommandTypes';
 *   const result = await ProposeCommand.execute({ ... });
 */
export const ProposeCommand = {
  execute(params: CommandInput<ProposeCommandParams>): Promise<ProposeCommandResult> {
    return Commands.execute<ProposeCommandParams, ProposeCommandResult>('development/propose-command', params as Partial<ProposeCommandParams>);
  },
  commandName: 'development/propose-command' as const,
} as const;
