/**
 * Progressive Scorer - Token-window analysis for real-time complexity reassessment
 *
 * Analyzes AI response streams to detect upgrade indicators and determine when
 * to upgrade to more capable models. Now uses PLUGGABLE detection via
 * ComplexityDetector interface.
 *
 * **Architecture**: Scoring logic separated from detection logic
 * - ProgressiveScorer = evaluates thresholds and makes upgrade decisions
 * - ComplexityDetector = detects indicators (RegExp, embeddings, ML, etc.)
 *
 * **Purpose**: Enable mid-stream model upgrades when lower-tier models show signs
 * of struggling, maintaining cost-efficiency while preserving quality.
 *
 * **Core Concept**: Start cheap/free (qwen2.5:7b), detect complexity as generating,
 * upgrade only when needed (llama3.1:70b → deepseek-chat → claude-3-5-sonnet).
 *
 * **Integration**: Used by AIProviderDaemon streaming wrapper (Phase 2B)
 *
 * @see PHASE2-PROGRESSIVE-SCORING-PLAN.md - Implementation plan
 * @see ADAPTIVE-COMPLEXITY-ROUTING.md - Full architecture
 * @see COMPLEXITY-DETECTOR-REFACTORING.md - Plug-and-play pattern
 */

import type {
  ComplexityLevel,
  ProgressiveScorerConfig,
  ScoringResult,
  UpgradeIndicator
} from '../../../shared/ComplexityTypes';
import { DEFAULT_PROGRESSIVE_SCORER_CONFIG } from '../../../shared/ComplexityTypes';
import type { ComplexityDetector } from './ComplexityDetector';
import { ComplexityDetectorFactory } from './ComplexityDetector';

/**
 * ProgressiveScorer - Analyzes token windows for upgrade indicators
 *
 * State machine that tracks indicators across streaming generation:
 * 1. Accumulates text in sliding window
 * 2. Delegates detection to pluggable ComplexityDetector
 * 3. Scores confidence based on indicator types and counts
 * 4. Determines if upgrade thresholds exceeded
 * 5. Recommends complexity level for upgrade
 *
 * **Thread-safe**: Single instance per response generation
 * **Stateful**: Tracks indicators and tokens analyzed
 * **Resettable**: Can be reused across multiple analyses
 * **Pluggable**: Accepts any ComplexityDetector implementation
 */
export class ProgressiveScorer {
  /** Pluggable complexity detector (RegExp, embedding, ML, etc.) */
  private detector: ComplexityDetector;

  /** Configuration (window size, thresholds) */
  private config: ProgressiveScorerConfig;

  /** Indicators detected so far */
  private indicators: UpgradeIndicator[] = [];

  /** Total tokens analyzed */
  private tokensAnalyzed: number = 0;

  /**
   * Create a progressive scorer with pluggable detector
   *
   * @param detector - ComplexityDetector implementation (optional, defaults to regex)
   * @param config - Partial config to override defaults (optional)
   *
   * @example
   * // Default (regex)
   * const scorer = new ProgressiveScorer();
   *
   * @example
   * // Custom detector
   * const detector = new EmbeddingComplexityDetector();
   * const scorer = new ProgressiveScorer(detector);
   *
   * @example
   * // Custom detector + config
   * const detector = ComplexityDetectorFactory.create('ml');
   * const scorer = new ProgressiveScorer(detector, {
   *   windowSize: 500,
   *   thresholds: { indicatorCount: 5 }
   * });
   */
  constructor(
    detector?: ComplexityDetector,
    config?: Partial<ProgressiveScorerConfig>
  ) {
    // Use provided detector or default to regex
    this.detector = detector ?? ComplexityDetectorFactory.createDefault();

    // Merge config with defaults
    this.config = {
      ...DEFAULT_PROGRESSIVE_SCORER_CONFIG,
      ...config,
      thresholds: {
        ...DEFAULT_PROGRESSIVE_SCORER_CONFIG.thresholds,
        ...(config?.thresholds || {})
      }
    };
  }

  /**
   * Analyze text chunk for upgrade indicators
   *
   * Delegates detection to pluggable detector, then evaluates upgrade thresholds.
   *
   * **Token estimation**: ~4 characters per token (rough heuristic)
   *
   * @param chunk - Text chunk to analyze (from streaming response)
   * @param offset - Token offset in full response (for tracking)
   * @returns Scoring result with upgrade recommendation
   */
  analyze(chunk: string, offset: number): ScoringResult {
    // Update tokens analyzed (estimate: 4 chars per token)
    const estimatedTokens = Math.floor(chunk.length / 4);
    this.tokensAnalyzed += estimatedTokens;

    // Delegate detection to pluggable detector
    const newIndicators = this.detector.analyze(chunk, offset);
    this.indicators.push(...newIndicators);

    // Evaluate upgrade thresholds (scoring logic)
    return this.evaluateUpgrade();
  }

