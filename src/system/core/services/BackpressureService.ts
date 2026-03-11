/**
 * BackpressureService - Adaptive Load Management for AI Operations
 *
 * RTOS-inspired backpressure that allows callers to check system load
 * before making requests. Like a network router, the system adapts to
 * congestion by signaling callers to back off.
 *
 * Key principles:
 * - NO hardcoded sleeps or delays
 * - Query actual queue load from Candle adapter
 * - Callers decide whether to proceed based on current load
 * - Adaptive: when queue clears, traffic resumes automatically
 *
 * Usage:
 *   if (!BackpressureService.shouldProceed('low')) {
 *     return; // Skip this operation, try again next tick
 *   }
 *   await performExpensiveOperation();
 */

import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type { PressureLevel } from '../../../shared/generated/system';
import type { PressureSnapshotInfo } from '../../../workers/continuum-core/bindings/modules/system_resources';

/**
 * Priority levels for operations
 * Higher priority = more likely to proceed under load
 */
export type OperationPriority = 'critical' | 'high' | 'normal' | 'low' | 'background';

/**
 * Queue statistics from Candle adapter
 */
interface QueueStats {
  queueSize: number;
  activeRequests: number;
  maxConcurrent: number;
  load: number; // 0.0 to 1.0+
}

/**
 * Load thresholds per priority level
 * When queue load exceeds threshold, operation should not proceed
 */
const LOAD_THRESHOLDS: Record<OperationPriority, number> = {
  critical: 2.0,   // Always proceed unless queue is overwhelmed (2x capacity)
  high: 1.5,       // Proceed up to 1.5x capacity
  normal: 1.0,     // Proceed up to capacity
  low: 0.7,        // Back off when 70% loaded
  background: 0.3  // Only proceed when mostly idle
};

/**
 * BackpressureService - Adaptive load management singleton
 *
 * Queries Candle queue stats and provides shouldProceed() decision
 */
export class BackpressureService {
  private static cachedStats: QueueStats | null = null;
  private static cacheTimestamp: number = 0;
  private static readonly CACHE_TTL_MS = 100; // Refresh every 100ms (fast enough to adapt)

  /** Graduated pressure state — cached to avoid IPC on every check */
  private static _pressureLevel: PressureLevel = 'normal';
  private static _pressureSnapshot: PressureSnapshotInfo | null = null;
  private static pressureTimestamp: number = 0;
  private static readonly PRESSURE_TTL_MS = 2000; // Check every 2s (matches Rust poll interval)

  /**
   * Check if an operation should proceed based on current system load
   *
   * @param priority - Operation priority (default: 'normal')
   * @returns true if operation should proceed, false if should back off
   *
   * @example
   * // In SemanticCompressionAdapter before LLM synthesis:
   * if (!BackpressureService.shouldProceed('low')) {
   *   return this.createFallbackMemory(group, context);
   * }
   * const synthesis = await this.persona.generateText({ ... });
   *
   * @example
   * // In EmbeddingService before embedding generation:
   * if (!BackpressureService.shouldProceed('background')) {
   *   return entity; // Skip embedding, try again later
   * }
   */
  static shouldProceed(priority: OperationPriority = 'normal'): boolean {
    const stats = this.getQueueStats();
    if (!stats) {
      // Can't get stats - assume OK to proceed (fail-open)
      return true;
    }

    const threshold = LOAD_THRESHOLDS[priority];
    const proceed = stats.load < threshold;

    // Log when backing off (helps debug)
    if (!proceed) {
      console.log(`🚦 Backpressure: Blocking ${priority} operation (load=${stats.load.toFixed(2)}, threshold=${threshold})`);
    }

    return proceed;
  }

  /**
   * Get current load level (0.0 to 1.0+)
   *
   * Combines Candle queue load with Rust memory pressure gate.
   * When memory gate is closed, returns 2.0 (emergency) regardless of queue.
   */
  static getLoad(): number {
    // Memory gate overrides everything — if Rust says we're OOM, stop.
    if (this.isMemoryGateClosed()) {
      return 2.0;
    }
    const stats = this.getQueueStats();
    return stats?.load ?? 0;
  }

