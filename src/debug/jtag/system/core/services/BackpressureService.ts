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
   * Can be used for adaptive behavior beyond simple proceed/don't proceed:
   * - Adjust batch sizes based on load
   * - Delay operations proportional to load
   * - Show load indicators in UI
   */
  static getLoad(): number {
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
   * Check if system is under high load
   * Convenience method for quick checks
   */
  static isHighLoad(): boolean {
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
