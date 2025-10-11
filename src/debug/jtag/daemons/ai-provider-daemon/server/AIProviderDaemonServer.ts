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

    // Initialize ProcessPool for genome inference workers
    console.log('🏊 AIProviderDaemonServer: Initializing ProcessPool for genome inference...');

    // Worker script needs to be run with tsx since it's TypeScript
    // Path to source .ts file (tsx will handle execution)
    // Use absolute path from project root to avoid __dirname confusion after compilation
    const workerPath = path.resolve(process.cwd(), 'system/genome/server/inference-worker.ts');
    console.log(`🔧 AIProviderDaemonServer: Worker path resolved to: ${workerPath}`);
    this.processPool = new ProcessPool(workerPath, {
      hotPoolSize: 3,
      warmPoolSize: 10,
      minProcesses: 1,
      maxProcesses: 10,
      healthCheckIntervalMs: 5000,
      maxIdleTimeMs: 60000,
      maxMemoryMB: 2048,
      maxRequestsPerProcess: 1000,
      maxErrorsBeforeTerminate: 5,
      processTimeoutMs: 30000,
    });

    await this.processPool.initialize();
    console.log('✅ AIProviderDaemonServer: ProcessPool initialized');

    // Initialize static AIProviderDaemon interface for commands to use (like DataDaemon.query)
    AIProviderDaemon.initialize(this);

    console.log(`🤖 ${this.toString()}: AI provider daemon server initialized with static interface + ProcessPool`);
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
