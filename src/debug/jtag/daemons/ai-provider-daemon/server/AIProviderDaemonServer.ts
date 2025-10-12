/**
 * AI Provider Daemon Server - Server-specific AI Integration
 * ===========================================================
 *
 * Server implementation of AIProviderDaemon with full access to:
 * - HTTP requests (fetch API)
 * - File system (for config/cache)
 * - Environment variables (for API keys)
 * - ProcessPool for genome inference workers
 *
 * All AI provider logic is in the shared AIProviderDaemon base class.
 * This server version provides daemon registration and ProcessPool initialization.
 */

import { AIProviderDaemon } from '../shared/AIProviderDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { ProcessPool } from '../../../system/genome/server/ProcessPool';
import { initializeSecrets } from '../../../system/secrets/SecretManager';
import * as path from 'path';

export class AIProviderDaemonServer extends AIProviderDaemon {
  private processPool?: ProcessPool;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Override to return typed ProcessPool instance
   */
  protected getProcessPoolInstance(): ProcessPool | undefined {
    return this.processPool;
  }

  /**
   * Server-specific initialization
   * Initializes base daemon, ProcessPool, and registers static interface
   */
  protected async initialize(): Promise<void> {
    // Initialize SecretManager FIRST (adapters depend on it)
    console.log('🔐 AIProviderDaemonServer: Initializing SecretManager...');
    await initializeSecrets();
    console.log('✅ AIProviderDaemonServer: SecretManager initialized');

    await super['initialize']();

    // DISABLED: ProcessPool adds 40s overhead - direct Ollama adapter is 132x faster
    // ProcessPool with HTTP workers → 41s per request
    // Direct Ollama adapter → 310ms per request
    console.log('🤖 AIProviderDaemonServer: Using direct Ollama adapter (no ProcessPool)');

    // Initialize static AIProviderDaemon interface for commands to use (like DataDaemon.query)
    AIProviderDaemon.initialize(this);

    console.log(`🤖 ${this.toString()}: AI provider daemon server initialized with direct adapters (no ProcessPool)`);
  }

  /**
   * Server-specific shutdown
   * Shuts down ProcessPool gracefully, then delegates to base class
   */
  async shutdown(): Promise<void> {
    console.log('🔄 AIProviderDaemonServer: Shutting down ProcessPool...');

    if (this.processPool) {
      await this.processPool.shutdown();
      console.log('✅ AIProviderDaemonServer: ProcessPool shutdown complete');
    }

    await super.shutdown();
  }
}
