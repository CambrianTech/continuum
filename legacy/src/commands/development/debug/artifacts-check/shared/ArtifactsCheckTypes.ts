/**
 * Debug command to check if ArtifactsDaemon is working
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating DevelopmentDebugArtifactsCheckParams
 */
export const createDevelopmentDebugArtifactsCheckParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ArtifactsCheckParams, 'context' | 'sessionId' | 'userId'>
): ArtifactsCheckParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating DevelopmentDebugArtifactsCheckResult with defaults
 */
export const createDevelopmentDebugArtifactsCheckResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ArtifactsCheckResult, 'context' | 'sessionId' | 'userId'>
): ArtifactsCheckResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart development/debug/artifacts-check-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentDebugArtifactsCheckResultFromParams = (
  params: ArtifactsCheckParams,
  differences: Omit<ArtifactsCheckResult, 'context' | 'sessionId' | 'userId'>
): ArtifactsCheckResult => transformPayload(params, differences);

