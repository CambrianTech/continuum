/**
 * dev/integrate — Merge persona branches into a feature/integration branch.
 *
 * Usage:
 *   ./jtag dev/integrate --featureBranch="feature/auth" --cwd="."
 *   ./jtag dev/integrate --featureBranch="feature/auth" --branches="ai/helper/middleware,ai/teacher/tests"
 *   ./jtag dev/integrate --featureBranch="feature/auth"  # auto-discovers all ai/* branches
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface DevIntegrateParams extends CommandParams {
  /** Target integration branch name */
  featureBranch: string;
  /** Comma-separated branch names to merge (auto-discovers ai/* if omitted) */
  branches?: string;
  /** Base branch (default: main) */
  baseBranch?: string;
  /** Project working directory */
  cwd?: string;
  /** Build command (null to skip) */
  buildCommand?: string;
  /** Test command (null to skip) */
  testCommand?: string;
  /** CodingAgent model for conflict resolution */
  codingModel?: string;
  /** Max budget for conflict resolution */
  maxBudgetUsd?: number;
  /** Persona orchestrating */
  personaId?: string;
  /** Persona display name */
  personaName?: string;
  /** Chat room */
  roomId?: string;
  /** Skip chat reporting */
  autonomous?: boolean;
}

export interface DevIntegrateResult extends CommandResult {
  success: boolean;
  handle?: string;
  error?: string;
}

export const DevIntegrate = {
  execute(params?: CommandInput<DevIntegrateParams>): Promise<DevIntegrateResult> {
    return Commands.execute<DevIntegrateParams, DevIntegrateResult>('dev/integrate', params as Partial<DevIntegrateParams>);
  },
  commandName: 'dev/integrate' as const,
} as const;
