/**
 * Server Daemon Base - Server-only daemon foundation with automatic logging
 *
 * Extends DaemonBase with server-specific functionality:
 * - Automatic file-based logging (Logger.create)
 * - Self-healing infrastructure (rate limiting, health monitoring)
 * - Concurrency primitives (optional opt-in)
 */

import { DaemonBase } from '../shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { Logger } from '../../../system/core/logging/Logger';
import { RateLimiter, AsyncQueue, Semaphore, DaemonMetrics } from '../../../generator/DaemonConcurrency';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

export abstract class ServerDaemonBase extends DaemonBase {
  // OPTIONAL: Concurrency primitives (daemons can opt-in)
  protected rateLimiter?: RateLimiter;
  protected requestQueue?: AsyncQueue<BaseResponsePayload>;
  protected semaphore?: Semaphore;
  protected metrics?: DaemonMetrics;

  // OPTIONAL: Health monitoring state (daemons can opt-in)
  protected healthState?: {
    isHealthy: boolean;
    consecutiveFailures: number;
    lastSuccessTime: number;
    lastHeartbeat: number;
  };

  private heartbeatTimer?: NodeJS.Timeout;

  constructor(name: string, context: JTAGContext, router: JTAGRouter) {
    super(name, context, router);

    // Automatically set up file-based logging for all server daemons
    // Use actual class name (e.g., 'AIProviderDaemonServer') for clarity
    // Logs go to .continuum/.../logs/daemons/{ClassName}.log
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Cleanup resources when daemon shuts down
   * Override in subclasses if cleanup is needed
   */
  async shutdown(): Promise<void> {
    this.log.info(`🔄 ${this.toString()}: Shutting down...`);

    // Stop heartbeat if running
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = undefined;
    }
  }

  // ===========================================================================
  // OPTIONAL SELF-HEALING HELPERS (Daemons can opt-in)
  // ===========================================================================

  /**
   * Initialize concurrency primitives
   * Call this in child daemon constructor to enable concurrency control
   */
  protected initializeConcurrency(
    rateLimit: { maxTokens: number; refillRate: number } = { maxTokens: 100, refillRate: 10 },
    maxConcurrent: number = 10
  ): void {
    this.rateLimiter = new RateLimiter(rateLimit.maxTokens, rateLimit.refillRate);
    this.requestQueue = new AsyncQueue<BaseResponsePayload>();
    this.semaphore = new Semaphore(maxConcurrent);
    this.metrics = new DaemonMetrics();
  }

  /**
   * Initialize health monitoring
   * Call this in child daemon initialize() to enable self-healing
   */
  protected initializeHealthMonitoring(heartbeatInterval: number = 10000): void {
    this.healthState = {
      isHealthy: true,
      consecutiveFailures: 0,
      lastSuccessTime: Date.now(),
      lastHeartbeat: Date.now()
    };

    this.heartbeatTimer = setInterval(() => {
      if (!this.healthState) return;

      this.healthState.lastHeartbeat = Date.now();

      // Check if daemon is stuck (no successful operations in 60s)
      const timeSinceSuccess = Date.now() - this.healthState.lastSuccessTime;
      if (timeSinceSuccess > 60000) {
        this.log.warn(`⚠️  ${this.toString()}: Daemon appears stuck (${Math.round(timeSinceSuccess / 1000)}s since last success)`);
        this.healthState.isHealthy = false;
        void this.attemptSelfHeal();
      }
    }, heartbeatInterval);
  }

  /**
   * Record successful operation (updates health state)
   */
  protected recordSuccess(): void {
    if (!this.healthState) return;

    this.healthState.consecutiveFailures = 0;
    this.healthState.lastSuccessTime = Date.now();
  }

  /**
   * Record failed operation (updates health state, triggers self-heal if needed)
   */
  protected recordFailure(): void {
    if (!this.healthState) return;

    this.healthState.consecutiveFailures++;

    // Trigger self-heal after 5 consecutive failures
    if (this.healthState.consecutiveFailures >= 5) {
      this.healthState.isHealthy = false;
      this.log.warn(`⚠️  ${this.toString()}: Too many failures (${this.healthState.consecutiveFailures}), triggering self-heal...`);
      void this.attemptSelfHeal();
    }
  }

  /**
   * Attempt to self-heal/restart daemon
   */
  private async attemptSelfHeal(): Promise<void> {
    this.log.warn(`🔧 ${this.toString()}: Attempting self-heal/restart...`);

    try {
      // Shutdown
      await this.shutdown();

      // Wait a moment
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Reinitialize
      await this.initialize();

      // Reset health state
      if (this.healthState) {
        this.healthState.isHealthy = true;
        this.healthState.consecutiveFailures = 0;
        this.healthState.lastSuccessTime = Date.now();
      }

      this.log.info(`✅ ${this.toString()}: Self-heal successful`);
    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      this.log.error(`❌ ${this.toString()}: Self-heal failed: ${errorMessage}`);
      // Stay unhealthy - system admin needs to intervene
    }
  }

  /**
   * Get metrics snapshot (if metrics enabled)
   */
  getMetrics() {
    if (!this.metrics) {
      return null;
    }

    return {
      ...this.metrics.getSnapshot(
        this.requestQueue?.size ?? 0,
        (this.semaphore?.maxPermits ?? 0) - (this.semaphore?.available ?? 0)
      ),
      healthState: this.healthState ? { ...this.healthState } : null
    };
  }
}
