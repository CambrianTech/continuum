/**
 * AI Status Command - Shared Logic
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIStatusParams, AIStatusResult } from './AIStatusTypes';

export abstract class AIStatusCommand extends CommandBase<AIStatusParams, AIStatusResult> {
  constructor(
    name: string,
    context: AIStatusParams['context'],
    subpath: string,
    commander: ICommandDaemon
  ) {
    super(name, context, subpath, commander);
  }

  // Shared validation logic
  protected validateParams(params: AIStatusParams): string | null {
    // No required params - all optional filters
    return null;
  }

  // Shared helper: Classify persona health status
  protected classifyHealthStatus(
    isInitialized: boolean,
    isSubscribed: boolean,
    hasWorker: boolean,
    errorCount: number,
    timeSinceLastResponse?: number
  ): 'healthy' | 'starting' | 'degraded' | 'dead' {
    // Dead: Not initialized or major errors
    if (!isInitialized || errorCount > 10) {
      return 'dead';
    }

    // Starting: Initialized but not fully operational
    if (!isSubscribed || !hasWorker) {
      return 'starting';
    }

    // Degraded: No recent responses or some errors
    if (timeSinceLastResponse && timeSinceLastResponse > 300000) { // 5 minutes
      return 'degraded';
    }

    if (errorCount > 3) {
      return 'degraded';
    }

    // Healthy: Fully operational
    return 'healthy';
  }
}
