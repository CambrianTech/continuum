/**
 * Progressive Scorer - Token-window analysis for real-time complexity reassessment
 *
 * Analyzes AI response streams to detect upgrade indicators (hedging, uncertainty,
 * self-correction, etc.) and determine when to upgrade to more capable models.
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
 */

import type {
  ComplexityLevel,
  ProgressiveScorerConfig,
  ScoringResult,
  UpgradeIndicator
} from '../../../shared/ComplexityTypes';
import { DEFAULT_PROGRESSIVE_SCORER_CONFIG } from '../../../shared/ComplexityTypes';

/**
 * Upgrade indicator patterns - RegExp patterns for detecting complexity signals
 *
 * Based on linguistic research on uncertainty markers and meta-cognitive language
 * Used to identify when AI models are struggling and may benefit from upgrade
 */
const INDICATOR_PATTERNS: Record<UpgradeIndicator['type'], RegExp[]> = {
  /**
   * Hedging language - Indicates uncertainty or qualification
   * Examples: "it depends", "possibly", "might", "may", "could be"
   */
  hedging: [
    /\b(it depends|depending on)\b/i,
    /\b(possibly|perhaps|maybe)\b/i,
    /\b(might|may|could)\b(?![\w\s]+\b(not|never)\b)/i,  // Avoid false positives
    /\b(likely|probably|potentially)\b/i,
    /\b(seems to|appears to|tends to)\b/i
  ],

  /**
   * Self-correction - Model reconsidering or revising previous statements
   * Examples: "actually", "on second thought", "wait", "I should clarify"
   */
  'self-correction': [
    /\b(actually|in fact)\b/i,
    /\b(on second thought|thinking about it)\b/i,
    /\b(wait|hold on)\b/i,
    /\b(I should (clarify|correct|revise))\b/i,
    /\b(let me (rephrase|reconsider))\b/i
  ],

  /**
   * Multi-perspective reasoning - Considering multiple angles or viewpoints
   * Examples: "on one hand", "alternatively", "conversely", "however"
   */
  'multi-perspective': [
    /\b(on (one|the other) hand)\b/i,
    /\b(alternatively|conversely)\b/i,
    /\b(however|nevertheless|nonetheless)\b/i,
    /\b(from (another|a different) (perspective|angle|viewpoint))\b/i,
    /\b(that said|having said that)\b/i
  ],

  /**
   * Uncertainty admission - Explicit acknowledgment of complexity or unknowns
   * Examples: "I'm not sure", "this is complex", "it's unclear", "difficult to say"
   */
  uncertainty: [
    /\b(I'?m not (sure|certain|confident))\b/i,
    /\b(this is (complex|complicated|nuanced|difficult))\b/i,
    /\b(it'?s (unclear|ambiguous|hard to say))\b/i,
    /\b(without more (information|context|details))\b/i,
    /\b(difficult to (determine|know|say))\b/i
  ],

  /**
   * Clarification requests - Asking for more information or details
   * Examples: "could you clarify", "I need more information", "can you specify"
   */
  clarification: [
    /\b(could you (clarify|specify|elaborate))\b/i,
    /\b(I need (more|additional) (information|details|context))\b/i,
    /\b(can you (provide|give|share) more)\b/i,
    /\b(what (specifically|exactly) do you mean)\b/i,
    /\b(to better (understand|answer|help))\b/i
  ]
};

/**
 * ProgressiveScorer - Analyzes token windows for upgrade indicators
 *
 * State machine that tracks indicators across streaming generation:
 * 1. Accumulates text in sliding window
 * 2. Detects patterns using regex matching
 * 3. Scores confidence based on indicator types and counts
 * 4. Determines if upgrade thresholds exceeded
 * 5. Recommends complexity level for upgrade
 *
 * **Thread-safe**: Single instance per response generation
 * **Stateful**: Tracks indicators and tokens analyzed
 * **Resettable**: Can be reused across multiple analyses
 */
export class ProgressiveScorer {
  /** Configuration (window size, thresholds) */
  private config: ProgressiveScorerConfig;

  /** Indicators detected so far */
  private indicators: UpgradeIndicator[] = [];

  /** Total tokens analyzed */
  private tokensAnalyzed: number = 0;

  /**
   * Create a progressive scorer with optional custom configuration
   *
   * @param config - Partial config to override defaults
   */
  constructor(config?: Partial<ProgressiveScorerConfig>) {
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
   * Performs pattern matching against all indicator types, scores confidence,
   * and determines if upgrade thresholds have been exceeded.
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

    // Detect indicators in this chunk
    const newIndicators = this.detectIndicators(chunk, offset);
    this.indicators.push(...newIndicators);

    // Check if upgrade thresholds exceeded
    return this.evaluateUpgrade();
  }

  /**
   * Detect upgrade indicators in text chunk
   *
   * Scans chunk with all pattern types, records matches with metadata
   *
   * @param chunk - Text to scan
   * @param offset - Token offset for recording position
   * @returns Array of detected indicators
   */
  private detectIndicators(chunk: string, offset: number): UpgradeIndicator[] {
    const detected: UpgradeIndicator[] = [];

    // Scan each indicator type
    for (const [type, patterns] of Object.entries(INDICATOR_PATTERNS)) {
      for (const pattern of patterns) {
        const matches = chunk.matchAll(new RegExp(pattern, 'gi'));

        for (const match of matches) {
          // Calculate confidence based on pattern strength
          const confidence = this.calculateConfidence(type as UpgradeIndicator['type'], match[0]);

          detected.push({
            type: type as UpgradeIndicator['type'],
            pattern: match[0],
            offset: offset + (match.index ?? 0),
            confidence
          });
        }
      }
    }

    return detected;
  }

  /**
   * Calculate confidence score for a matched indicator
   *
   * Factors:
   * - Indicator type (uncertainty > hedging > clarification)
   * - Pattern specificity (longer/more specific = higher confidence)
   * - Context (surrounded by technical language = lower false positive)
   *
   * @param type - Indicator type
   * @param matchedText - Actual matched text
   * @returns Confidence score (0.0-1.0)
   */
  private calculateConfidence(type: UpgradeIndicator['type'], matchedText: string): number {
    // Base confidence by type
    const typeConfidence: Record<UpgradeIndicator['type'], number> = {
      uncertainty: 0.9,        // Strongest signal
      'self-correction': 0.8,  // Clear meta-cognitive marker
      clarification: 0.7,      // Moderate signal
      'multi-perspective': 0.6, // Weaker (might be intentional)
      hedging: 0.5            // Weakest (common in technical writing)
    };

    let confidence = typeConfidence[type];

    // Boost confidence for longer/more specific patterns
    if (matchedText.length > 15) {
      confidence += 0.1;
    }

    // Cap at 1.0
    return Math.min(confidence, 1.0);
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
          reason: `Detected ${this.indicators.length} complexity indicators (confidence: ${avgConfidence.toFixed(2)})`,
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
   * Clears indicators and token count, reusing config
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
  getState(): { indicatorsDetected: number; tokensAnalyzed: number; indicators: UpgradeIndicator[] } {
    return {
      indicatorsDetected: this.indicators.length,
      tokensAnalyzed: this.tokensAnalyzed,
      indicators: [...this.indicators] // Return copy to prevent mutation
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
}
