/**
 * dev/activity — Show what each persona and sentinel has edited.
 *
 * Usage:
 *   ./jtag dev/activity                              # All persona workspaces + recent sentinels
 *   ./jtag dev/activity --personaId="helper"          # Single persona's activity
 *   ./jtag dev/activity --repo="/path/to/project"     # Activity on a specific repo
 *   ./jtag dev/activity --sentinelsOnly=true           # Only sentinel history
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface DevActivityParams extends CommandParams {
  /** Filter to a specific persona (by uniqueId or UUID) */
  personaId?: string;
  /** Filter to activity on a specific repo path */
  repo?: string;
  /** Only show sentinel history (skip workspace git activity) */
  sentinelsOnly?: boolean;
  /** Max recent sentinels to show (default: 10) */
  limit?: number;
}

export interface WorkspaceActivity {
  personaId: string;
  personaName: string;
  branch: string;
  worktreeDir: string;
  repoPath: string;
  modified: string[];
  staged: string[];
  untracked: string[];
  recentCommits: CommitInfo[];
}

export interface CommitInfo {
  hash: string;
  message: string;
  date: string;
}

export interface SentinelActivity {
  handle: string;
  name: string;
  status: string;
  personaId?: string;
  template?: string;
  workingDir: string;
  startedAt?: string;
  durationMs?: number;
  error?: string;
}

export interface DevActivityResult extends CommandResult {
  success: boolean;
  workspaces: WorkspaceActivity[];
  sentinels: SentinelActivity[];
  error?: string;
}

export const DevActivity = {
  execute(params?: CommandInput<DevActivityParams>): Promise<DevActivityResult> {
    return Commands.execute<DevActivityParams, DevActivityResult>('dev/activity', params as Partial<DevActivityParams>);
  },
  commandName: 'dev/activity' as const,
} as const;
