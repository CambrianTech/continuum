/**
 * Claude Snapshot Command - Shared Types
 *
 * Saves a work-state snapshot for session continuity. Captures what Claude was doing, what's pending, and what comes next — so the next Claude instance can resume without reading 200 lines of MEMORY.md and guessing.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Claude Snapshot Command Parameters
 */
export interface ClaudeSnapshotParams extends CommandParams {
  // What was being worked on — the current task and approach
  summary: string;
  // What's unfinished — branches, uncommitted code, failing tests
  pendingWork?: string;
  // What should happen next — the plan for the next session
  nextSteps?: string;
  // Key decisions made this session and why — so the next instance doesn't relitigate them
  decisions?: string;
  // Comma-separated issue numbers touched this session (e.g. '376,335,317')
  issuesWorked?: string;
}

/**
 * Factory function for creating ClaudeSnapshotParams
 */
export const createClaudeSnapshotParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // What was being worked on — the current task and approach
    summary: string;
    // What's unfinished — branches, uncommitted code, failing tests
    pendingWork?: string;
    // What should happen next — the plan for the next session
    nextSteps?: string;
    // Key decisions made this session and why — so the next instance doesn't relitigate them
    decisions?: string;
    // Comma-separated issue numbers touched this session (e.g. '376,335,317')
    issuesWorked?: string;
  }
): ClaudeSnapshotParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  pendingWork: data.pendingWork ?? '',
  nextSteps: data.nextSteps ?? '',
  decisions: data.decisions ?? '',
  issuesWorked: data.issuesWorked ?? '',
  ...data
});

/**
 * Claude Snapshot Command Result
 */
export interface ClaudeSnapshotResult extends CommandResult {
  success: boolean;
  // Unique ID for this snapshot
  snapshotId: string;
  // Where the snapshot was saved
  filePath: string;
  // When the snapshot was taken
  timestamp: string;
  error?: JTAGError;
}

/**
 * Factory function for creating ClaudeSnapshotResult with defaults
 */
export const createClaudeSnapshotResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Unique ID for this snapshot
    snapshotId?: string;
    // Where the snapshot was saved
    filePath?: string;
    // When the snapshot was taken
    timestamp?: string;
    error?: JTAGError;
  }
): ClaudeSnapshotResult => createPayload(context, sessionId, {
  snapshotId: data.snapshotId ?? '',
  filePath: data.filePath ?? '',
  timestamp: data.timestamp ?? '',
  ...data
});

/**
 * Smart Claude Snapshot-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createClaudeSnapshotResultFromParams = (
  params: ClaudeSnapshotParams,
  differences: Omit<ClaudeSnapshotResult, 'context' | 'sessionId' | 'userId'>
): ClaudeSnapshotResult => transformPayload(params, differences);

/**
 * Claude Snapshot — Type-safe command executor
 *
 * Usage:
 *   import { ClaudeSnapshot } from '...shared/ClaudeSnapshotTypes';
 *   const result = await ClaudeSnapshot.execute({ ... });
 */
export const ClaudeSnapshot = {
  execute(params: CommandInput<ClaudeSnapshotParams>): Promise<ClaudeSnapshotResult> {
    return Commands.execute<ClaudeSnapshotParams, ClaudeSnapshotResult>('claude/snapshot', params as Partial<ClaudeSnapshotParams>);
  },
  commandName: 'claude/snapshot' as const,
} as const;
