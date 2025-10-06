/**
 * AI Provider Daemon Server - Server-specific AI Integration
 * ===========================================================
 *
 * Server implementation of AIProviderDaemon with full access to:
 * - HTTP requests (fetch API)
 * - File system (for config/cache)
 * - Environment variables (for API keys)
 *
 * All AI provider logic is in the shared AIProviderDaemon base class.
 * This server version just provides the daemon registration interface.
 */

import { AIProviderDaemon } from '../shared/AIProviderDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class AIProviderDaemonServer extends AIProviderDaemon {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Server-specific initialization
   * Initializes base daemon and registers static interface
   */
  protected async initialize(): Promise<void> {
    await super['initialize']();

    // Initialize static AIProviderDaemon interface for commands to use (like DataDaemon.query)
    AIProviderDaemon.initialize(this);

    console.log(`🤖 ${this.toString()}: AI provider daemon server initialized with static interface`);
  }

  /**
   * Server-specific shutdown (if needed)
   * Currently delegates to base class
   */
  async shutdown(): Promise<void> {
    await super.shutdown();
  }
}
