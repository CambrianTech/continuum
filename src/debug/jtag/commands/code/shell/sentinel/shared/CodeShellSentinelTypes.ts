/**
 * Code Shell Sentinel Command - Shared Types
 *
 * Configure sentinel filter rules on a shell execution. Rules classify output lines
 * and control which lines are emitted or suppressed during watch.
 * Patterns are compiled to regex on the Rust side for performance.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SentinelRule } from '@shared/generated/code/SentinelRule';

/**
 * Code Shell Sentinel Command Parameters
 */
export interface CodeShellSentinelParams extends CommandParams {
  /** Execution handle to attach sentinel rules to */
  executionId: string;
  /** Array of classification rules (pattern, classification, action) */
  rules: SentinelRule[];
}

/**
 * Factory function for creating CodeShellSentinelParams
 */
export const createCodeShellSentinelParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    executionId: string;
    rules: SentinelRule[];
  }
): CodeShellSentinelParams => createPayload(context, sessionId, {
  ...data
});

/**
 * Code Shell Sentinel Command Result
 */
export interface CodeShellSentinelResult extends CommandResult {
  success: boolean;
  /** Whether rules were applied successfully */
  applied: boolean;
  /** Number of sentinel rules configured */
  ruleCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeShellSentinelResult with defaults
 */
export const createCodeShellSentinelResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    applied?: boolean;
    ruleCount?: number;
    error?: JTAGError;
  }
): CodeShellSentinelResult => createPayload(context, sessionId, {
  applied: data.applied ?? false,
  ruleCount: data.ruleCount ?? 0,
  ...data
});

/**
 * Smart Code Shell Sentinel-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeShellSentinelResultFromParams = (
  params: CodeShellSentinelParams,
  differences: Omit<CodeShellSentinelResult, 'context' | 'sessionId'>
): CodeShellSentinelResult => transformPayload(params, differences);

/**
 * Code Shell Sentinel — Type-safe command executor
 *
 * Usage:
 *   import { CodeShellSentinel } from '...shared/CodeShellSentinelTypes';
 *   const result = await CodeShellSentinel.execute({ ... });
 */
export const CodeShellSentinel = {
  execute(params: CommandInput<CodeShellSentinelParams>): Promise<CodeShellSentinelResult> {
    return Commands.execute<CodeShellSentinelParams, CodeShellSentinelResult>('code/shell/sentinel', params as Partial<CodeShellSentinelParams>);
  },
  commandName: 'code/shell/sentinel' as const,
} as const;
