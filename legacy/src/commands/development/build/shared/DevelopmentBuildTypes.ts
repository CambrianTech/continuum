/**
 * Development Build Command - Shared Types
 *
 * Zero-friction TypeScript build check. Returns success or structured errors.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Parsed TypeScript compilation error
 */
export interface TypeScriptError {
  file: string;
  line: number;
  column: number;
  code: string;
  message: string;
}

/**
 * Development Build Command Parameters
 */
export interface DevelopmentBuildParams extends CommandParams {
  // Only return success/failure, no output details
  quiet?: boolean;
}

/**
 * Factory function for creating DevelopmentBuildParams
 */
export const createDevelopmentBuildParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Only return success/failure, no output details
    quiet?: boolean;
  }
): DevelopmentBuildParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  quiet: data.quiet ?? false,
  ...data
});

/**
 * Development Build Command Result
 */
export interface DevelopmentBuildResult extends CommandResult {
  success: boolean;
  // Number of compilation errors
  errorCount: number;
  // Parsed errors with file, line, column, code, message
  errors: TypeScriptError[];
  // Build time in milliseconds
  duration: number;
  // Raw compiler output (omitted in quiet mode)
  output: string;
  error?: JTAGError;
}

/**
 * Factory function for creating DevelopmentBuildResult with defaults
 */
export const createDevelopmentBuildResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Number of compilation errors
    errorCount?: number;
    // Parsed errors with file, line, column, code, message
    errors?: TypeScriptError[];
    // Build time in milliseconds
    duration?: number;
    // Raw compiler output (omitted in quiet mode)
    output?: string;
    error?: JTAGError;
  }
): DevelopmentBuildResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  errorCount: data.errorCount ?? 0,
  errors: data.errors ?? [],
  duration: data.duration ?? 0,
  output: data.output ?? '',
  ...data
});

/**
 * Smart Development Build-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentBuildResultFromParams = (
  params: DevelopmentBuildParams,
  differences: Omit<DevelopmentBuildResult, 'context' | 'sessionId'>
): DevelopmentBuildResult => transformPayload(params, differences);

/**
 * DevelopmentBuild — Type-safe command executor
 *
 * Usage:
 *   import { DevelopmentBuild } from '...shared/DevelopmentBuildTypes';
 *   const result = await DevelopmentBuild.execute({ ... });
 */
export const DevelopmentBuild = {
  execute(params: CommandInput<DevelopmentBuildParams>): Promise<DevelopmentBuildResult> {
    return Commands.execute<DevelopmentBuildParams, DevelopmentBuildResult>('development/build', params as Partial<DevelopmentBuildParams>);
  },
  commandName: 'development/build' as const,
} as const;