  /**
   * Evaluate if upgrade thresholds have been exceeded
   *
   * Three-tier threshold system:
   * 1. Indicator count: >= N indicators (default: 3)
   * 2. Confidence average: >= threshold (default: 0.6)
   * 3. Token budget: >= max tokens (default: 1000) - force decision
   *
   * @returns Scoring result with upgrade decision
   */
  private evaluateUpgrade(): ScoringResult {
    // No indicators yet - no upgrade
    if (this.indicators.length === 0) {
      return { shouldUpgrade: false };
    }

    // Calculate average confidence
    const avgConfidence = this.indicators.reduce((sum, ind) => sum + ind.confidence, 0) / this.indicators.length;

    // Check threshold 1: Indicator count
    if (this.indicators.length >= this.config.thresholds.indicatorCount) {
      // Also check threshold 2: Confidence minimum
      if (avgConfidence >= this.config.thresholds.confidence) {
        return {
          shouldUpgrade: true,
          reason: `Detected ${this.indicators.length} complexity indicators (confidence: ${avgConfidence.toFixed(2)}) using ${this.detector.getName()}`,
          newLevel: this.determineTargetLevel()
        };
      }
    }

    // Check threshold 3: Token budget exceeded (fail-safe)
    if (this.tokensAnalyzed >= this.config.thresholds.tokenBudget) {
      // If indicators present but below count/confidence, still upgrade as fail-safe
      if (this.indicators.length > 0) {
        return {
          shouldUpgrade: true,
          reason: `Token budget exceeded (${this.tokensAnalyzed} tokens) with ${this.indicators.length} indicators`,
          newLevel: 'moderate' // Conservative upgrade
        };
      }
    }

    // Thresholds not exceeded
    return { shouldUpgrade: false };
  }

  /**
   * Determine target complexity level based on indicator analysis
   *
   * Decision logic:
   * - High uncertainty indicators → 'nuanced' (premium models)
   * - Multiple strong indicators → 'moderate' (capable models)
   * - Default upgrade → 'moderate' (conservative)
   *
   * @returns Target complexity level for upgrade
   */
  private determineTargetLevel(): ComplexityLevel {
    // Count uncertainty indicators (strongest signal for nuanced complexity)
    const uncertaintyCount = this.indicators.filter(ind => ind.type === 'uncertainty').length;

    // If multiple uncertainty admissions, route to premium
    if (uncertaintyCount >= 2) {
      return 'nuanced';
    }

    // Count self-correction indicators (meta-cognitive struggle)
    const selfCorrectionCount = this.indicators.filter(ind => ind.type === 'self-correction').length;

    // If model repeatedly self-correcting, needs capable tier
    if (selfCorrectionCount >= 2) {
      return 'moderate';
    }

    // Default: moderate upgrade (conservative choice)
    return 'moderate';
  }

  /**
   * Reset scorer state for new analysis
   *
   * Clears indicators and token count, reusing config and detector
   * Allows single instance to analyze multiple responses
   */
  reset(): void {
    this.indicators = [];
    this.tokensAnalyzed = 0;
  }

  /**
   * Get current analysis state for debugging/monitoring
   *
   * Useful for:
   * - Unit testing (verify detection logic)
   * - Performance monitoring (track indicator frequency)
   * - Debugging (understand why upgrade triggered)
   *
   * @returns Current state snapshot
   */
  getState(): {
    indicatorsDetected: number;
    tokensAnalyzed: number;
    indicators: UpgradeIndicator[];
    detectorName: string;
  } {
    return {
      indicatorsDetected: this.indicators.length,
      tokensAnalyzed: this.tokensAnalyzed,
      indicators: [...this.indicators], // Return copy to prevent mutation
      detectorName: this.detector.getName()
    };
  }

  /**
   * Get current configuration
   *
   * Useful for:
   * - Verification (check thresholds being used)
   * - Debugging (understand scoring behavior)
   *
   * @returns Current configuration
   */
  getConfig(): ProgressiveScorerConfig {
    return { ...this.config };
  }

  /**
   * Get detector name for logging/debugging
   *
   * @returns Name of current detector implementation
   */
  getDetectorName(): string {
    return this.detector.getName();
  }

  /**
   * Swap detector at runtime (advanced use case)
   *
   * Useful for A/B testing or dynamic switching based on performance
   *
   * @param detector - New detector to use
   */
  setDetector(detector: ComplexityDetector): void {
    this.detector = detector;
  }
}
