/**
 * dev/fix-bug — Shorthand for sentinel/run --template=dev/fix-bug
 *
 * Usage:
 *   ./jtag dev/fix-bug --bug="Users can't upload files > 2MB"
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface DevFixBugParams extends CommandParams {
  /** Bug description */
  bug: string;
  /** Working directory (default: cwd) */
  cwd?: string;
  /** Skip collaborative checkpoints */
  autonomous?: boolean;
  /** Chat room for updates */
  roomId?: string;
  personaId?: string;
  personaName?: string;
  buildCommand?: string | null;
  testCommand?: string | null;
  codingModel?: string;
  maxBudgetUsd?: number;
}

export interface DevFixBugResult extends CommandResult {
  success: boolean;
  handle?: string;
  error?: string;
}

export const DevFixBug = {
  execute(params: CommandInput<DevFixBugParams>): Promise<DevFixBugResult> {
    return Commands.execute<DevFixBugParams, DevFixBugResult>('dev/fix-bug', params as Partial<DevFixBugParams>);
  },
  commandName: 'dev/fix-bug' as const,
} as const;
