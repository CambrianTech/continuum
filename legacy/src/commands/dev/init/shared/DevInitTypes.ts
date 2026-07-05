/**
 * dev/init — Detect project type and verify dev pipeline setup.
 *
 * Usage:
 *   ./jtag dev/init --cwd="."
 *   ./jtag dev/init --repo="/path/to/project"
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface DevInitParams extends CommandParams {
  /** Path to repo (default: cwd) */
  repo?: string;
  /** Alias for repo */
  cwd?: string;
}

export interface DevInitResult extends CommandResult {
  success: boolean;
  projectType: string;
  buildCommand: string | null;
  testCommand: string | null;
  hasClaudeMd: boolean;
  availableTemplates: string[];
}

export const DevInit = {
  execute(params?: CommandInput<DevInitParams>): Promise<DevInitResult> {
    return Commands.execute<DevInitParams, DevInitResult>('dev/init', params as Partial<DevInitParams>);
  },
  commandName: 'dev/init' as const,
} as const;
