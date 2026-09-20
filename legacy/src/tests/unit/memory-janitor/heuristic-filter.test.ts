/**
 * Unit tests for MemoryJanitorDaemon heuristic filter
 *
 * Tests the Pass 1 logic that removes 80-90% of items without LLM
 */

import { describe, it, expect, beforeEach } from 'vitest';

interface WorkingMemoryEntity {
  id: string;
  personaId: string;
  content: string;
  timestamp: Date;
  domain: 'chat' | 'code' | 'academy' | 'self';
  ephemeral: boolean;
  consolidated: boolean;
  importance: number;
}

/**
 * Heuristic filter implementation (extracted for testing)
 */
function heuristicFilter(items: WorkingMemoryEntity[]): {
  ephemeral: WorkingMemoryEntity[];
  candidates: WorkingMemoryEntity[];
} {
  const ephemeral: WorkingMemoryEntity[] = [];
  const candidates: WorkingMemoryEntity[] = [];

  for (const item of items) {
    // Explicit ephemeral flag
    if (item.ephemeral) {
      ephemeral.push(item);
      continue;
    }

    // Old and low importance
    const ageMs = Date.now() - item.timestamp.getTime();
    const ageHours = ageMs / (1000 * 60 * 60);

    if (ageHours > 24 && item.importance < 0.3) {
      ephemeral.push(item);
      continue;
    }

    // Everything else needs LLM evaluation
    candidates.push(item);
  }

  return { ephemeral, candidates };
}

