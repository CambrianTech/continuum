/**
 * AdaptiveConsolidationThreshold - Activity AND time-responsive consolidation
 *
 * Dual adjustment system:
 * 1. Activity-responsive (sigmoid):
 *    - Low activity → Low threshold → Consolidate MORE (concentration)
 *    - High activity → High threshold → Consolidate LESS (filter noise)
 *
 * 2. Time-responsive (exponential decay):
 *    - Threshold decays over time since last consolidation
 *    - Guarantees minimum consolidation frequency
 *    - Prevents "stuck" state where nothing consolidates
 *
 * Research: Temperature annealing, PER importance sampling, forgetting curves
 */

import { sigmoid, exponentialDecay } from './NonLinearMath';

export class AdaptiveConsolidationThreshold {
  private baseThreshold = 0.3;           // Minimum threshold
  private maxThreshold = 0.8;            // Maximum threshold
  private currentThreshold = 0.5;        // Current adaptive value

  // Activity tracking
  private recentActivity: number[] = [];  // Messages per minute, last 10 minutes
  private readonly activityWindow = 10;   // 10 minute window

  // Time decay tracking
  private lastConsolidation: Date = new Date();
  private readonly decayHalfLife = 5 * 60 * 1000;  // 5 minutes in milliseconds

  // Sigmoid parameters
  private readonly steepness = 0.5;      // k: curve steepness
  private readonly midpoint = 5.0;       // x0: activity level at threshold=0.5

  /**
   * Update threshold based on activity AND time (DUAL NON-LINEAR SYSTEM)
   *
   * 1. Activity effect (sigmoid):
   *    - Low activity → Low threshold → Consolidate MORE
   *    - High activity → High threshold → Consolidate LESS
   *
   * 2. Time decay effect (exponential):
   *    - Threshold decays over time since last consolidation
   *    - After decayHalfLife, threshold is halfway to baseThreshold
   *    - Guarantees minimum consolidation frequency
   */
  updateThreshold(messagesPerMinute: number): void {
    // Track recent activity
    this.recentActivity.push(messagesPerMinute);
    if (this.recentActivity.length > this.activityWindow) {
      this.recentActivity.shift();
    }

    // 1. Calculate activity-based threshold (sigmoid)
    const avgActivity = this.recentActivity.reduce((sum, a) => sum + a, 0)
                       / Math.max(1, this.recentActivity.length);

    const normalizedThreshold = sigmoid(avgActivity, this.steepness, this.midpoint);
    const activityThreshold = this.baseThreshold +
      (this.maxThreshold - this.baseThreshold) * normalizedThreshold;

    // 2. Calculate time decay multiplier (exponential)
    const timeSinceConsolidation = Date.now() - this.lastConsolidation.getTime();
    const decayMultiplier = exponentialDecay(timeSinceConsolidation, this.decayHalfLife);

    // 3. Combine: threshold decays from activity-based to base over time
    // After 0min: threshold = activityThreshold
    // After 5min (halfLife): threshold = (activityThreshold + baseThreshold) / 2
    // After 15min: threshold ≈ baseThreshold (forces consolidation)
    this.currentThreshold = this.baseThreshold +
      (activityThreshold - this.baseThreshold) * decayMultiplier;
  }

  /**
   * Record successful consolidation (resets time decay)
   */
  recordConsolidation(): void {
    this.lastConsolidation = new Date();
  }

  /**
   * Get current threshold for consolidation decision
   */
  getThreshold(): number {
    return this.currentThreshold;
  }

  /**
   * Should consolidate this thought?
   */
  shouldConsolidate(importance: number): boolean {
    return importance >= this.currentThreshold;
  }

  /**
   * Get statistics for logging/debugging
   */
  getStats() {
    const avgActivity = this.recentActivity.reduce((sum, a) => sum + a, 0)
                       / Math.max(1, this.recentActivity.length);
    const timeSinceConsolidation = Date.now() - this.lastConsolidation.getTime();
    const minutesSinceConsolidation = timeSinceConsolidation / (60 * 1000);
    const decayMultiplier = exponentialDecay(timeSinceConsolidation, this.decayHalfLife);

    return {
      currentThreshold: this.currentThreshold,
      baseThreshold: this.baseThreshold,
      maxThreshold: this.maxThreshold,
      avgActivity,
      activityWindow: this.activityWindow,
      minutesSinceConsolidation,
      decayMultiplier
    };
  }

  /**
   * Reset activity history and time (e.g., new session)
   */
  reset(): void {
    this.recentActivity = [];
    this.currentThreshold = 0.5;
    this.lastConsolidation = new Date();
  }
}
