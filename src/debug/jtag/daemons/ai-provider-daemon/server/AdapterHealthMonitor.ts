/**
 * AdapterHealthMonitor - Event-Driven Adapter Health Monitoring
 *
 * Replaces per-adapter setInterval with centralized, concurrent-safe health monitoring.
 * Uses SystemDaemon for configuration (no setTimeout violations).
 *
 * Architecture:
 * - Single monitor for ALL adapters (no setInterval spam)
 * - Event-driven health checks (triggered by system events)
 * - Configuration via SystemConfigEntity (runtime tunable)
 * - Concurrent-safe (proper locking, no race conditions)
 * - Adaptive cadence based on system load
 */

import type { AIProviderAdapter, HealthStatus } from '../shared/AIProviderTypesV2';
import { SystemDaemon } from '../../system-daemon/shared/SystemDaemon';
import { Events } from '../../../system/core/shared/Events';
import { Logger } from '../../../system/core/logging/Logger';

const log = Logger.create('AdapterHealthMonitor', 'adapters');

interface AdapterHealthState {
  adapter: AIProviderAdapter;
  lastCheckTime: number;
  lastStatus: HealthStatus | null;
  consecutiveFailures: number;
  isChecking: boolean; // Lock to prevent concurrent checks
}

/**
 * AdapterHealthMonitor - Centralized health monitoring for all AI adapters
 *
 * Pattern: Event-driven monitoring instead of setInterval
 * - Responds to system events (system:health-check:tick)
 * - Uses SystemDaemon for configuration
 * - Concurrent-safe with per-adapter locks
 */
export class AdapterHealthMonitor {
  private static instance: AdapterHealthMonitor | null = null;
  private adapters: Map<string, AdapterHealthState> = new Map();
  private isRunning: boolean = false;
  private lastTickTime: number = 0;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get singleton instance
   */
  static getInstance(): AdapterHealthMonitor {
    if (!AdapterHealthMonitor.instance) {
      AdapterHealthMonitor.instance = new AdapterHealthMonitor();
    }
    return AdapterHealthMonitor.instance;
  }

  /**
   * Initialize health monitoring
   * Call this after all adapters are registered
   */
  async initialize(): Promise<void> {
    if (this.isRunning) {
      log.warn('AdapterHealthMonitor already running');
      return;
    }

    log.info('💚 Initializing AdapterHealthMonitor...');

    // Subscribe to health check events (main monitoring loop)
    await Events.subscribe('system:health-check:tick', async () => {
      await this.performHealthChecks();
    });

    // Subscribe to unhealthy adapter events (restart handling)
    await Events.subscribe('system:adapter:unhealthy', async (data: any) => {
      await this.handleUnhealthyAdapter(data.providerId);
    });

    this.isRunning = true;
    log.info('✅ AdapterHealthMonitor initialized');

    // Don't perform initial health check synchronously - it blocks main thread
    // The first system:health-check:tick event will trigger health checks
    // This prevents initialization order issues with SystemDaemon
  }

  /**
   * Handle unhealthy adapter - trigger restart
   */
  private async handleUnhealthyAdapter(providerId: string): Promise<void> {
    const state = this.adapters.get(providerId);
    if (!state) {
      log.warn(`Cannot restart unknown adapter: ${providerId}`);
      return;
    }

    log.warn(`🔄 Triggering restart for unhealthy adapter: ${providerId}`);

    try {
      if (state.adapter.handleRestartRequest) {
        await state.adapter.handleRestartRequest();
      } else {
        log.warn(`Adapter ${providerId} does not support automatic restart`);
      }
    } catch (error) {
      log.error(`Failed to restart adapter ${providerId}:`, error);
    }
  }

  /**
   * Register an adapter for health monitoring
   */
  registerAdapter(adapter: AIProviderAdapter): void {
    if (this.adapters.has(adapter.providerId)) {
      log.warn(`Adapter ${adapter.providerId} already registered`);
      return;
    }

    this.adapters.set(adapter.providerId, {
      adapter,
      lastCheckTime: 0,
      lastStatus: null,
      consecutiveFailures: 0,
      isChecking: false,
    });

    log.info(`📝 Registered adapter: ${adapter.providerId}`);
  }

