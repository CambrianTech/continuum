/**
 * Claude Context Command - Shared Types
 *
 * Generates a comprehensive context summary for Claude Code session resumption — recent git changes, open issues, team chat, system health, and active work state. This is Claude's bridge from stateless sessions to persistent citizenship.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Claude Context Command Parameters
 */
export interface ClaudeContextParams extends CommandParams {
  // Include recent git log and uncommitted changes (default: true)
  includeGit?: boolean;
  // Include open GitHub issues summary (default: true)
  includeIssues?: boolean;
  // Include recent team chat messages (default: true)
  includeChat?: boolean;
  // Include system health status (default: true)
  includeHealth?: boolean;
  // Number of recent chat messages to include (default: 20)
  chatLimit?: number;
  // Number of recent git commits to include (default: 10)
  gitLimit?: number;
  // Number of open issues to include (default: 20)
  issueLimit?: number;
}

/**
 * Factory function for creating ClaudeContextParams
 */
export const createClaudeContextParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Include recent git log and uncommitted changes (default: true)
    includeGit?: boolean;
    // Include open GitHub issues summary (default: true)
    includeIssues?: boolean;
    // Include recent team chat messages (default: true)
    includeChat?: boolean;
    // Include system health status (default: true)
    includeHealth?: boolean;
    // Number of recent chat messages to include (default: 20)
    chatLimit?: number;
    // Number of recent git commits to include (default: 10)
    gitLimit?: number;
    // Number of open issues to include (default: 20)
    issueLimit?: number;
  }
): ClaudeContextParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  includeGit: data.includeGit ?? false,
  includeIssues: data.includeIssues ?? false,
  includeChat: data.includeChat ?? false,
  includeHealth: data.includeHealth ?? false,
  chatLimit: data.chatLimit ?? 0,
  gitLimit: data.gitLimit ?? 0,
  issueLimit: data.issueLimit ?? 0,
  ...data
});

/**
 * Claude Context Command Result
 */
export interface ClaudeContextResult extends CommandResult {
  success: boolean;
  // Git state: branch, recent commits, uncommitted changes
  git: object;
  // Open issues grouped by phase from gap analysis
  issues: object;
  // Recent team chat messages and active discussions
  chat: object;
  // System health: server status, browser connection, active personas
  health: object;
  // Human-readable summary of current state for session resumption
  summary: string;
  error?: JTAGError;
}

/**
 * Factory function for creating ClaudeContextResult with defaults
 */
export const createClaudeContextResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Git state: branch, recent commits, uncommitted changes
    git?: object;
    // Open issues grouped by phase from gap analysis
    issues?: object;
    // Recent team chat messages and active discussions
    chat?: object;
    // System health: server status, browser connection, active personas
    health?: object;
    // Human-readable summary of current state for session resumption
    summary?: string;
    error?: JTAGError;
  }
): ClaudeContextResult => createPayload(context, sessionId, {
  git: data.git ?? {},
  issues: data.issues ?? {},
  chat: data.chat ?? {},
  health: data.health ?? {},
  summary: data.summary ?? '',
  ...data
});

/**
 * Smart Claude Context-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createClaudeContextResultFromParams = (
  params: ClaudeContextParams,
  differences: Omit<ClaudeContextResult, 'context' | 'sessionId' | 'userId'>
): ClaudeContextResult => transformPayload(params, differences);

/**
 * Claude Context — Type-safe command executor
 *
 * Usage:
 *   import { ClaudeContext } from '...shared/ClaudeContextTypes';
 *   const result = await ClaudeContext.execute({ ... });
 */
export const ClaudeContext = {
  execute(params: CommandInput<ClaudeContextParams>): Promise<ClaudeContextResult> {
    return Commands.execute<ClaudeContextParams, ClaudeContextResult>('claude/context', params as Partial<ClaudeContextParams>);
  },
  commandName: 'claude/context' as const,
} as const;
