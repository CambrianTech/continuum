/**
 * Unit tests for RawMemoryAdapter
 *
 * Tests simple pass-through consolidation (no synthesis)
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { RawMemoryAdapter } from '../../../system/user/server/modules/cognitive/memory/adapters/RawMemoryAdapter';
import type { WorkingMemoryEntry } from '../../../system/user/server/modules/cognition/memory/InMemoryCognitionStorage';
import type { ConsolidationContext } from '../../../system/user/server/modules/cognitive/memory/adapters/MemoryConsolidationAdapter';
import { MemoryType } from '../../../system/user/server/modules/MemoryTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';

describe('RawMemoryAdapter', () => {
  let adapter: RawMemoryAdapter;
  let mockContext: ConsolidationContext;

  beforeEach(() => {
    adapter = new RawMemoryAdapter();

    mockContext = {
      personaId: generateUUID(),
      personaName: 'TestPersona',
      sessionId: generateUUID(),
      timestamp: new Date()
    };
  });

  describe('getName()', () => {
    it('should return adapter name', () => {
      expect(adapter.getName()).toBe('RawMemoryAdapter');
    });
  });

  describe('doesSynthesis()', () => {
    it('should return false (no synthesis)', () => {
      expect(adapter.doesSynthesis()).toBe(false);
    });
  });

  describe('consolidate()', () => {
    it('should return empty result for empty thoughts array', async () => {
      const result = await adapter.consolidate([], mockContext);

      expect(result.memories).toEqual([]);
      expect(result.metadata?.synthesisCount).toBe(0);
      expect(result.metadata?.groupsCreated).toBe(0);
    });

    it('should convert each thought to one memory (1:1 mapping)', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'First observation',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'decision',
          thoughtContent: 'Important decision',
          importance: 0.9,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'pattern',
          thoughtContent: 'Pattern identified',
          importance: 0.6,
          domain: 'coding',
          contextId: 'room-456',
          shareable: true
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Should create exactly 3 memories (one per thought)
      expect(result.memories).toHaveLength(3);
      expect(result.metadata?.synthesisCount).toBe(0); // No synthesis
      expect(result.metadata?.groupsCreated).toBe(3); // Each thought is its own "group"
    });

    it('should preserve thought content exactly', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Exact thought content to preserve',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      expect(result.memories[0].content).toBe('Exact thought content to preserve');
    });

    it('should map thought types to memory types correctly', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'reflection',
          thoughtContent: 'Reflection',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'decision',
          thoughtContent: 'Decision',
          importance: 0.8,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'pattern',
          thoughtContent: 'Pattern',
          importance: 0.6,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Observation',
          importance: 0.5,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Check mappings
      expect(result.memories[0].type).toBe(MemoryType.OBSERVATION); // reflection → OBSERVATION
      expect(result.memories[1].type).toBe(MemoryType.DECISION);    // decision → DECISION
      expect(result.memories[2].type).toBe(MemoryType.INSIGHT);     // pattern → INSIGHT
      expect(result.memories[3].type).toBe(MemoryType.OBSERVATION); // observation → OBSERVATION
    });

    it('should preserve importance values', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Low importance',
          importance: 0.3,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'High importance',
          importance: 0.95,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      expect(result.memories[0].importance).toBe(0.3);
      expect(result.memories[1].importance).toBe(0.95);
    });

    it('should preserve domain and contextId in context', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Test',
          importance: 0.7,
          domain: 'teaching',
          contextId: 'room-xyz',
          shareable: true
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      expect(result.memories[0].context.domain).toBe('teaching');
      expect(result.memories[0].context.contextId).toBe('room-xyz');
      expect(result.memories[0].context.thoughtType).toBe('observation');
      expect(result.memories[0].context.shareable).toBe(true);
    });

    it('should set correct memory metadata', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Test',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      const memory = result.memories[0];

      // Check metadata
      expect(memory.personaId).toBe(mockContext.personaId);
      expect(memory.sessionId).toBe(mockContext.sessionId);
      expect(memory.source).toBe('working-memory');
      expect(memory.accessCount).toBe(0);
      expect(memory.relatedTo).toEqual([]);
      expect(memory.tags).toContain('test'); // Domain becomes tag
    });

    it('should handle thoughts without domain/contextId', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'No context',
          importance: 0.7,
          domain: undefined,
          contextId: undefined,
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      expect(result.memories[0].context.domain).toBeUndefined();
      expect(result.memories[0].context.contextId).toBeUndefined();
      expect(result.memories[0].tags).toEqual([]); // No domain = no tags
    });

    it('should use default OBSERVATION type for unknown thought types', async () => {
      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'unknown-type' as any,
          thoughtContent: 'Test',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      expect(result.memories[0].type).toBe(MemoryType.OBSERVATION);
    });
  });
});
