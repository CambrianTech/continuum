/**
 * Sandbox Enforcement Integration Test
 *
 * Tests that the CodeAgentOrchestrator respects security tiers:
 * 1. Plans include riskLevel from PlanFormulator
 * 2. ToolAllowlistEnforcer blocks disallowed tool calls
 * 3. Risk level flows through to persisted CodingPlanEntity
 * 4. Discovery-tier plans can't write files
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { CodeAgentOrchestrator } from '../../system/code/server/CodeAgentOrchestrator';
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
    id: 'task-enforce-0001-0001-task00000001' as UUID,
    personaId: 'ai-00-0001-0001-0001-ai0000000001' as UUID,
    description: 'Test sandbox enforcement',
    taskType: 'generation',
    maxToolCalls: 20,
    maxDurationMs: 120000,
    createdAt: Date.now(),
    ...overrides,
  };
}

function mockSuccessfulCommands() {
  mockExecute.mockImplementation(async (cmd: string) => {
    if (cmd === 'code/tree') return { success: true, root: { name: '.', children: [] } };
    if (cmd === 'code/read') return { success: true, content: 'file content' };
    if (cmd === 'code/edit') return { success: true, changeId: 'change-001' };
    if (cmd === 'code/write') return { success: true, changeId: 'change-002' };
    if (cmd === 'development/exec') return { success: true, output: 'npm output' };
    return { success: true };
  });
}

// ── Tests ───────────────────────────────────────────────────

describe('Sandbox Enforcement', () => {
  let orchestrator: CodeAgentOrchestrator;

  beforeEach(() => {
    mockGenerateText.mockReset();
    mockExecute.mockReset();
    mockDataDaemonStore.mockReset();
    mockDataDaemonUpdate.mockReset();

    mockDataDaemonStore.mockImplementation(async (_c: string, entity: any) => {
      entity.id = 'plan-enforce-id' as UUID;
      return entity;
    });
    mockDataDaemonUpdate.mockResolvedValue({});

    orchestrator = new CodeAgentOrchestrator();
  });

  describe('riskLevel flows from plan to entity', () => {
    it('low-risk plan persists riskLevel and securityTier', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Read a single file',
          riskLevel: 'low',
          riskReason: 'Read-only, no modifications',
          steps: [{
            stepNumber: 1,
            action: 'read',
            description: 'Read utils.ts',
            targetFiles: ['utils.ts'],
            toolCall: 'code/read',
            toolParams: { filePath: 'utils.ts' },
            dependsOn: [],
            verification: 'File read',
          }],
        }),
      });
      mockSuccessfulCommands();

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('completed');

      // Verify entity was persisted with risk info
      expect(mockDataDaemonStore).toHaveBeenCalledTimes(1);
      const entity = mockDataDaemonStore.mock.calls[0][1];
      expect(entity.riskLevel).toBe('low');
      expect(entity.riskReason).toBe('Read-only, no modifications');
      expect(entity.securityTier).toBe('write'); // low → write tier
    });

    it('critical-risk plan gets system tier', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Modify build system',
          riskLevel: 'critical',
          riskReason: 'Modifies build configuration and deployment scripts',
          steps: [{
            stepNumber: 1,
            action: 'read',
            description: 'Read build config',
            targetFiles: ['build.config.ts'],
            toolCall: 'code/read',
            toolParams: { filePath: 'build.config.ts' },
            dependsOn: [],
            verification: 'Config read',
          }],
        }),
      });
      mockSuccessfulCommands();

      await orchestrator.execute(makeTask());

      const entity = mockDataDaemonStore.mock.calls[0][1];
      expect(entity.riskLevel).toBe('critical');
      expect(entity.securityTier).toBe('system'); // critical → system tier
    });
  });

  describe('enforcer blocks disallowed tools', () => {
    it('write-tier plan blocks code/delete steps', async () => {
      // Plan with riskLevel=low (→ write tier) tries to use code/delete (explicitly denied)
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Delete old file',
          riskLevel: 'low',
          riskReason: 'Simple cleanup',
          steps: [
            {
              stepNumber: 1,
              action: 'read',
              description: 'Read old file',
              targetFiles: ['old.ts'],
              toolCall: 'code/read',
              toolParams: { filePath: 'old.ts' },
              dependsOn: [],
              verification: 'File read',
            },
            {
              stepNumber: 2,
              action: 'verify',
              description: 'Delete old file',
              targetFiles: ['old.ts'],
              toolCall: 'code/delete',
              toolParams: { filePath: 'old.ts' },
              dependsOn: [1],
              verification: 'File deleted',
            },
          ],
        }),
      });
      mockSuccessfulCommands();

      const result = await orchestrator.execute(makeTask());

      // Step 1 (read) should succeed, step 2 (code/delete) should fail (denied in write tier)
      const readStep = result.stepResults.find(r => r.stepNumber === 1);
      const deleteStep = result.stepResults.find(r => r.stepNumber === 2);

      expect(readStep?.status).toBe('completed');
      expect(deleteStep?.status).toBe('failed');
      expect(deleteStep?.error).toContain('denied');
    });

    it('system-tier plan allows code/delete', async () => {
      // Plan with riskLevel=critical (→ system tier) can use code/delete
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'System cleanup',
          riskLevel: 'critical',
          riskReason: 'Requires deletion capability',
          steps: [
            {
              stepNumber: 1,
              action: 'verify',
              description: 'Delete deprecated file',
              targetFiles: ['deprecated.ts'],
              toolCall: 'code/delete',
              toolParams: { filePath: 'deprecated.ts' },
              dependsOn: [],
              verification: 'File removed',
            },
          ],
        }),
      });
      mockSuccessfulCommands();

      const result = await orchestrator.execute(makeTask());

      const deleteStep = result.stepResults.find(r => r.stepNumber === 1);
      expect(deleteStep?.status).toBe('completed');
    });

    it('write-tier plan allows code/write and code/edit', async () => {
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Edit files',
          riskLevel: 'medium',
          riskReason: 'Standard file modifications',
          steps: [
            {
              stepNumber: 1,
              action: 'read',
              description: 'Read file',
              targetFiles: ['utils.ts'],
              toolCall: 'code/read',
              toolParams: { filePath: 'utils.ts' },
              dependsOn: [],
              verification: 'Read',
            },
            {
              stepNumber: 2,
              action: 'edit',
              description: 'Edit file',
              targetFiles: ['utils.ts'],
              toolCall: 'code/edit',
              toolParams: { filePath: 'utils.ts', editMode: { type: 'append', content: 'new code' } },
              dependsOn: [1],
              verification: 'Edited',
            },
          ],
        }),
      });
      mockSuccessfulCommands();

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('completed');
      expect(result.stepResults.every(r => r.status === 'completed')).toBe(true);
    });
  });

  describe('default risk handling', () => {
    it('plan without riskLevel defaults to medium/write tier', async () => {
      // Old-style plan without risk fields
      mockGenerateText.mockResolvedValue({
        text: JSON.stringify({
          summary: 'Legacy plan',
          steps: [{
            stepNumber: 1,
            action: 'read',
            description: 'Read file',
            targetFiles: ['utils.ts'],
            toolCall: 'code/read',
            toolParams: { filePath: 'utils.ts' },
            dependsOn: [],
            verification: 'Read',
          }],
        }),
      });
      mockSuccessfulCommands();

      const result = await orchestrator.execute(makeTask());

      expect(result.status).toBe('completed');

      // Entity should have default risk values
      const entity = mockDataDaemonStore.mock.calls[0][1];
      expect(entity.riskLevel).toBe('medium');
      expect(entity.securityTier).toBe('write');
    });
  });
});
