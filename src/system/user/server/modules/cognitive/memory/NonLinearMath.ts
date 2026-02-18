/**
 * Non-linear math utilities for importance scoring
 * Research: PER (Schaul 2016), Softmax Temperature, TD-learning
 */

/**
 * Softmax with temperature control
 * High T → uniform (exploratory), Low T → sharp (focused)
 */
export function softmax(scores: number[], temperature: number = 1.0): number[] {
  const expScores = scores.map(s => Math.exp(s / temperature));
  const sum = expScores.reduce((a, b) => a + b, 0);
  return expScores.map(s => s / sum);
}

/**
 * Sigmoid function for smooth, organic transitions
 * Returns value in [0, 1]
 */
export function sigmoid(x: number, steepness: number = 1.0, midpoint: number = 0.0): number {
  return 1.0 / (1.0 + Math.exp(-steepness * (x - midpoint)));
}

/**
 * Exponential decay for time-based importance
 */
export function exponentialDecay(age: number, halfLife: number): number {
  return Math.exp(-age / halfLife);
}

/**
 * Power function for emphasis (PER-style)
 * alpha < 1: smooths differences
 * alpha > 1: sharpens differences
 */
export function powerEmphasis(value: number, alpha: number): number {
  return Math.pow(value, alpha);
}
