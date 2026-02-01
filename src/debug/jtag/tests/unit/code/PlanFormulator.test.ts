/**
 * PlanFormulator Unit Tests
 *
 * Tests LLM plan generation by mocking AIProviderDaemon.
 * Validates:
 * - Prompt construction (system prompt, tool schemas, constraints)
 * - JSON plan parsing from LLM responses
 * - Plan validation (actions, dependencies, step numbers)
 * - Error handling for invalid LLM output
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { PlanFormulator } from '../../../system/code/server/PlanFormulator';
import { CodingModelSelector } from '../../../system/code/server/CodingModelSelector';
import type { CodingTask } from '../../../system/code/shared/CodingTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// Mock AIProviderDaemon
const mockGenerateText = vi.fn();
vi.mock('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon', () => ({
  AIProviderDaemon: {
    generateText: (...args: unknown[]) => mockGenerateText(...args),
  },
}));

// Mock Logger
vi.mock('../../../system/core/logging/Logger', () => ({
  Logger: {
    create: () => ({
      debug: () => {},
      info: () => {},
      warn: () => {},
      error: () => {},
    }),
  },
}));

function makeTask(overrides?: Partial<CodingTask>): CodingTask {
  return {
    id: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID,
    personaId: '11111111-2222-3333-4444-555555555555' as UUID,
    description: 'Add a greet function to utils.ts',
    taskType: 'generation',
    maxToolCalls: 15,
    maxDurationMs: 120000,
    createdAt: Date.now(),
    ...overrides,
  };
}

/** Helper: mock LLM returning a valid plan JSON */
function mockValidPlan() {
  mockGenerateText.mockResolvedValue({
    text: JSON.stringify({
      summary: 'Read utils.ts, add greet function, verify',
      steps: [
        {
          stepNumber: 1,
          action: 'read',
          description: 'Read current utils.ts contents',
          targetFiles: ['utils.ts'],
          toolCall: 'code/read',
          toolParams: { filePath: 'utils.ts' },
          dependsOn: [],
          verification: 'File contents returned',
        },
        {
          stepNumber: 2,
          action: 'edit',
          description: 'Add greet function to utils.ts',
          targetFiles: ['utils.ts'],
          toolCall: 'code/edit',
          toolParams: {
            filePath: 'utils.ts',
            editMode: { type: 'append', content: '\nexport function greet(name: string): string {\n  return `Hello, ${name}!`;\n}\n' },
            description: 'Add greet function',
          },
          dependsOn: [1],
          verification: 'Edit applied successfully',
        },
        {
          stepNumber: 3,
          action: 'verify',
          description: 'Read back to verify greet function added',
          targetFiles: ['utils.ts'],
          toolCall: 'code/read',
          toolParams: { filePath: 'utils.ts' },
          dependsOn: [2],
          verification: 'greet function present in file',
        },
      ],
    }),
    usage: { inputTokens: 500, outputTokens: 200 },
  });
}

