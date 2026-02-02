/**
 * CodeAgentOrchestrator Unit Tests
 *
 * Tests the execution engine by mocking PlanFormulator and Commands.execute.
 * Validates:
 * - Step execution in dependency order
 * - Budget enforcement (time and tool calls)
 * - Retry logic on step failure
 * - Result aggregation (filesModified, changeIds, errors)
 * - Graceful degradation on partial completion
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { CodeAgentOrchestrator } from '../../../system/code/server/CodeAgentOrchestrator';
import type { CodingTask } from '../../../system/code/shared/CodingTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// Mock AIProviderDaemon (used by PlanFormulator)
const mockGenerateText = vi.fn();
vi.mock('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon', () => ({
  AIProviderDaemon: {
    generateText: (...args: unknown[]) => mockGenerateText(...args),
  },
}));

// Mock Commands.execute (used by orchestrator for code/* calls)
const mockExecute = vi.fn();
vi.mock('../../../system/core/shared/Commands', () => ({
  Commands: {
    execute: (...args: unknown[]) => mockExecute(...args),
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

// Mock CodeDaemon.createWorkspace (workspace bootstrap)
vi.mock('../../../daemons/code-daemon/shared/CodeDaemon', () => ({
  CodeDaemon: {
    createWorkspace: vi.fn().mockResolvedValue(undefined),
  },
}));

// Mock fs for workspace directory creation + CLAUDE.md reading
vi.mock('fs', () => ({
  existsSync: vi.fn().mockReturnValue(true),
  mkdirSync: vi.fn(),
  readFileSync: vi.fn().mockReturnValue('# Project Conventions\nCompression principle applies.'),
}));

function makeTask(overrides?: Partial<CodingTask>): CodingTask {
  return {
    id: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID,
    personaId: '11111111-2222-3333-4444-555555555555' as UUID,
    description: 'Add a greet function to utils.ts',
    taskType: 'generation',
    maxToolCalls: 20,
    maxDurationMs: 120000,
    createdAt: Date.now(),
    ...overrides,
  };
}

/** Mock PlanFormulator returning a simple 3-step plan */
function mockSimplePlan() {
  mockGenerateText.mockResolvedValue({
    text: JSON.stringify({
      summary: 'Read, edit, verify',
      steps: [
        {
          stepNumber: 1,
          action: 'read',
          description: 'Read utils.ts',
          targetFiles: ['utils.ts'],
          toolCall: 'code/read',
          toolParams: { filePath: 'utils.ts' },
          dependsOn: [],
          verification: 'File read',
        },
        {
          stepNumber: 2,
          action: 'edit',
          description: 'Add greet function',
          targetFiles: ['utils.ts'],
          toolCall: 'code/edit',
          toolParams: { filePath: 'utils.ts', editMode: { type: 'append', content: 'function greet() {}' } },
          dependsOn: [1],
          verification: 'Edit applied',
        },
        {
          stepNumber: 3,
          action: 'verify',
          description: 'Verify changes',
          targetFiles: ['utils.ts'],
          toolCall: 'code/read',
          toolParams: { filePath: 'utils.ts' },
          dependsOn: [2],
          verification: 'greet function present',
        },
      ],
    }),
  });
}

