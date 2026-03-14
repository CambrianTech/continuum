/**
 * dev/code-review — Shorthand for sentinel/run --template=dev/code-review
 *
 * Usage:
 *   ./jtag dev/code-review --branch="feature/user-profiles"
 *   ./jtag dev/code-review  (reviews HEAD vs main)
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface DevCodeReviewParams extends CommandParams {
  /** Branch to review (default: HEAD) */
  branch?: string;
  /** Base branch for diff (default: main) */
  baseBranch?: string;
  /** Working directory */
  cwd?: string;
  /** Skip collaborative checkpoints */
  autonomous?: boolean;
  roomId?: string;
  personaId?: string;
  personaName?: string;
}

export interface DevCodeReviewResult extends CommandResult {
  success: boolean;
  handle?: string;
  error?: string;
}

export const DevCodeReview = {
  execute(params?: CommandInput<DevCodeReviewParams>): Promise<DevCodeReviewResult> {
    return Commands.execute<DevCodeReviewParams, DevCodeReviewResult>('dev/code-review', params as Partial<DevCodeReviewParams>);
  },
  commandName: 'dev/code-review' as const,
} as const;
