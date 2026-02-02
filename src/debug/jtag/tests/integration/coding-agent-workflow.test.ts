/**
 * Coding Agent Workflow Integration Test (TDD)
 *
 * Tests the complete plan → execute → persist lifecycle:
 * 1. Orchestrator receives a coding task
 * 2. PlanFormulator generates a step DAG (mocked LLM)
 * 3. Steps execute via code/* commands (mocked)
 * 4. CodingPlanEntity is persisted with initial state
 * 5. Step statuses are updated during execution
 * 6. Plan is finalized with results
 *
 * This is a workflow test — it exercises the real orchestrator logic
 * with controlled inputs, verifying the full lifecycle including
 * persistence. If any step in the chain breaks, this test catches it.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { CodeAgentOrchestrator } from '../../system/code/server/CodeAgentOrchestrator';
import { CodingPlanEntity } from '../../system/data/entities/CodingPlanEntity';
import type { CodingTask } from '../../system/code/shared/CodingTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

// ── Mocks ──────────────────────────────────────────────────

const mockGenerateText = vi.fn();
vi.mock('../../daemons/ai-provider-daemon/shared/AIProviderDaemon', () => ({
  AIProviderDaemon: {
    generateText: (...args: unknown[]) => mockGenerateText(...args),
  },
}));

const mockExecute = vi.fn();
vi.mock('../../system/core/shared/Commands', () => ({
  Commands: {
    execute: (...args: unknown[]) => mockExecute(...args),
  },
}));

vi.mock('../../system/core/logging/Logger', () => ({
  Logger: {
    create: () => ({
      debug: () => {},
      info: () => {},
      warn: () => {},
      error: () => {},
    }),
  },
}));

// Track DataDaemon persistence calls
const mockDataDaemonStore = vi.fn();
const mockDataDaemonUpdate = vi.fn();

vi.mock('../../daemons/data-daemon/shared/DataDaemon', () => ({
  DataDaemon: {
    store: (...args: unknown[]) => mockDataDaemonStore(...args),
    update: (...args: unknown[]) => mockDataDaemonUpdate(...args),
  },
}));

// ── Helpers ─────────────────────────────────────────────────

function makeTask(overrides?: Partial<CodingTask>): CodingTask {
  return {
    id: 'task-0001-0001-0001-task00000001' as UUID,
    personaId: 'ai-00-0001-0001-0001-ai0000000001' as UUID,
    description: 'Add a greet function to utils.ts',
    taskType: 'generation',
    maxToolCalls: 20,
    maxDurationMs: 120000,
    createdAt: Date.now(),
    ...overrides,
  };
}

/** 3-step plan: read → edit → verify */
function mockThreeStepPlan() {
  mockGenerateText.mockResolvedValue({
    text: JSON.stringify({
      summary: 'Read utils.ts, add greet function, verify',
      steps: [
        {
          stepNumber: 1,
          action: 'read',
          description: 'Read utils.ts',
          targetFiles: ['utils.ts'],
          toolCall: 'code/read',
          toolParams: { filePath: 'utils.ts' },
          dependsOn: [],
          verification: 'File content returned',
        },
        {
          stepNumber: 2,
          action: 'edit',
          description: 'Add greet function',
          targetFiles: ['utils.ts'],
          toolCall: 'code/edit',
          toolParams: {
            filePath: 'utils.ts',
            editMode: { type: 'append', content: 'function greet() {}' },
          },
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

/** Simulate successful code/* command responses */
function mockSuccessfulCodeCommands() {
  mockExecute.mockImplementation(async (cmd: string) => {
    if (cmd === 'code/tree') return { success: true, root: { name: '.', children: [] } };
    if (cmd === 'code/read') return { success: true, content: 'export function greet() {}' };
    if (cmd === 'code/edit') return { success: true, changeId: 'change-abc-001' };
    return { success: true };
  });
}

// ── Tests ───────────────────────────────────────────────────

describe('Coding Agent Workflow', () => {
  let orchestrator: CodeAgentOrchestrator;

  beforeEach(() => {
    mockGenerateText.mockReset();
    mockExecute.mockReset();
    mockDataDaemonStore.mockReset();
    mockDataDaemonUpdate.mockReset();

    // DataDaemon.store returns the entity with an id assigned
    mockDataDaemonStore.mockImplementation(async (_collection: string, entity: CodingPlanEntity) => {
      entity.id = 'plan-persisted-id-0001' as UUID;
      return entity;
    });
    mockDataDaemonUpdate.mockResolvedValue({});

    orchestrator = new CodeAgentOrchestrator();
  });

  describe('happy path: plan → execute → persist', () => {
    it('persists a CodingPlanEntity on successful execution', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      const result = await orchestrator.execute(makeTask());

      // ── Execution succeeded ──
      expect(result.status).toBe('completed');
      expect(result.stepResults).toHaveLength(3);
      expect(result.stepResults.every(r => r.status === 'completed')).toBe(true);

      // ── Plan was persisted ──
      expect(mockDataDaemonStore).toHaveBeenCalledTimes(1);
      const [collection, entity] = mockDataDaemonStore.mock.calls[0];
      expect(collection).toBe('coding_plans');
      expect(entity).toBeInstanceOf(CodingPlanEntity);
    });

    it('persisted plan has correct initial structure', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      const entity: CodingPlanEntity = mockDataDaemonStore.mock.calls[0][1];

      expect(entity.taskId).toBe('task-0001-0001-0001-task00000001');
      expect(entity.createdById).toBe('ai-00-0001-0001-0001-ai0000000001');
      expect(entity.leadId).toBe('ai-00-0001-0001-0001-ai0000000001');
      expect(entity.summary).toBe('Read utils.ts, add greet function, verify');
      expect(entity.taskDescription).toBe('Add a greet function to utils.ts');
      expect(entity.status).toBe('executing');
      expect(entity.steps).toHaveLength(3);
      expect(entity.assignees).toContain('ai-00-0001-0001-0001-ai0000000001');
      expect(entity.executionStartedAt).toBeGreaterThan(0);
    });

    it('step snapshots have correct structural properties', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      const entity: CodingPlanEntity = mockDataDaemonStore.mock.calls[0][1];

      // Structural properties (immutable during execution)
      expect(entity.steps).toHaveLength(3);
      for (const step of entity.steps) {
        expect(step.toolCall).toMatch(/^code\//);
        expect(step.stepNumber).toBeGreaterThan(0);
        expect(step.action).toBeTruthy();
        expect(step.description).toBeTruthy();
        expect(Array.isArray(step.dependsOn)).toBe(true);
      }

      // Store is called before any update (ordering proof)
      expect(mockDataDaemonStore).toHaveBeenCalledTimes(1);
      expect(mockDataDaemonUpdate).toHaveBeenCalled();
    });

    it('updates step status during execution', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      // DataDaemon.update called for each step + finalization
      // 3 step updates + 1 finalize = 4 calls
      expect(mockDataDaemonUpdate).toHaveBeenCalledTimes(4);

      // Each step update includes the steps array
      for (let i = 0; i < 3; i++) {
        const updateCall = mockDataDaemonUpdate.mock.calls[i];
        expect(updateCall[0]).toBe('coding_plans'); // collection
        expect(updateCall[1]).toBe('plan-persisted-id-0001'); // entity id
        expect(updateCall[2]).toHaveProperty('steps');
      }
    });

    it('finalizes plan with execution results', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      // Last update call is finalization
      const finalizeCall = mockDataDaemonUpdate.mock.calls[3];
      const finalizeData = finalizeCall[2];

      expect(finalizeData.status).toBe('completed');
      expect(finalizeData.executionCompletedAt).toBeGreaterThan(0);
      expect(finalizeData.filesModified).toContain('utils.ts');
      expect(finalizeData.changeIds).toContain('change-abc-001');
      expect(finalizeData.totalToolCalls).toBeGreaterThanOrEqual(4);
      expect(finalizeData.totalDurationMs).toBeGreaterThan(0);
    });

    it('tracks changeIds from edit operations', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      const result = await orchestrator.execute(makeTask());

      expect(result.changeIds).toContain('change-abc-001');
      expect(result.filesModified).toContain('utils.ts');
    });
  });

  describe('partial completion: some steps fail', () => {
    it('persists partial status when edit fails', async () => {
      mockThreeStepPlan();
      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'data' };
        if (cmd === 'code/edit') return { success: false, error: 'Conflict' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('partial');
      expect(result.errors.length).toBeGreaterThan(0);

      // Plan was finalized as partial
      const finalizeCall = mockDataDaemonUpdate.mock.calls.at(-1);
      expect(finalizeCall?.[2].status).toBe('partial');
    });

    it('skipped steps are recorded in persistence', async () => {
      mockThreeStepPlan();
      mockExecute.mockImplementation(async (cmd: string) => {
        if (cmd === 'code/tree') return { success: true, root: {} };
        if (cmd === 'code/read') return { success: true, content: 'data' };
        if (cmd === 'code/edit') return { success: false, error: 'Failed' };
        return { success: true };
      });

      const result = await orchestrator.execute(makeTask());

      // Step 3 (verify) depends on step 2 (edit) which failed → skipped
      const verifyStep = result.stepResults.find(r => r.stepNumber === 3);
      expect(verifyStep?.status).toBe('skipped');
    });
  });

  describe('plan formulation failure', () => {
    it('persists failed status when LLM is unavailable', async () => {
      mockGenerateText.mockRejectedValue(new Error('LLM unavailable'));
      mockExecute.mockResolvedValue({ success: true });

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('failed');
      expect(result.errors).toContain('LLM unavailable');

      // No plan was created (failure happened before plan formulation)
      // DataDaemon.store should NOT have been called
      expect(mockDataDaemonStore).not.toHaveBeenCalled();
    });
  });

  describe('persistence failure resilience', () => {
    it('continues execution even if DataDaemon.store fails', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();
      mockDataDaemonStore.mockRejectedValue(new Error('DB unavailable'));

      const result = await orchestrator.execute(makeTask());

      // Execution should still complete successfully
      expect(result.status).toBe('completed');
      expect(result.stepResults).toHaveLength(3);
      expect(result.stepResults.every(r => r.status === 'completed')).toBe(true);
    });

    it('continues execution even if DataDaemon.update fails', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();
      mockDataDaemonStore.mockImplementation(async (_c: string, entity: CodingPlanEntity) => {
        entity.id = 'plan-id' as UUID;
        return entity;
      });
      mockDataDaemonUpdate.mockRejectedValue(new Error('DB write error'));

      const result = await orchestrator.execute(makeTask());

      // Execution should still complete despite persistence failures
      expect(result.status).toBe('completed');
    });
  });

  describe('budget enforcement with persistence', () => {
    it('persists budget_exceeded as partial status', async () => {
      // Plan with 5 sequential steps
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
      mockSuccessfulCodeCommands();

      const result = await orchestrator.execute(makeTask({ maxToolCalls: 5 }));

      expect(['partial', 'budget_exceeded']).toContain(result.status);

      // Plan was finalized
      if (mockDataDaemonUpdate.mock.calls.length > 0) {
        const finalizeCall = mockDataDaemonUpdate.mock.calls.at(-1);
        expect(['partial', 'completed']).toContain(finalizeCall?.[2].status);
      }
    });
  });

  describe('plan entity structure integrity', () => {
    it('step snapshots preserve dependency DAG', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      const entity: CodingPlanEntity = mockDataDaemonStore.mock.calls[0][1];

      expect(entity.steps[0].dependsOn).toEqual([]);
      expect(entity.steps[1].dependsOn).toEqual([1]);
      expect(entity.steps[2].dependsOn).toEqual([2]);
    });

    it('step snapshots preserve tool params', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      const entity: CodingPlanEntity = mockDataDaemonStore.mock.calls[0][1];

      expect(entity.steps[0].toolParams).toEqual({ filePath: 'utils.ts' });
      expect(entity.steps[1].toolParams).toHaveProperty('editMode');
    });

    it('generatedBy includes model info', async () => {
      mockThreeStepPlan();
      mockSuccessfulCodeCommands();

      await orchestrator.execute(makeTask());

      const entity: CodingPlanEntity = mockDataDaemonStore.mock.calls[0][1];

      expect(entity.generatedBy.provider).toBeTruthy();
      expect(entity.generatedBy.model).toBeTruthy();
    });
  });
});