describe('CodeAgentOrchestrator', () => {
  let orchestrator: CodeAgentOrchestrator;

  beforeEach(() => {
    mockGenerateText.mockReset();
    mockExecute.mockReset();
    orchestrator = new CodeAgentOrchestrator();
  });

  describe('execute - happy path', () => {
    it('executes all plan steps and returns completed', async () => {
      mockSimplePlan();

      // Use mockImplementation to handle discovery + architecture doc reads + plan steps
      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'file content' };
        if (cmd === 'code/edit') return { success: true, changeId: 'c1' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('completed');
      expect(result.stepResults).toHaveLength(3);
      expect(result.stepResults.every(r => r.status === 'completed')).toBe(true);
      expect(result.totalToolCalls).toBeGreaterThanOrEqual(4); // 1 discovery + arch reads + 3 steps
    });

    it('tracks modified files from edit steps', async () => {
      mockSimplePlan();

      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'file content' };
        if (cmd === 'code/edit') return { success: true, changeId: 'change-123' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      expect(result.filesModified).toContain('utils.ts');
      expect(result.changeIds).toContain('change-123');
    });

    it('includes execution timing', async () => {
      mockSimplePlan();
      mockExecute.mockResolvedValue({ success: true });

      const result = await orchestrator.execute(makeTask());

      expect(result.totalDurationMs).toBeGreaterThan(0);
      for (const step of result.stepResults) {
        expect(step.durationMs).toBeGreaterThanOrEqual(0);
      }
    });
  });

  describe('budget enforcement', () => {
    it('stops when max tool calls exceeded', async () => {
      mockSimplePlan();

      // Task with only 2 tool calls allowed (discovery uses 1, only 1 left for plan)
      mockExecute.mockResolvedValue({ success: true });

      const result = await orchestrator.execute(makeTask({ maxToolCalls: 3 }));

      // Should have stopped partway through
      expect(result.totalToolCalls).toBeLessThanOrEqual(3);
      const skipped = result.stepResults.filter(r => r.status === 'skipped');
      expect(skipped.length).toBeGreaterThan(0);
    });

    it('reports partial or budget_exceeded when budget runs out mid-execution', async () => {
      // Plan with 5 steps (within maxToolCalls for formulation)
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Five reads',
          steps: Array.from({ length: 5 }, (_, i) => ({
            stepNumber: i + 1,
            action: 'read',
            targetFiles: [`file${i}.ts`],
            toolCall: 'code/read',
            toolParams: { filePath: `file${i}.ts` },
            dependsOn: i > 0 ? [i] : [],
            verification: 'ok',
          })),
        }),
      });

      mockExecute.mockResolvedValue({ success: true });

      // 5 tool calls total: 1 for discovery leaves 4 for 5 plan steps = can't finish all
      const result = await orchestrator.execute(makeTask({ maxToolCalls: 5 }));

      // Some steps completed, some skipped due to budget
      expect(['partial', 'budget_exceeded']).toContain(result.status);
      const skipped = result.stepResults.filter(r => r.status === 'skipped');
      expect(skipped.length).toBeGreaterThan(0);
    });
  });

  describe('step failure and retry', () => {
    it('retries failed steps up to 3 times', async () => {
      mockSimplePlan();

      let callCount = 0;
      mockExecute.mockImplementation(async (cmd: string) => {
        callCount++;
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'data' };
        if (cmd === 'code/edit') {
          // Fail first 2 times, succeed on 3rd
          if (callCount <= 4) return { success: false, error: 'Conflict' };
          return { success: true, changeId: 'c1' };
        }
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      // Step 2 (edit) should have retried and eventually succeeded
      const editStep = result.stepResults.find(r => r.toolCall === 'code/edit');
      expect(editStep?.status).toBe('completed');
    });

    it('marks step as failed after max retries', async () => {
      mockSimplePlan();

      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'data' };
        if (cmd === 'code/edit') return { success: false, error: 'Always fails' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      const editStep = result.stepResults.find(r => r.toolCall === 'code/edit');
      expect(editStep?.status).toBe('failed');
      expect(editStep?.error).toContain('Always fails');
    });

    it('skips dependent steps when dependency fails', async () => {
      mockSimplePlan();

      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'data' };
        if (cmd === 'code/edit') return { success: false, error: 'Edit failed' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      // Step 3 (verify) depends on step 2 (edit) which failed
      const verifyStep = result.stepResults.find(r => r.stepNumber === 3);
      expect(verifyStep?.status).toBe('skipped');
      expect(verifyStep?.error).toContain('Dependencies not met');
    });

    it('returns partial status when some steps succeed', async () => {
      mockSimplePlan();

      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'data' };
        if (cmd === 'code/edit') return { success: false, error: 'Failed' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('partial');
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe('error handling', () => {
    it('handles plan formulation failure gracefully', async () => {
      mockGenerateText.mockRejectedValue(new Error('LLM unavailable'));
      mockExecute.mockResolvedValue({ success: true });

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('failed');
      expect(result.errors).toContain('LLM unavailable');
    });

    it('handles command execution exception', async () => {
      mockSimplePlan();

      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') throw new Error('Connection lost');
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      // Step 1 (read) should fail with exception
      const readStep = result.stepResults.find(r => r.stepNumber === 1);
      expect(readStep?.status).toBe('failed');
      expect(readStep?.error).toContain('Connection lost');
    });
  });

  describe('dryRun mode', () => {
    it('executes read steps normally in dryRun', async () => {
      mockSimplePlan();
      mockExecute
        .mockResolvedValueOnce({ success: true, root: {} })          // code/tree (discovery)
        .mockResolvedValueOnce({ success: true, content: 'old' })    // step 1: code/read
        .mockResolvedValue({ success: true, content: 'data' });      // remaining reads

      const result = await orchestrator.execute(makeTask(), { dryRun: true });

      // Step 1 (read) should execute normally
      const readStep = result.stepResults.find(r => r.stepNumber === 1);
      expect(readStep?.status).toBe('completed');
    });

    it('mocks write/edit steps in dryRun', async () => {
      mockSimplePlan();
      mockExecute
        .mockResolvedValueOnce({ success: true, root: {} })          // code/tree (discovery)
        .mockResolvedValueOnce({ success: true, content: 'old' })    // step 1: code/read
        .mockResolvedValue({ success: true, content: 'data' });      // step 3: verify read

      const result = await orchestrator.execute(makeTask(), { dryRun: true });

      // Step 2 (edit) should be mocked — completed but with dryRun flag
      const editStep = result.stepResults.find(r => r.stepNumber === 2);
      expect(editStep?.status).toBe('completed');

      const output = editStep?.output as Record<string, unknown>;
      expect(output?.dryRun).toBe(true);
      expect(output?.wouldModify).toEqual(['utils.ts']);
    });

    it('dryRun does not call Commands.execute for write steps', async () => {
      mockSimplePlan();

      const callLog: string[] = [];
      mockExecute.mockImplementation(async (cmd: string) => {
        callLog.push(cmd);
        if (cmd === 'code/tree') return { success: true, root: {} };
        return { success: true, content: 'data' };
      });

      await orchestrator.execute(makeTask(), { dryRun: true });

      // code/edit should NOT appear in call log
      expect(callLog).not.toContain('code/edit');
      // code/read and code/tree should appear
      expect(callLog).toContain('code/tree');
      expect(callLog).toContain('code/read');
    });

    it('dryRun completes all steps successfully', async () => {
      mockSimplePlan();
      mockExecute.mockResolvedValue({ success: true, content: 'data', root: {} });

      const result = await orchestrator.execute(makeTask(), { dryRun: true });

      expect(result.status).toBe('completed');
      expect(result.stepResults.every(r => r.status === 'completed')).toBe(true);
    });

    it('dryRun does not produce changeIds', async () => {
      mockSimplePlan();
      mockExecute.mockResolvedValue({ success: true, content: 'data', root: {} });

      const result = await orchestrator.execute(makeTask(), { dryRun: true });

      // No real writes happened, so no changeIds
      expect(result.changeIds).toHaveLength(0);
    });
  });

  describe('verify→re-plan iteration loop', () => {
    it('skips verification when autoVerify is false', async () => {
      mockSimplePlan();
      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'file content' };
        if (cmd === 'code/edit') return { success: true, changeId: 'c1' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask(), { autoVerify: false });

      expect(result.status).toBe('completed');
      // code/verify should NOT have been called
      const calls = mockExecute.mock.calls.map((c: unknown[]) => c[0]);
      expect(calls).not.toContain('code/verify');
    });

    it('skips verification in dryRun mode', async () => {
      mockSimplePlan();
      mockExecute.mockResolvedValue({ success: true, content: 'data', root: {} });

      const result = await orchestrator.execute(makeTask(), { dryRun: true });

      // code/verify should NOT have been called
      const calls = mockExecute.mock.calls.map((c: unknown[]) => c[0]);
      expect(calls).not.toContain('code/verify');
    });

    it('runs verification after write steps and passes', async () => {
      mockSimplePlan();
      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'file content' };
        if (cmd === 'code/edit') return { success: true, changeId: 'c1' };
        if (cmd === 'code/verify') return { success: true, typeCheck: { passed: true, errorCount: 0, errors: [] } };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('completed');
      expect(result.errors).toHaveLength(0);
      const calls = mockExecute.mock.calls.map((c: unknown[]) => c[0]);
      expect(calls).toContain('code/verify');
    });

    it('records errors when verification fails and iterations exhausted', async () => {
      mockSimplePlan();

      // First call for planning, then always fail verification
      let verifyCallCount = 0;
      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'file content' };
        if (cmd === 'code/edit') return { success: true, changeId: 'c1' };
        if (cmd === 'code/verify') {
          verifyCallCount++;
          return {
            success: false,
            typeCheck: {
              passed: false,
              errorCount: 1,
              errors: [{ file: 'utils.ts', line: 5, column: 1, code: 'TS2345', message: 'Type error' }],
            },
          };
        }
        return { success: true };
      });

      // Allow re-plan — the LLM mock needs to return a fix plan too
      mockGenerateText
        .mockResolvedValueOnce({
          text: JSON.stringify({
            summary: 'Original plan',
            steps: [
              { stepNumber: 1, action: 'read', targetFiles: ['utils.ts'], toolCall: 'code/read', toolParams: { filePath: 'utils.ts' }, dependsOn: [], verification: 'ok' },
              { stepNumber: 2, action: 'edit', targetFiles: ['utils.ts'], toolCall: 'code/edit', toolParams: { filePath: 'utils.ts', editType: 'append', content: 'x' }, dependsOn: [1], verification: 'ok' },
            ],
          }),
        })
        .mockResolvedValueOnce({
          text: JSON.stringify({
            summary: 'Fix type error',
            steps: [
              { stepNumber: 1, action: 'edit', targetFiles: ['utils.ts'], toolCall: 'code/edit', toolParams: { filePath: 'utils.ts', editType: 'search_replace', search: 'x', replace: 'y' }, dependsOn: [], verification: 'ok' },
            ],
          }),
        });

      const result = await orchestrator.execute(makeTask({ maxToolCalls: 30 }), { maxVerifyIterations: 2 });

      // Should have verification errors recorded
      expect(result.errors.some((e: string) => e.includes('TS2345'))).toBe(true);
      // Should have called verify at least twice (initial + after fix)
      expect(verifyCallCount).toBeGreaterThanOrEqual(2);
    });
  });
});
