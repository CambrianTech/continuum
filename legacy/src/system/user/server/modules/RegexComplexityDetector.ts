/**
 * RegexComplexityDetector - Pattern-based complexity detection
 *
 * One concrete implementation of ComplexityDetector interface.
 * Uses hard-coded RegExp patterns to detect linguistic markers of model struggle.
 *
 * **Architectural Role:**
 * - This is ONE approach to complexity detection (not THE approach)
 * - Can be swapped out for embedding-based or ML-based detectors
 * - Plugs into ProgressiveScorer via ComplexityDetector interface
 *
 * **Trade-offs:**
 * - ✅ Fast: O(n) pattern matching, no API calls
 * - ✅ Simple: No dependencies, easy to understand
 * - ❌ Brittle: Hard-coded patterns may miss novel phrasings
 * - ❌ Language-specific: Only works for English
 *
 * **When to Remove:**
 * - If false positive rate > 10% in Phase 3 testing
 * - If embedding-based approach shows >20% better accuracy
 * - If ML classifier achieves >90% precision/recall
 *
 * @see ComplexityDetector - Interface contract
 * @see ProgressiveScorer - Consumer of this detector
 */

import type { UpgradeIndicator } from '../../../shared/ComplexityTypes';
import type { ComplexityDetector } from './ComplexityDetector';

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
 * RegexComplexityDetector - Pattern-based implementation of ComplexityDetector
 *
 * Scans text for linguistic markers using hard-coded RegExp patterns.
 * Fast and simple, but brittle and language-specific.
 *
 * **Configuration Options:**
 * - patterns: Custom pattern map (overrides default INDICATOR_PATTERNS)
 * - confidenceBoost: Multiplier for confidence scores (default: 1.0)
 */
export class RegexComplexityDetector implements ComplexityDetector {
  private patterns: Record<UpgradeIndicator['type'], RegExp[]>;
  private confidenceBoost: number;

  /**
   * Create regex-based detector
   *
   * @param options - Configuration options
   * @param options.patterns - Custom pattern map (optional)
   * @param options.confidenceBoost - Confidence multiplier (optional)
   */
  constructor(options?: {
    patterns?: Record<UpgradeIndicator['type'], RegExp[]>;
    confidenceBoost?: number;
  }) {
    this.patterns = options?.patterns ?? INDICATOR_PATTERNS;
    this.confidenceBoost = options?.confidenceBoost ?? 1.0;
  }

  /**
   * Analyze text chunk for upgrade indicators using pattern matching
   *
   * @param chunk - Text to scan
   * @param offset - Token offset for recording position
   * @returns Array of detected indicators
   */
  analyze(chunk: string, offset: number): UpgradeIndicator[] {
    const detected: UpgradeIndicator[] = [];

    // Scan each indicator type
    for (const [type, patterns] of Object.entries(this.patterns)) {
      for (const pattern of patterns) {
        const matches = chunk.matchAll(new RegExp(pattern, 'gi'));

        for (const match of matches) {
          // Calculate confidence based on pattern strength
          const confidence = this.calculateConfidence(type as UpgradeIndicator['type'], match[0]);

          // Convert character-based match.index to token offset (4 chars per token heuristic)
          const tokenOffset = Math.floor((match.index ?? 0) / 4);

          detected.push({
            type: type as UpgradeIndicator['type'],
            pattern: match[0],
            offset: offset + tokenOffset,
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
   * - Confidence boost multiplier
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

    // Apply confidence boost multiplier
    confidence *= this.confidenceBoost;

    // Cap at 1.0
    return Math.min(confidence, 1.0);
  }

  /**
   * Get detector name for logging/debugging
   */
  getName(): string {
    return 'RegexComplexityDetector';
  }
}
