/**
 * CodeToolSource Unit Tests
 *
 * Tests the CodeToolSource RAGSource in isolation by mocking PersonaToolRegistry.
 * Validates:
 * - isApplicable() based on persona tool permissions
 * - load() generates correct coding workflow prompt
 * - Budget-aware: falls back to minimal prompt when budget is tight
 * - Caching: repeated calls use cached prompt
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { CodeToolSource } from '../../../system/rag/sources/CodeToolSource';
import type { RAGSourceContext } from '../../../system/rag/shared/RAGSource';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// Mock PersonaToolRegistry
const mockToolsForPersona = vi.fn();

vi.mock('../../../system/user/server/modules/PersonaToolRegistry', () => ({
  PersonaToolRegistry: {
    sharedInstance: () => ({
      listToolsForPersona: mockToolsForPersona,
    }),
  },
}));

// Mock Logger (avoid real logging in tests)
vi.mock('../../../system/core/logging/Logger', () => ({
  Logger: {
    create: () => ({
      debug: () => {},
      error: () => {},
      warn: () => {},
      info: () => {},
    }),
  },
}));

/**
 * Helper to create a fake tool definition
 */
function fakeTool(name: string, description = `${name} command`) {
  return {
    name,
    description,
    category: name.startsWith('code/') ? 'code' as const : 'system' as const,
    permissions: ['code:search'],
    parameters: { type: 'object' as const, properties: {}, required: [] },
    examples: [],
  };
}

/**
 * Helper to build a RAGSourceContext
 */
function makeContext(overrides?: Partial<RAGSourceContext>): RAGSourceContext {
  return {
    personaId: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID,
    roomId: '11111111-2222-3333-4444-555555555555' as UUID,
    options: {},
    totalBudget: 2000,
    ...overrides,
  };
}