  /**
   * Get full queue statistics
   * Useful for diagnostics and logging
   */
  static getStats(): QueueStats | null {
    return this.getQueueStats();
  }

  /**
   * Check if system is under high load.
   * True when Candle queue exceeds threshold OR Rust memory gate is closed.
   */
  static isHighLoad(): boolean {
    if (this.isMemoryGateClosed()) return true;
    return this.getLoad() > LOAD_THRESHOLDS.normal;
  }

  /**
   * Check if system is mostly idle
   * Good time for background operations
   */
  static isIdle(): boolean {
    return this.getLoad() < LOAD_THRESHOLDS.background;
  }

  /**
   * Current graduated pressure level from Rust (cached, non-blocking).
   * Subsystems read this to self-manage under pressure.
   */
  static get pressureLevel(): PressureLevel {
    this.refreshPressure();
    return this._pressureLevel;
  }

  /**
   * Full pressure snapshot (level, pressure ratio, RSS, consecutive count).
   */
  static get pressureSnapshot(): PressureSnapshotInfo {
    this.refreshPressure();
    return this._pressureSnapshot ?? { level: 'normal' as PressureLevel, pressure: 0, normalizedPressure: 0, rssBytes: 0, consecutiveAtLevel: 0 };
  }

  /**
   * Normalized pressure (0.0 - 1.0) where 0 = no concern, 1 = emergency.
   * Maps the action zone (80-95% system memory) to a clean 0-1 range.
   * Use this instead of raw pressure for proportional responses.
   */
  static get normalizedPressure(): number {
    this.refreshPressure();
    return this._pressureSnapshot?.normalizedPressure ?? 0;
  }

  /**
   * Check Rust memory gate (derived from graduated pressure).
   * The gate closes when system memory pressure is critical for 3+ consecutive polls.
   */
  private static isMemoryGateClosed(): boolean {
    this.refreshPressure();
    return this._pressureLevel === 'critical' && (this._pressureSnapshot?.consecutiveAtLevel ?? 0) >= 3;
  }

  /**
   * Fire-and-forget pressure refresh — non-blocking, stale cache is fine for 2s.
   */
  private static refreshPressure(): void {
    const now = Date.now();

    if ((now - this.pressureTimestamp) < this.PRESSURE_TTL_MS) {
      return;
    }

    // Prevent concurrent refreshes
    this.pressureTimestamp = now;
    RustCoreIPCClient.getInstanceAsync()
      .then(client => client.pressureSnapshot())
      .then(snapshot => {
        const prev = this._pressureLevel;
        this._pressureLevel = snapshot.level;
        this._pressureSnapshot = snapshot;
        if (snapshot.level !== 'normal' && snapshot.level !== prev) {
          console.log(`🚦 BackpressureService: Pressure ${prev} → ${snapshot.level} (normalized=${snapshot.normalizedPressure.toFixed(2)}, raw=${(snapshot.pressure * 100).toFixed(1)}%, RSS=${Math.round(snapshot.rssBytes / 1024 / 1024)}MB, consecutive=${snapshot.consecutiveAtLevel})`);
        }
      })
      .catch(() => {
        // IPC unavailable — assume normal (fail-open)
        this._pressureLevel = 'normal';
        this._pressureSnapshot = null;
      });
  }

  /**
   * Get queue stats from Candle adapter with caching
   * Cache prevents hammering the adapter on every check
   */
  private static getQueueStats(): QueueStats | null {
    const now = Date.now();

    // Return cached stats if fresh
    if (this.cachedStats && (now - this.cacheTimestamp) < this.CACHE_TTL_MS) {
      return this.cachedStats;
    }

    try {
      // Get Candle adapter from AIProviderDaemon
      const adapter = AIProviderDaemon.getAdapter('candle');
      if (!adapter) {
        return null;
      }

      // Check if adapter has getQueueStats method
      if (typeof (adapter as any).getQueueStats !== 'function') {
        return null;
      }

      // Get fresh stats
      const stats = (adapter as any).getQueueStats() as QueueStats;
      this.cachedStats = stats;
      this.cacheTimestamp = now;

      return stats;
    } catch (error) {
      // Fail silently - don't break callers if stats unavailable
      return null;
    }
  }
}
