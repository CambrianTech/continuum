/**
 * Debug command to check if ArtifactsDaemon is working
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

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
