/**
 * ComplexityDetector - Abstract interface for detecting upgrade indicators
 *
 * Pure abstraction layer separating "what to detect" from "how to detect".
 * Implementations can use regex, embeddings, ML classifiers, etc.
 *
 * **Architecture Pattern**: Same as DataAdapter interface in ORM
 * - ComplexityDetector = interface (pure abstraction)
 * - RegexComplexityDetector = one implementation (hard-coded patterns)
 * - EmbeddingComplexityDetector = future implementation (semantic similarity)
 * - MLComplexityDetector = future implementation (trained classifier)
 *
 * **Why This Matters:**
 * - Plug-and-play: Swap implementations without touching ProgressiveScorer
 * - A/B testing: Try different detection approaches side by side
 * - Easy removal: If regex doesn't work, drop it and plug in something better
 * - Future-proof: Add new detectors without redesigning the system
 *
 * @see ProgressiveScorer - Uses ComplexityDetector via dependency injection
 * @see RegexComplexityDetector - Default implementation using pattern matching
 */

import type { UpgradeIndicator } from '../../../shared/ComplexityTypes';

/**
 * ComplexityDetector - Pure abstraction for detecting upgrade indicators
 *
 * Interface contract:
 * - Input: Text chunk + offset
 * - Output: Array of detected indicators with confidence scores
 * - No assumptions about detection method
 */
export interface ComplexityDetector {
  /**
   * Analyze text chunk for upgrade indicators
   *
   * @param chunk - Text to analyze (streaming chunk)
   * @param offset - Token offset in full response
   * @returns Array of detected indicators
   */
  analyze(chunk: string, offset: number): UpgradeIndicator[];

  /**
   * Get detector name for logging/debugging
   */
  getName(): string;
}

/**
 * ComplexityDetectorFactory - Creates detector instances
 *
 * Enables runtime selection of detector implementation:
 * - 'regex' → RegexComplexityDetector
 * - 'embedding' → EmbeddingComplexityDetector (future)
 * - 'ml' → MLComplexityDetector (future)
 *
 * @example
 * const detector = ComplexityDetectorFactory.create('regex');
 * const scorer = new ProgressiveScorer(detector, config);
 */
export class ComplexityDetectorFactory {
  /**
   * Create detector instance by type
   *
   * @param type - Detector type ('regex', 'embedding', 'ml')
   * @param options - Type-specific configuration
   * @returns ComplexityDetector instance
   */
  static create(type: 'regex' | 'embedding' | 'ml', options?: unknown): ComplexityDetector {
    switch (type) {
      case 'regex':
        // Import dynamically to avoid circular deps
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const { RegexComplexityDetector } = require('./RegexComplexityDetector');
        return new RegexComplexityDetector(options);

      case 'embedding':
        throw new Error('EmbeddingComplexityDetector not yet implemented');

      case 'ml':
        throw new Error('MLComplexityDetector not yet implemented');

      default:
        throw new Error(`Unknown detector type: ${type}`);
    }
  }

  /**
   * Get default detector (regex for now)
   *
   * @returns Default ComplexityDetector instance
   */
  static createDefault(): ComplexityDetector {
    return ComplexityDetectorFactory.create('regex');
  }
}
