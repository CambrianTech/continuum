/**
 * System Heartbeat - Adaptive Cadence Discovery using Second-Order Dynamics
 *
 * PROBLEM: Fixed decision windows miss slow evaluators
 * SOLUTION: Learn the system's natural resonant frequency (cadence)
 *
 * ANALOGY: Like a mass-spring-damper system finding equilibrium
 * - Measures actual evaluation speeds
 * - Adapts smoothly using second-order dynamics
 * - No overshoot (critically damped)
 * - Converges to p95 evaluation time
 *
 * MATH:
 *   x''(t) + 2ζωₙx'(t) + ωₙ²x(t) = ωₙ²u(t)
 *   where:
 *     x(t) = adaptive window (output)
 *     u(t) = p95 evaluation time (target)
 *     ωₙ = natural frequency (response speed)
 *     ζ = damping ratio (1.0 = critical, no overshoot)
 *
 * RESULT: Window smoothly tracks p95, capturing 95% of evaluations
 */

import type { UUID } from '../../core/types/JTAGTypes';

/**
 * Evaluation speed statistics for a conversation context (room)
 */
export interface HeartbeatStats {
  /** Recent evaluation times (milliseconds) */
  samples: number[];

  /** Moving average */
  avgTime: number;

  /** 95th percentile (target cadence) */
  p95Time: number;

  /** 99th percentile (outlier detection) */
  p99Time: number;

  /** Standard deviation (variance indicator) */
  stdDev: number;
}

/**
 * System Heartbeat - Adaptive cadence discovery
 *
 * Uses second-order dynamics to smoothly track the natural resonant frequency
 * of AI evaluation speeds in a conversation context.
 *
 * @example
 * ```typescript
 * const heartbeat = new SystemHeartbeat(contextId);
 *
 * // Record each evaluation
 * heartbeat.recordEvaluation(personaId, 3200); // 3.2 seconds
 *
 * // Get adaptive window (updates every call)
 * const window = heartbeat.getAdaptiveCadence(deltaTime); // 5680ms
 *
 * // Check health
 * const stats = heartbeat.getStats();
 * console.log(`p95: ${stats.p95Time}ms, window: ${window}ms`);
 * ```
 */
export class SystemHeartbeat {
  private contextId: UUID;

  // Evaluation time samples (circular buffer, last 50)
  private samples: number[] = [];
  private maxSamples = 50;

  // Second-order dynamics state
  private position: number = 5000;  // Current adaptive window (ms)
  private velocity: number = 0;     // Rate of change (ms/s)

  // Tuning parameters (critically damped second-order system)
  private readonly frequency = 1.0;  // Natural frequency ωₙ (Hz)
  private readonly damping = 1.0;    // Damping ratio ζ (1.0 = critical)

  // Safety bounds
  private readonly minWindow = 1000;   // 1 second minimum
  private readonly maxWindow = 15000;  // 15 second maximum

  // Metrics
  private totalEvaluations = 0;
  private lastUpdateTime = Date.now();

  constructor(contextId: UUID) {
    this.contextId = contextId;
  }

  /**
   * Record an evaluation time
   *
   * @param personaId - AI that evaluated
   * @param durationMs - Time taken to evaluate (milliseconds)
   */
  recordEvaluation(personaId: UUID, durationMs: number): void {
    // Add to circular buffer
    this.samples.push(durationMs);
    if (this.samples.length > this.maxSamples) {
      this.samples.shift();
    }

    this.totalEvaluations++;
  }

