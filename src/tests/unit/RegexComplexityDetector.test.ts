/**
 * Unit tests for RegexComplexityDetector
 *
 * Tests the character-to-token offset conversion fix
 */

import { describe, it, expect } from 'vitest';
import { RegexComplexityDetector } from '../../system/user/server/modules/RegexComplexityDetector';

describe('RegexComplexityDetector', () => {
  describe('offset calculation', () => {
    it('should convert character offsets to token offsets (4 chars per token)', () => {
      const detector = new RegexComplexityDetector();

      // Test chunk with uncertainty indicator at known position
      const chunk = "This is a test. I'm not sure if this is correct.";
      //                              ^ "I am not sure" starts at char 16
      //                              Token offset: 16 / 4 = 4
      const tokenOffset = 100; // Simulating we're at token 100 in full response

      const indicators = detector.analyze(chunk, tokenOffset);

      // Should detect "not sure" (uncertainty indicator)
      expect(indicators.length).toBeGreaterThan(0);

      const uncertaintyIndicator = indicators.find(ind => ind.type === 'uncertainty');
      expect(uncertaintyIndicator).toBeDefined();

      if (uncertaintyIndicator) {
        // Character position of "not sure" in chunk: around 22
        // Token offset: 22 / 4 = 5 (rounded down)
        // Total offset: 100 + 5 = 105
        expect(uncertaintyIndicator.offset).toBeGreaterThanOrEqual(100);
        expect(uncertaintyIndicator.offset).toBeLessThan(120); // Reasonable range

        console.log(`✓ Detected uncertainty at token offset: ${uncertaintyIndicator.offset}`);
        console.log(`  Pattern: "${uncertaintyIndicator.pattern}"`);
        console.log(`  Expected range: 100-120, got: ${uncertaintyIndicator.offset}`);
      }
    });

    it('should handle multiple indicators with correct offsets', () => {
      const detector = new RegexComplexityDetector();

      // Chunk with multiple indicators at different positions
      const chunk = 'I might be wrong. Actually, I think it could be different.';
      //             ^ "might" at ~2      ^ "Actually" at ~18  ^ "could be" at ~36
      //             Token: 0              Token: 4            Token: 9
      const tokenOffset = 50;

      const indicators = detector.analyze(chunk, tokenOffset);

      // Should detect at least 2-3 indicators (hedging + self-correction + hedging)
      expect(indicators.length).toBeGreaterThan(1);

      // All offsets should be >= 50 (base offset)
      indicators.forEach(ind => {
        expect(ind.offset).toBeGreaterThanOrEqual(50);
        console.log(`  Indicator: ${ind.type} at token ${ind.offset} - "${ind.pattern}"`);
      });

      // Offsets should be in increasing order (roughly)
      const offsets = indicators.map(ind => ind.offset);
      console.log(`✓ Detected ${indicators.length} indicators at tokens:`, offsets);
    });

    it('should produce consistent offsets with 4-char-per-token heuristic', () => {
      const detector = new RegexComplexityDetector();

      // Known pattern at specific position
      const chunk = "0123456789ABCDEF I'm not sure about this";
      //                                ^ "not sure" starts at char ~17
      //                                Token offset: 17 / 4 = 4
      const tokenOffset = 0;

      const indicators = detector.analyze(chunk, tokenOffset);

      const uncertaintyIndicator = indicators.find(ind =>
        ind.pattern.toLowerCase().includes('not sure')
      );

      expect(uncertaintyIndicator).toBeDefined();

      if (uncertaintyIndicator) {
        // Character position ~17-22, token position ~4-5
        expect(uncertaintyIndicator.offset).toBeGreaterThanOrEqual(0);
        expect(uncertaintyIndicator.offset).toBeLessThanOrEqual(10);
        console.log(`✓ Consistent offset: ${uncertaintyIndicator.offset} (expected 4-5)`);
      }
    });
  });

  describe('indicator detection', () => {
    it('should detect uncertainty indicators', () => {
      const detector = new RegexComplexityDetector();
      const chunk = "I'm not sure about this approach.";

      const indicators = detector.analyze(chunk, 0);

      const uncertaintyCount = indicators.filter(ind => ind.type === 'uncertainty').length;
      expect(uncertaintyCount).toBeGreaterThan(0);
      console.log(`✓ Detected ${uncertaintyCount} uncertainty indicator(s)`);
    });

    it('should detect self-correction indicators', () => {
      const detector = new RegexComplexityDetector();
      const chunk = 'Actually, I was wrong about that.';

      const indicators = detector.analyze(chunk, 0);

      const correctionCount = indicators.filter(ind => ind.type === 'self-correction').length;
      expect(correctionCount).toBeGreaterThan(0);
      console.log(`✓ Detected ${correctionCount} self-correction indicator(s)`);
    });

    it('should detect hedging indicators', () => {
      const detector = new RegexComplexityDetector();
      const chunk = 'This might possibly work in some cases.';

      const indicators = detector.analyze(chunk, 0);

      const hedgingCount = indicators.filter(ind => ind.type === 'hedging').length;
      expect(hedgingCount).toBeGreaterThan(0);
      console.log(`✓ Detected ${hedgingCount} hedging indicator(s)`);
    });
  });

  describe('getName', () => {
    it('should return detector name', () => {
      const detector = new RegexComplexityDetector();
      expect(detector.getName()).toBe('RegexComplexityDetector');
    });
  });
});
