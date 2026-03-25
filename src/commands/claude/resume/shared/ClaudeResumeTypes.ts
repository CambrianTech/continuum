/**
 * Claude Resume Command - Shared Types
 *
 * Loads the latest snapshot and current context, synthesizes a session briefing. Run this first thing in a new session — it tells you who you are, what you were doing, and what to do next.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Claude Resume Command Parameters
 */
export interface ClaudeResumeParams extends CommandParams {
  // Include full chat history and issue details (default: false — summary only)
  verbose?: boolean;
}

/**
 * Factory function for creating ClaudeResumeParams
 */
export const createClaudeResumeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Include full chat history and issue details (default: false — summary only)
    verbose?: boolean;
  }
): ClaudeResumeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  verbose: data.verbose ?? false,
  ...data
});

/**
 * Claude Resume Command Result
 */
export interface ClaudeResumeResult extends CommandResult {
  success: boolean;
  // The latest saved snapshot (null if none exists)
  snapshot: object;
  // Git commits since the snapshot was taken
  gitSince: string;
  // Human-readable session briefing — everything you need to resume
  briefing: string;
  error?: JTAGError;
}

/**
 * Factory function for creating ClaudeResumeResult with defaults
 */
export const createClaudeResumeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The latest saved snapshot (null if none exists)
    snapshot?: object;
    // Git commits since the snapshot was taken
    gitSince?: string;
    // Human-readable session briefing — everything you need to resume
    briefing?: string;
    error?: JTAGError;
  }
): ClaudeResumeResult => createPayload(context, sessionId, {
  snapshot: data.snapshot ?? {},
  gitSince: data.gitSince ?? '',
  briefing: data.briefing ?? '',
  ...data
});

/**
 * Smart Claude Resume-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createClaudeResumeResultFromParams = (
  params: ClaudeResumeParams,
  differences: Omit<ClaudeResumeResult, 'context' | 'sessionId' | 'userId'>
): ClaudeResumeResult => transformPayload(params, differences);

/**
 * Claude Resume — Type-safe command executor
 *
 * Usage:
 *   import { ClaudeResume } from '...shared/ClaudeResumeTypes';
 *   const result = await ClaudeResume.execute({ ... });
 */
export const ClaudeResume = {
  execute(params: CommandInput<ClaudeResumeParams>): Promise<ClaudeResumeResult> {
    return Commands.execute<ClaudeResumeParams, ClaudeResumeResult>('claude/resume', params as Partial<ClaudeResumeParams>);
  },
  commandName: 'claude/resume' as const,
} as const;
