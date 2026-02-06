/**
 * CodingModelSelector Unit Tests
 *
 * Tests model routing for different coding task types.
 * Validates:
 * - Default tier selection for each task type
 * - Provider fallback when preferred provider unavailable
 * - Edge cases: no providers, single provider
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { CodingModelSelector } from '../../../system/code/server/CodingModelSelector';
import type { CodingTaskType } from '../../../system/code/shared/CodingTypes';

describe('CodingModelSelector', () => {
  let selector: CodingModelSelector;

  beforeEach(() => {
    // Full set of SOTA providers
    selector = new CodingModelSelector(new Set([
      'anthropic', 'openai', 'deepseek', 'groq', 'xai', 'google', 'together', 'fireworks',
    ]));
  });

  describe('default tier selection', () => {
    it('selects anthropic for planning tasks', () => {
      const tier = selector.select('planning');
      expect(tier.provider).toBe('anthropic');
      expect(tier.taskType).toBe('planning');
      expect(tier.temperature).toBeLessThanOrEqual(0.5);
    });

    it('selects anthropic for generation tasks', () => {
      const tier = selector.select('generation');
      expect(tier.provider).toBe('anthropic');
      expect(tier.taskType).toBe('generation');
    });

    it('selects anthropic for editing tasks with low temperature', () => {
      const tier = selector.select('editing');
      expect(tier.provider).toBe('anthropic');
      expect(tier.temperature).toBeLessThanOrEqual(0.3);
    });

    it('selects deepseek for review tasks', () => {
      const tier = selector.select('review');
      expect(tier.provider).toBe('deepseek');
      expect(tier.taskType).toBe('review');
    });

    it('selects groq for quick-fix tasks', () => {
      const tier = selector.select('quick-fix');
      expect(tier.provider).toBe('groq');
      expect(tier.taskType).toBe('quick-fix');
    });

    it('selects groq for discovery tasks', () => {
      const tier = selector.select('discovery');
      expect(tier.provider).toBe('groq');
      expect(tier.taskType).toBe('discovery');
    });
  });

  describe('all task types return valid tiers', () => {
    const taskTypes: CodingTaskType[] = [
      'planning', 'generation', 'editing', 'review', 'quick-fix', 'discovery',
    ];

    for (const taskType of taskTypes) {
      it(`returns valid tier for "${taskType}"`, () => {
        const tier = selector.select(taskType);
        expect(tier.taskType).toBe(taskType);
        expect(tier.provider).toBeTruthy();
        expect(tier.model).toBeTruthy();
        expect(tier.temperature).toBeGreaterThanOrEqual(0);
        expect(tier.temperature).toBeLessThanOrEqual(1);
        expect(tier.maxTokens).toBeGreaterThan(0);
        expect(tier.description).toBeTruthy();
      });
    }
  });

  describe('provider fallback', () => {
    it('falls back when preferred provider is unavailable', () => {
      // Only openai available — planning defaults to anthropic, should fallback
      const limited = new CodingModelSelector(new Set(['openai']));
      const tier = limited.select('planning');
      expect(tier.provider).toBe('openai');
      expect(tier.taskType).toBe('planning');
    });

    it('falls through fallback order correctly', () => {
      // Only groq available
      const groqOnly = new CodingModelSelector(new Set(['groq']));
      const tier = groqOnly.select('planning');
      expect(tier.provider).toBe('groq');
    });

    it('preserves temperature and maxTokens from default tier on fallback', () => {
      const limited = new CodingModelSelector(new Set(['deepseek']));
      const tier = limited.select('editing');
      // Should keep editing's low temperature even on fallback
      expect(tier.temperature).toBeLessThanOrEqual(0.3);
      expect(tier.provider).toBe('deepseek');
    });

    it('marks fallback in description', () => {
      const limited = new CodingModelSelector(new Set(['openai']));
      const tier = limited.select('review');
      // review defaults to deepseek, should fallback to openai
      expect(tier.description).toContain('fallback');
    });

    it('returns default tier when no providers available', () => {
      const empty = new CodingModelSelector(new Set());
      const tier = empty.select('planning');
      // Returns default (may fail at runtime), but returns a tier
      expect(tier.taskType).toBe('planning');
      expect(tier.provider).toBeTruthy();
    });
  });

  describe('hasFrontierModel', () => {
    it('returns true when frontier providers available', () => {
      expect(selector.hasFrontierModel).toBe(true);
    });

    it('returns false when no frontier providers available', () => {
      const empty = new CodingModelSelector(new Set());
      expect(empty.hasFrontierModel).toBe(false);
    });

    it('returns true with even a single frontier provider', () => {
      const single = new CodingModelSelector(new Set(['groq']));
      expect(single.hasFrontierModel).toBe(true);
    });

    it('returns false with only non-frontier providers', () => {
      const local = new CodingModelSelector(new Set(['ollama', 'candle']));
      expect(local.hasFrontierModel).toBe(false);
    });
  });

  describe('available providers update', () => {
    it('reflects updated providers in selection', () => {
      const limited = new CodingModelSelector(new Set(['groq']));
      expect(limited.select('planning').provider).toBe('groq');

      // Add anthropic
      limited.availableProviders = new Set(['groq', 'anthropic']);
      expect(limited.select('planning').provider).toBe('anthropic');
    });
  });

  describe('allTiers', () => {
    it('returns all configured tiers', () => {
      const tiers = selector.allTiers;
      expect(tiers.length).toBe(6); // 6 task types
      const taskTypes = tiers.map(t => t.taskType);
      expect(taskTypes).toContain('planning');
      expect(taskTypes).toContain('generation');
      expect(taskTypes).toContain('editing');
      expect(taskTypes).toContain('review');
      expect(taskTypes).toContain('quick-fix');
      expect(taskTypes).toContain('discovery');
    });
  });
});
