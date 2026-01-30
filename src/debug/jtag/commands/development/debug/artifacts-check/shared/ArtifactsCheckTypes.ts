/**
 * Debug command to check if ArtifactsDaemon is working
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface ArtifactsCheckParams extends CommandParams {
  testFile?: string;  // Optional file to test reading
}

export interface ArtifactsCheckResult extends CommandResult {
  daemonFound: boolean;
  daemonName?: string;
  testResult?: {
    operation: string;
    success: boolean;
    data?: string;
    error?: string;
  };
  systemInfo: {
    totalDaemons: number;
    daemonList: string[];
  };
  context: import('@system/core/types/JTAGTypes').JTAGContext;
  sessionId: string;
}

/**
 * ArtifactsCheck — Type-safe command executor
 *
 * Usage:
 *   import { ArtifactsCheck } from '...shared/ArtifactsCheckTypes';
 *   const result = await ArtifactsCheck.execute({ ... });
 */
export const ArtifactsCheck = {
  execute(params: CommandInput<ArtifactsCheckParams>): Promise<ArtifactsCheckResult> {
    return Commands.execute<ArtifactsCheckParams, ArtifactsCheckResult>('development/debug/artifacts-check', params as Partial<ArtifactsCheckParams>);
  },
  commandName: 'development/debug/artifacts-check' as const,
} as const;