  /**
   * Unregister an adapter from health monitoring
   */
  unregisterAdapter(providerId: string): void {
    this.adapters.delete(providerId);
    log.info(`🗑️  Unregistered adapter: ${providerId}`);
  }

  /**
   * Perform health checks on all adapters
   * Concurrent-safe: each adapter has a lock to prevent overlapping checks
   */
  private async performHealthChecks(): Promise<void> {
    const now = Date.now();

    // Get health check interval from SystemDaemon
    const systemDaemon = SystemDaemon.sharedInstance();
    const checkInterval = systemDaemon.getSetting('system/scheduling/timings/adapter-health-check') as number ?? 30000;

    // Check if enough time has passed since last tick
    if (now - this.lastTickTime < checkInterval) {
      return; // Too soon, skip this tick
    }

    this.lastTickTime = now;
    log.debug(`🔍 Performing health checks (${this.adapters.size} adapters)...`);

    // Check all adapters concurrently (each has its own lock)
    const checks = Array.from(this.adapters.values()).map(state =>
      this.checkAdapter(state, now, checkInterval)
    );

    await Promise.allSettled(checks);
  }

  /**
   * Check a single adapter's health
   * Concurrent-safe with per-adapter lock
   */
  private async checkAdapter(
    state: AdapterHealthState,
    now: number,
    checkInterval: number
  ): Promise<void> {
    // Lock: Skip if already checking
    if (state.isChecking) {
      log.debug(`⏭️  Skipping ${state.adapter.providerId} (check in progress)`);
      return;
    }

    // Check if enough time has passed since last check
    if (now - state.lastCheckTime < checkInterval) {
      return; // Too soon for this adapter
    }

    // Acquire lock
    state.isChecking = true;

    try {
      log.debug(`🔍 Checking ${state.adapter.providerId}...`);
      const health = await state.adapter.healthCheck();

      // Update state
      state.lastCheckTime = now;
      state.lastStatus = health;

      if (health.status === 'healthy') {
        if (state.consecutiveFailures > 0) {
          log.info(`✅ ${state.adapter.providerId}: Recovered after ${state.consecutiveFailures} failures`);
        }
        state.consecutiveFailures = 0;
      } else {
        state.consecutiveFailures++;
        log.warn(`⚠️  ${state.adapter.providerId}: Health check failed (${state.consecutiveFailures} consecutive failures)`);

        // Get max failures threshold from SystemDaemon
        const systemDaemon = SystemDaemon.sharedInstance();
        const maxFailures = systemDaemon.getSetting('system/scheduling/policies/adapter-max-consecutive-failures') as number ?? 3;

        if (state.consecutiveFailures >= maxFailures) {
          // Emit event for adapter restart (handled externally)
          await Events.emit('system:adapter:unhealthy', {
            providerId: state.adapter.providerId,
            consecutiveFailures: state.consecutiveFailures,
            lastStatus: health,
          });
        }
      }
    } catch (error) {
      state.consecutiveFailures++;
      log.error(`❌ ${state.adapter.providerId}: Health check error (${state.consecutiveFailures} consecutive failures):`, error);
    } finally {
      // Release lock
      state.isChecking = false;
    }
  }

  /**
   * Get current health status for all adapters
   */
  getHealthStatus(): Map<string, HealthStatus | null> {
    const status = new Map<string, HealthStatus | null>();
    for (const [providerId, state] of this.adapters) {
      status.set(providerId, state.lastStatus);
    }
    return status;
  }

  /**
   * Shutdown health monitoring
   */
  async shutdown(): Promise<void> {
    log.info('🛑 Shutting down AdapterHealthMonitor...');
    this.isRunning = false;
    this.adapters.clear();
  }
}
