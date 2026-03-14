/**
 * dev/build-feature — Shorthand for sentinel/run --template=dev/build-feature
 *
 * Usage:
 *   ./jtag dev/build-feature --feature="Add user profiles" --cwd="."
 *   ./jtag dev/build-feature --feature="Implement search" --autonomous=true
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface DevBuildFeatureParams extends CommandParams {
  /** Feature description (natural language) */
  feature: string;
  /** Working directory (default: cwd) */
  cwd?: string;
  /** Skip collaborative checkpoints */
  autonomous?: boolean;
  /** Chat room for updates */
  roomId?: string;
  /** Persona ID (auto-detected if omitted) */
  personaId?: string;
  /** Persona name */
  personaName?: string;
  /** Build command (null to skip) */
  buildCommand?: string | null;
  /** Test command (null to skip) */
  testCommand?: string | null;
  /** CodingAgent model */
  codingModel?: string;
  /** Max budget in USD */
  maxBudgetUsd?: number;
}

export interface DevBuildFeatureResult extends CommandResult {
  success: boolean;
  handle?: string;
  error?: string;
}

export const DevBuildFeature = {
  execute(params: CommandInput<DevBuildFeatureParams>): Promise<DevBuildFeatureResult> {
    return Commands.execute<DevBuildFeatureParams, DevBuildFeatureResult>(
      'dev/build-feature',
      params as Partial<DevBuildFeatureParams>,
    );
  },
  commandName: 'dev/build-feature' as const,
} as const;