  /**
   * Get adaptive cadence using second-order dynamics
   *
   * Updates internal state and returns smoothly-tracking window that
   * follows the p95 evaluation time with natural spring-damper behavior.
   *
   * @param dt - Delta time since last update (seconds, default: auto-calculate)
   * @returns Adaptive window in milliseconds
   */
  getAdaptiveCadence(dt?: number): number {
    // Need at least 5 samples to calculate meaningful statistics
    if (this.samples.length < 5) {
      return this.position; // Return current position
    }

    // Calculate delta time if not provided
    if (dt === undefined) {
      const now = Date.now();
      dt = (now - this.lastUpdateTime) / 1000; // Convert to seconds
      this.lastUpdateTime = now;
    }

    // Clamp dt to reasonable range
    dt = Math.max(0.001, Math.min(1.0, dt));

    // Calculate target (p95 evaluation time)
    const target = this.calculateP95();

    // Second-order dynamics update
    // Spring force toward target (proportional to distance)
    const springForce = this.frequency * this.frequency * (target - this.position);

    // Damping force (proportional to velocity)
    const dampingForce = 2 * this.damping * this.frequency * this.velocity;

    // Net acceleration
    const acceleration = springForce - dampingForce;

    // Integrate velocity and position (semi-implicit Euler)
    this.velocity += acceleration * dt;
    this.position += this.velocity * dt;

    // Clamp to safety bounds
    this.position = Math.max(this.minWindow, Math.min(this.maxWindow, this.position));

    // Clamp velocity to prevent instability
    const maxVelocity = 10000; // 10 seconds per second
    this.velocity = Math.max(-maxVelocity, Math.min(maxVelocity, this.velocity));

    return Math.round(this.position);
  }

  /**
   * Get current statistics
   */
  getStats(): HeartbeatStats {
    if (this.samples.length === 0) {
      return {
        samples: [],
        avgTime: this.position,
        p95Time: this.position,
        p99Time: this.position,
        stdDev: 0
      };
    }

    const sorted = [...this.samples].sort((a, b) => a - b);
    const sum = this.samples.reduce((a, b) => a + b, 0);
    const avg = sum / this.samples.length;

    // Calculate standard deviation
    const variance = this.samples.reduce((sum, x) => sum + Math.pow(x - avg, 2), 0) / this.samples.length;
    const stdDev = Math.sqrt(variance);

    return {
      samples: [...this.samples],
      avgTime: Math.round(avg),
      p95Time: this.calculateP95(),
      p99Time: this.calculateP99(),
      stdDev: Math.round(stdDev)
    };
  }

  /**
   * Get current adaptive window without updating
   */
  getCurrentCadence(): number {
    return Math.round(this.position);
  }

  /**
   * Get total evaluations recorded
   */
  getTotalEvaluations(): number {
    return this.totalEvaluations;
  }

  /**
   * Calculate 95th percentile of evaluation times
   * This is the target cadence (captures 95% of evaluations)
   */
  private calculateP95(): number {
    if (this.samples.length === 0) return this.position;

    const sorted = [...this.samples].sort((a, b) => a - b);
    const index = Math.floor(sorted.length * 0.95);
    return sorted[Math.min(index, sorted.length - 1)];
  }

  /**
   * Calculate 99th percentile of evaluation times
   * Used for outlier detection
   */
  private calculateP99(): number {
    if (this.samples.length === 0) return this.position;

    const sorted = [...this.samples].sort((a, b) => a - b);
    const index = Math.floor(sorted.length * 0.99);
    return sorted[Math.min(index, sorted.length - 1)];
  }

  /**
   * Reset state (useful for testing or context changes)
   */
  reset(): void {
    this.samples = [];
    this.position = 5000;
    this.velocity = 0;
    this.totalEvaluations = 0;
    this.lastUpdateTime = Date.now();
  }
}

/**
 * Heartbeat manager - tracks cadence for multiple conversation contexts
 */
export class HeartbeatManager {
  private heartbeats: Map<UUID, SystemHeartbeat> = new Map();

  /**
   * Get or create heartbeat for a conversation context
   */
  getHeartbeat(contextId: UUID): SystemHeartbeat {
    let heartbeat = this.heartbeats.get(contextId);
    if (!heartbeat) {
      heartbeat = new SystemHeartbeat(contextId);
      this.heartbeats.set(contextId, heartbeat);
      console.log(`🫀 Created heartbeat tracker for context ${contextId.slice(0, 8)}`);
    }
    return heartbeat;
  }

  /**
   * Remove heartbeat for a context (cleanup)
   */
  removeHeartbeat(contextId: UUID): void {
    this.heartbeats.delete(contextId);
  }

  /**
   * Get all tracked contexts
   */
  getTrackedContexts(): UUID[] {
    return Array.from(this.heartbeats.keys());
  }

  /**
   * Get heartbeat statistics for all contexts
   */
  getAllStats(): Map<UUID, HeartbeatStats> {
    const stats = new Map<UUID, HeartbeatStats>();
    for (const [contextId, heartbeat] of this.heartbeats) {
      stats.set(contextId, heartbeat.getStats());
    }
    return stats;
  }
}