describe('PlanFormulator', () => {
  let formulator: PlanFormulator;

  beforeEach(() => {
    mockGenerateText.mockReset();
    const selector = new CodingModelSelector(new Set(['anthropic', 'deepseek', 'groq']));
    formulator = new PlanFormulator(selector);
  });

  describe('formulate', () => {
    it('generates a valid plan from LLM response', async () => {
      mockValidPlan();

      const plan = await formulator.formulate(makeTask());

      expect(plan.taskId).toBe('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee');
      expect(plan.summary).toBe('Read utils.ts, add greet function, verify');
      expect(plan.steps).toHaveLength(3);
      expect(plan.estimatedToolCalls).toBe(3);
      expect(plan.generatedBy.provider).toBe('anthropic');
      expect(plan.generatedAt).toBeGreaterThan(0);
    });

    it('preserves step structure from LLM', async () => {
      mockValidPlan();

      const plan = await formulator.formulate(makeTask());
      const step1 = plan.steps[0];

      expect(step1.stepNumber).toBe(1);
      expect(step1.action).toBe('read');
      expect(step1.toolCall).toBe('code/read');
      expect(step1.targetFiles).toEqual(['utils.ts']);
      expect(step1.dependsOn).toEqual([]);
    });

    it('validates dependency ordering', async () => {
      mockValidPlan();

      const plan = await formulator.formulate(makeTask());

      expect(plan.steps[1].dependsOn).toEqual([1]); // edit depends on read
      expect(plan.steps[2].dependsOn).toEqual([2]); // verify depends on edit
    });

    it('passes task description to LLM', async () => {
      mockValidPlan();

      await formulator.formulate(makeTask({ description: 'Refactor auth module' }));

      expect(mockGenerateText).toHaveBeenCalledTimes(1);
      const request = mockGenerateText.mock.calls[0][0];
      const userMessage = request.messages.find((m: any) => m.role === 'user' && m.content.includes('Refactor auth module'));
      expect(userMessage).toBeDefined();
    });

    it('includes tool schemas in system prompt', async () => {
      mockValidPlan();

      await formulator.formulate(makeTask());

      const request = mockGenerateText.mock.calls[0][0];
      const systemMsg = request.messages.find((m: any) => m.role === 'system');
      expect(systemMsg.content).toContain('code/tree');
      expect(systemMsg.content).toContain('code/read');
      expect(systemMsg.content).toContain('code/write');
      expect(systemMsg.content).toContain('code/edit');
      expect(systemMsg.content).toContain('code/search');
    });

    it('includes constraints in system prompt', async () => {
      mockValidPlan();

      await formulator.formulate(makeTask({ maxToolCalls: 10, maxDurationMs: 60000 }));

      const request = mockGenerateText.mock.calls[0][0];
      const systemMsg = request.messages.find((m: any) => m.role === 'system');
      expect(systemMsg.content).toContain('10'); // max tool calls
      expect(systemMsg.content).toContain('60'); // 60 seconds
    });

    it('includes codebase context when provided', async () => {
      mockValidPlan();

      await formulator.formulate(makeTask(), '## Workspace Tree\nsrc/\n  utils.ts (200 bytes)');

      const request = mockGenerateText.mock.calls[0][0];
      const contextMsg = request.messages.find((m: any) => m.content?.includes('Workspace Tree'));
      expect(contextMsg).toBeDefined();
    });

    it('includes relevant files when specified', async () => {
      mockValidPlan();

      await formulator.formulate(makeTask({ relevantFiles: ['src/utils.ts', 'src/auth.ts'] }));

      const request = mockGenerateText.mock.calls[0][0];
      const filesMsg = request.messages.find((m: any) => m.content?.includes('src/utils.ts'));
      expect(filesMsg).toBeDefined();
    });
  });

  describe('error handling', () => {
    it('throws on empty LLM response', async () => {
      mockGenerateText.mockResolvedValue({ text: '' });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('empty response');
    });

    it('throws on non-JSON response', async () => {
      mockGenerateText.mockResolvedValue({ text: 'I think we should...' });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('No JSON object');
    });

    it('throws on missing summary', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({ steps: [{ stepNumber: 1, action: 'read' }] }),
      });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('missing "summary"');
    });

    it('throws on empty steps array', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({ summary: 'Do stuff', steps: [] }),
      });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('no steps');
    });

    it('throws on too many steps', async () => {
      const manySteps = Array.from({ length: 20 }, (_, i) => ({
        stepNumber: i + 1,
        action: 'read',
        toolCall: 'code/read',
        toolParams: {},
        dependsOn: [],
      }));

      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({ summary: 'Too many', steps: manySteps }),
      });

      await expect(formulator.formulate(makeTask({ maxToolCalls: 15 }))).rejects.toThrow('exceeds max');
    });

    it('throws on invalid action', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Bad action',
          steps: [{ stepNumber: 1, action: 'hack', toolCall: 'code/read', dependsOn: [] }],
        }),
      });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('invalid action');
    });

    it('throws on invalid toolCall', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Bad tool',
          steps: [{ stepNumber: 1, action: 'read', toolCall: 'rm -rf', dependsOn: [] }],
        }),
      });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('not a code/* command');
    });

    it('throws on forward dependency reference', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Bad deps',
          steps: [
            { stepNumber: 1, action: 'read', toolCall: 'code/read', dependsOn: [2] },
            { stepNumber: 2, action: 'read', toolCall: 'code/read', dependsOn: [] },
          ],
        }),
      });

      await expect(formulator.formulate(makeTask())).rejects.toThrow('invalid step');
    });

    it('extracts JSON from markdown code blocks', async () => {
      const planJson = JSON.stringify({
        summary: 'Wrapped in markdown',
        steps: [{
          stepNumber: 1,
          action: 'read',
          toolCall: 'code/read',
          toolParams: { filePath: 'test.ts' },
          dependsOn: [],
        }],
      });

      mockGenerateText.mockResolvedValue({
        text: `Here's the plan:\n\`\`\`json\n${planJson}\n\`\`\``,
      });

      const plan = await formulator.formulate(makeTask());
      expect(plan.summary).toBe('Wrapped in markdown');
      expect(plan.steps).toHaveLength(1);
    });
  });
});
