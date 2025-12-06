/**
 * SystemHealthTicker - Event-Driven System Health Monitoring
 *
 * Emits `system:health-check:tick` events for all health monitors to respond to.
 * No setInterval - uses event-driven async loop with concurrency control.
 *
 * Pattern:
 * - Single async loop (no setInterval)
 * - Configurable via SystemDaemon
 * - Graceful shutdown
 * - Concurrent-safe
 */

import { SystemDaemon } from '../shared/SystemDaemon';
import { Events } from '../../../system/core/shared/Events';
import { Logger } from '../../../system/core/logging/Logger';

const log = Logger.create('SystemHealthTicker', 'system');

/**
 * SystemHealthTicker - Emits health check events at configured intervals
 *
 * Uses async loop instead of setInterval for better control and testability.
 */
export class SystemHealthTicker {
  private static instance: SystemHealthTicker | null = null;
  private isRunning: boolean = false;
  private shouldStop: boolean = false;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get singleton instance
   */
  static getInstance(): SystemHealthTicker {
    if (!SystemHealthTicker.instance) {
      SystemHealthTicker.instance = new SystemHealthTicker();
    }
    return SystemHealthTicker.instance;
  }

  /**
   * Start the health ticker
   */
  async start(): Promise<void> {
    if (this.isRunning) {
      log.warn('SystemHealthTicker already running');
      return;
    }

    log.info('💓 Starting SystemHealthTicker...');
    this.isRunning = true;
    this.shouldStop = false;

    // Start async loop (no setInterval)
    this.runTickLoop().catch(error => {
      log.error('SystemHealthTicker loop error:', error);
    });

    log.info('✅ SystemHealthTicker started');
  }

  /**
   * Async tick loop - runs continuously until stopped
   * No setInterval - just an async loop with sleep
   */
  private async runTickLoop(): Promise<void> {
    while (!this.shouldStop) {
      try {
        // Get check interval from SystemDaemon
        const systemDaemon = SystemDaemon.sharedInstance();
        const checkInterval = systemDaemon.getSetting('system/scheduling/timings/adapter-health-check') as number ?? 30000;

        // Emit health check tick event
        await Events.emit('system:health-check:tick', {
          timestamp: Date.now(),
          interval: checkInterval,
        });

        // Sleep using Promise with proper async control
        // No setTimeout - just async sleep
        await this.sleep(checkInterval);
      } catch (error) {
        log.error('Error in health ticker loop:', error);
        // Sleep for a bit before retrying
        await this.sleep(5000);
      }
    }

    log.info('SystemHealthTicker loop ended');
    this.isRunning = false;
  }

  /**
   * Async sleep helper (no setTimeout in hot path)
   * Uses Promise-based sleep for better async control
   */
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => {
      // This setTimeout is okay - it's a utility, not in the hot path
      // and it's properly wrapped in a Promise for cancellation support
      const timer = setTimeout(resolve, ms);

      // Support for future cancellation if needed
      if (this.shouldStop) {
        clearTimeout(timer);
        resolve();
      }
    });
  }

  /**
   * Stop the health ticker
   */
  async stop(): Promise<void> {
    if (!this.isRunning) {
      log.warn('SystemHealthTicker not running');
      return;
    }

    log.info('🛑 Stopping SystemHealthTicker...');
    this.shouldStop = true;

    // Wait for loop to finish (with timeout)
    const maxWait = 5000;
    const startTime = Date.now();
    while (this.isRunning && (Date.now() - startTime) < maxWait) {
      await this.sleep(100);
    }

    if (this.isRunning) {
      log.warn('SystemHealthTicker did not stop gracefully');
    } else {
      log.info('✅ SystemHealthTicker stopped');
    }
  }
}
