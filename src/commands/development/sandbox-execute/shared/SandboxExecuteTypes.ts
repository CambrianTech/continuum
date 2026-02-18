/**
 * Sandbox Execute Types - Run AI-generated commands in isolation
 *
 * Executes commands from persona sandboxes using npx tsx for dynamic loading.
 * No main system recompile needed.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface SandboxExecuteParams extends CommandParams {
  /** Path to the sandbox command directory */
  commandPath: string;

  /** Parameters to pass to the command */
  params?: Record<string, unknown>;

  /** Timeout in milliseconds (default: 30000) */
  timeout?: number;
}

export interface SandboxExecuteResult extends CommandResult {
  success: boolean;

  /** Output from the command */
  output?: unknown;

  /** Stdout from execution */
  stdout?: string;

  /** Stderr from execution */
  stderr?: string;

  /** Execution time in ms */
  executionTimeMs?: number;

  /** Error if failed */
  error?: string;
}

export const createSandboxExecuteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<SandboxExecuteResult>, 'context' | 'sessionId'>
): SandboxExecuteResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: false,
  ...data
});

/**
 * SandboxExecute — Type-safe command executor
 *
 * Usage:
 *   import { SandboxExecute } from '...shared/SandboxExecuteTypes';
 *   const result = await SandboxExecute.execute({ ... });
 */
export const SandboxExecute = {
  execute(params: CommandInput<SandboxExecuteParams>): Promise<SandboxExecuteResult> {
    return Commands.execute<SandboxExecuteParams, SandboxExecuteResult>('development/sandbox-execute', params as Partial<SandboxExecuteParams>);
  },
  commandName: 'development/sandbox-execute' as const,
} as const;