describe('CodeToolSource', () => {
  let source: CodeToolSource;

  beforeEach(() => {
    source = new CodeToolSource();
    mockToolsForPersona.mockReset();
    // Clear the static cache between tests
    (CodeToolSource as any)._cachedPrompt = null;
    (CodeToolSource as any)._cacheGeneratedAt = 0;
  });

  describe('interface properties', () => {
    it('has correct name', () => {
      expect(source.name).toBe('code-tools');
    });

    it('has medium priority (50)', () => {
      expect(source.priority).toBe(50);
    });

    it('has 5% default budget', () => {
      expect(source.defaultBudgetPercent).toBe(5);
    });
  });

  describe('isApplicable', () => {
    it('returns true when persona has code/* tools', () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
        fakeTool('collaboration/chat/send'),
      ]);

      expect(source.isApplicable(makeContext())).toBe(true);
    });

    it('returns false when persona has no code/* tools', () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('collaboration/chat/send'),
        fakeTool('data/list'),
        fakeTool('screenshot'),
      ]);

      expect(source.isApplicable(makeContext())).toBe(false);
    });

    it('returns false when persona has zero tools', () => {
      mockToolsForPersona.mockReturnValue([]);

      expect(source.isApplicable(makeContext())).toBe(false);
    });
  });

  describe('load', () => {
    it('returns coding workflow guidance when persona has code tools', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
        fakeTool('code/edit'),
        fakeTool('code/tree'),
        fakeTool('code/search'),
        fakeTool('code/diff'),
        fakeTool('code/undo'),
        fakeTool('code/history'),
      ]);

      const section = await source.load(makeContext(), 500);

      expect(section.sourceName).toBe('code-tools');
      expect(section.tokenCount).toBeGreaterThan(0);
      expect(section.loadTimeMs).toBeGreaterThanOrEqual(0);
      expect(section.systemPromptSection).toBeDefined();
    });

    it('includes workflow steps matching available tool groups', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
        fakeTool('code/edit'),
        fakeTool('code/tree'),
        fakeTool('code/search'),
        fakeTool('code/diff'),
        fakeTool('code/undo'),
        fakeTool('code/history'),
      ]);

      const section = await source.load(makeContext(), 500);
      const prompt = section.systemPromptSection!;

      // Each tool group has a corresponding workflow step
      expect(prompt).toContain('**Discover**');
      expect(prompt).toContain('**Read**');
      expect(prompt).toContain('**Preview**');
      expect(prompt).toContain('**Edit**');
      expect(prompt).toContain('**Undo**');
      // Numbered steps
      expect(prompt).toMatch(/1\. \*\*Discover\*\*/);
      expect(prompt).toMatch(/2\. \*\*Read\*\*/);
    });

    it('includes code/* command names in grouped sections', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/tree'),
        fakeTool('code/search'),
        fakeTool('code/edit'),
        fakeTool('code/diff'),
        fakeTool('code/undo'),
        fakeTool('code/history'),
      ]);

      const section = await source.load(makeContext(), 500);
      const prompt = section.systemPromptSection!;

      // Check grouped tool names
      expect(prompt).toContain('code/tree');
      expect(prompt).toContain('code/search');
      expect(prompt).toContain('code/read');
      expect(prompt).toContain('code/edit');
      expect(prompt).toContain('code/diff');
      expect(prompt).toContain('code/undo');
      expect(prompt).toContain('code/history');
    });

    it('only includes tools the persona has access to', async () => {
      // Persona only has read and search — no write/edit/diff/undo/history
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/search'),
        fakeTool('code/tree'),
      ]);

      const section = await source.load(makeContext(), 500);
      const prompt = section.systemPromptSection!;

      // Available tools appear in grouped sections
      expect(prompt).toContain('code/read');
      expect(prompt).toContain('code/search');
      expect(prompt).toContain('code/tree');

      // Unavailable tool groups should not appear — neither in groups nor workflow steps
      expect(prompt).not.toContain('code/write');
      expect(prompt).not.toContain('code/edit');
      expect(prompt).not.toContain('code/diff');
      expect(prompt).not.toContain('code/undo');
      expect(prompt).not.toContain('code/history');

      // Change graph note should not appear for read-only personas
      expect(prompt).not.toContain('change graph');
    });

    it('includes metadata with code tool count', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
        fakeTool('code/edit'),
        fakeTool('collaboration/chat/send'), // not a code tool
      ]);

      const section = await source.load(makeContext(), 500);

      expect(section.metadata).toBeDefined();
      expect(section.metadata!.codeToolCount).toBe(3);
    });

    it('returns minimal prompt when budget is very tight', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
        fakeTool('code/edit'),
        fakeTool('code/tree'),
        fakeTool('code/search'),
        fakeTool('code/diff'),
        fakeTool('code/undo'),
        fakeTool('code/history'),
      ]);

      // Allocate almost zero budget — forces minimal prompt
      const section = await source.load(makeContext(), 10);
      const prompt = section.systemPromptSection!;

      // Minimal prompt should be a compact one-liner
      expect(prompt.length).toBeLessThan(200);
      expect(prompt).toContain('Code tools available');
      expect(prompt).toContain('Read before editing');
    });

    it('returns empty section on error', async () => {
      mockToolsForPersona.mockImplementation(() => {
        throw new Error('Registry unavailable');
      });

      const section = await source.load(makeContext(), 500);

      expect(section.sourceName).toBe('code-tools');
      expect(section.tokenCount).toBe(0);
      expect(section.metadata).toHaveProperty('error');
    });
  });

  describe('caching', () => {
    it('caches the prompt on first load', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
      ]);

      const section1 = await source.load(makeContext(), 500);
      const section2 = await source.load(makeContext(), 500);

      // Both should have identical content
      expect(section1.systemPromptSection).toBe(section2.systemPromptSection);
      // Second load should be faster (cache hit)
      // Not strictly testing timing, but verifying the cache path works
    });

    it('invalidates cache after TTL', async () => {
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
      ]);

      await source.load(makeContext(), 500);

      // Force cache expiry
      (CodeToolSource as any)._cacheGeneratedAt = Date.now() - 11 * 60 * 1000; // 11 min ago

      // Now add more tools
      mockToolsForPersona.mockReturnValue([
        fakeTool('code/read'),
        fakeTool('code/write'),
        fakeTool('code/edit'),
      ]);

      const section = await source.load(makeContext(), 500);

      // Should reflect the new tools
      expect(section.systemPromptSection).toContain('code/write');
      expect(section.systemPromptSection).toContain('code/edit');
    });
  });
});