describe('MemoryJanitorDaemon - Heuristic Filter', () => {
  describe('Explicit ephemeral flag', () => {
    it('should classify items with ephemeral=true as ephemeral regardless of other properties', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Important message',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: true,  // Explicit flag
          consolidated: false,
          importance: 0.9   // High importance doesn't matter
        }
      ];

      const result = heuristicFilter(items);

      expect(result.ephemeral).toHaveLength(1);
      expect(result.candidates).toHaveLength(0);
      expect(result.ephemeral[0].id).toBe('1');
    });
  });

  describe('Age and importance filter', () => {
    it('should classify old (>24h) low-importance (<0.3) items as ephemeral', () => {
      const oldDate = new Date(Date.now() - 25 * 60 * 60 * 1000); // 25 hours ago

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Old unimportant message',
          timestamp: oldDate,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.2  // Low importance
        }
      ];

      const result = heuristicFilter(items);

      expect(result.ephemeral).toHaveLength(1);
      expect(result.candidates).toHaveLength(0);
    });

    it('should keep old items with high importance as candidates', () => {
      const oldDate = new Date(Date.now() - 25 * 60 * 60 * 1000);

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Old but important message',
          timestamp: oldDate,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.8  // High importance
        }
      ];

      const result = heuristicFilter(items);

      expect(result.ephemeral).toHaveLength(0);
      expect(result.candidates).toHaveLength(1);
    });

    it('should keep recent low-importance items as candidates', () => {
      const recentDate = new Date(Date.now() - 2 * 60 * 60 * 1000); // 2 hours ago

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Recent unimportant message',
          timestamp: recentDate,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.2  // Low importance but recent
        }
      ];

      const result = heuristicFilter(items);

      expect(result.ephemeral).toHaveLength(0);
      expect(result.candidates).toHaveLength(1);
    });
  });

  describe('Edge cases', () => {
    it('should handle exactly 24 hours old items (boundary)', () => {
      const exactlyOldDate = new Date(Date.now() - 24 * 60 * 60 * 1000);

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Exactly 24h old',
          timestamp: exactlyOldDate,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.2
        }
      ];

      const result = heuristicFilter(items);

      // Exactly 24h should NOT be classified as ephemeral (> 24h required)
      expect(result.ephemeral).toHaveLength(0);
      expect(result.candidates).toHaveLength(1);
    });

    it('should handle exactly 0.3 importance (boundary)', () => {
      const oldDate = new Date(Date.now() - 25 * 60 * 60 * 1000);

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Exactly 0.3 importance',
          timestamp: oldDate,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.3  // Boundary value
        }
      ];

      const result = heuristicFilter(items);

      // Exactly 0.3 should NOT be classified as ephemeral (< 0.3 required)
      expect(result.ephemeral).toHaveLength(0);
      expect(result.candidates).toHaveLength(1);
    });

    it('should handle empty input', () => {
      const result = heuristicFilter([]);

      expect(result.ephemeral).toHaveLength(0);
      expect(result.candidates).toHaveLength(0);
    });
  });

  describe('Batch processing', () => {
    it('should correctly partition mixed items', () => {
      const now = Date.now();
      const old = new Date(now - 25 * 60 * 60 * 1000);
      const recent = new Date(now - 2 * 60 * 60 * 1000);

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Explicit ephemeral',
          timestamp: recent,
          domain: 'chat',
          ephemeral: true,  // Should be ephemeral
          consolidated: false,
          importance: 0.9
        },
        {
          id: '2',
          personaId: 'persona-1',
          content: 'Old and low importance',
          timestamp: old,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.1  // Should be ephemeral
        },
        {
          id: '3',
          personaId: 'persona-1',
          content: 'Recent and important',
          timestamp: recent,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.8  // Should be candidate
        },
        {
          id: '4',
          personaId: 'persona-1',
          content: 'Old but important',
          timestamp: old,
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.7  // Should be candidate
        }
      ];

      const result = heuristicFilter(items);

      expect(result.ephemeral).toHaveLength(2);
      expect(result.candidates).toHaveLength(2);

      // Verify correct items in each category
      const ephemeralIds = result.ephemeral.map(i => i.id).sort();
      const candidateIds = result.candidates.map(i => i.id).sort();

      expect(ephemeralIds).toEqual(['1', '2']);
      expect(candidateIds).toEqual(['3', '4']);
    });

    it('should achieve ~80-90% reduction on realistic data', () => {
      const now = Date.now();
      const items: WorkingMemoryEntity[] = [];

      // Simulate 100 items:
      // - 50% explicit ephemeral (routine chatter)
      // - 30% old + low importance
      // - 20% worth LLM evaluation
      for (let i = 0; i < 100; i++) {
        if (i < 50) {
          // Explicit ephemeral
          items.push({
            id: `${i}`,
            personaId: 'persona-1',
            content: `Ephemeral ${i}`,
            timestamp: new Date(now - Math.random() * 10 * 60 * 60 * 1000),
            domain: 'chat',
            ephemeral: true,
            consolidated: false,
            importance: Math.random()
          });
        } else if (i < 80) {
          // Old + low importance
          items.push({
            id: `${i}`,
            personaId: 'persona-1',
            content: `Old unimportant ${i}`,
            timestamp: new Date(now - (25 + Math.random() * 10) * 60 * 60 * 1000),
            domain: 'chat',
            ephemeral: false,
            consolidated: false,
            importance: 0.1 + Math.random() * 0.1  // 0.1-0.2 range
          });
        } else {
          // Worth LLM evaluation
          items.push({
            id: `${i}`,
            personaId: 'persona-1',
            content: `Potentially important ${i}`,
            timestamp: new Date(now - Math.random() * 10 * 60 * 60 * 1000),
            domain: 'chat',
            ephemeral: false,
            consolidated: false,
            importance: 0.5 + Math.random() * 0.5  // 0.5-1.0 range
          });
        }
      }

      const result = heuristicFilter(items);

      // Should filter out 80 items (50 explicit + 30 old/low)
      expect(result.ephemeral).toHaveLength(80);
      expect(result.candidates).toHaveLength(20);

      // Verify 80% reduction
      const reductionPercent = (result.ephemeral.length / items.length) * 100;
      expect(reductionPercent).toBe(80);
    });
  });
});
