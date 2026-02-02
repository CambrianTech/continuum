/**
 * CodingPlanEntity Unit Tests
 *
 * Tests the persistent coding plan entity:
 * - Construction and default values
 * - Validation (required fields, step structure, status enum)
 * - Computed properties (progress, stepsCompleted, isDelegated)
 * - Hierarchical plan relationships
 * - Collection and pagination config
 */

import { describe, it, expect } from 'vitest';
import {
  CodingPlanEntity,
  type CodingStepSnapshot,
  type CodingPlanStatus,
} from '../../../system/data/entities/CodingPlanEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

function makeStep(overrides?: Partial<CodingStepSnapshot>): CodingStepSnapshot {
  return {
    stepNumber: 1,
    action: 'read',
    description: 'Read file',
    targetFiles: ['utils.ts'],
    toolCall: 'code/read',
    toolParams: { filePath: 'utils.ts' },
    dependsOn: [],
    verification: 'File content returned',
    status: 'pending',
    ...overrides,
  };
}

function makePlan(overrides?: Partial<CodingPlanEntity>): CodingPlanEntity {
  const plan = new CodingPlanEntity();
  plan.taskId = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID;
  plan.createdById = '11111111-2222-3333-4444-555555555555' as UUID;
  plan.leadId = '11111111-2222-3333-4444-555555555555' as UUID;
  plan.summary = 'Read, edit, verify';
  plan.taskDescription = 'Add greet function to utils.ts';
  plan.steps = [
    makeStep({ stepNumber: 1, action: 'read' }),
    makeStep({ stepNumber: 2, action: 'edit', toolCall: 'code/edit', dependsOn: [1] }),
    makeStep({ stepNumber: 3, action: 'verify', dependsOn: [2] }),
  ];
  plan.estimatedToolCalls = 3;
  plan.assignees = ['11111111-2222-3333-4444-555555555555' as UUID];
  plan.generatedBy = { provider: 'anthropic', model: 'claude-sonnet', temperature: 0.3, durationMs: 500 };
  plan.status = 'draft';

  // Apply overrides
  if (overrides) {
    for (const [key, value] of Object.entries(overrides)) {
      (plan as Record<string, unknown>)[key] = value;
    }
  }

  return plan;
}

describe('CodingPlanEntity', () => {
  describe('construction and defaults', () => {
    it('creates with default values', () => {
      const plan = new CodingPlanEntity();

      expect(plan.taskId).toBe('');
      expect(plan.createdById).toBe('');
      expect(plan.leadId).toBe('');
      expect(plan.summary).toBe('');
      expect(plan.taskDescription).toBe('');
      expect(plan.steps).toEqual([]);
      expect(plan.estimatedToolCalls).toBe(0);
      expect(plan.assignees).toEqual([]);
      expect(plan.status).toBe('draft');
      expect(plan.filesModified).toEqual([]);
      expect(plan.filesCreated).toEqual([]);
      expect(plan.changeIds).toEqual([]);
      expect(plan.errors).toEqual([]);
      expect(plan.totalToolCalls).toBe(0);
      expect(plan.totalDurationMs).toBe(0);
    });

    it('has correct collection name', () => {
      const plan = new CodingPlanEntity();
      expect(plan.collection).toBe('coding_plans');
      expect(CodingPlanEntity.collection).toBe('coding_plans');
    });

    it('has pagination config with newest first', () => {
      const config = CodingPlanEntity.getPaginationConfig();
      expect(config.defaultSortField).toBe('createdAt');
      expect(config.defaultSortDirection).toBe('desc');
      expect(config.defaultPageSize).toBe(20);
    });
  });

  describe('validation', () => {
    it('validates a complete plan', () => {
      const plan = makePlan();
      const result = plan.validate();
      expect(result.success).toBe(true);
    });

    it('rejects missing taskId', () => {
      const plan = makePlan({ taskId: '' as UUID });
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('taskId');
    });

    it('rejects missing createdById', () => {
      const plan = makePlan({ createdById: '' as UUID });
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('createdById');
    });

    it('rejects missing leadId', () => {
      const plan = makePlan({ leadId: '' as UUID });
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('leadId');
    });

    it('rejects missing summary', () => {
      const plan = makePlan({ summary: '' });
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('summary');
    });

    it('rejects missing taskDescription', () => {
      const plan = makePlan({ taskDescription: '  ' });
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('taskDescription');
    });

    it('rejects empty steps array', () => {
      const plan = makePlan();
      plan.steps = [];
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('at least one step');
    });

    it('rejects empty assignees', () => {
      const plan = makePlan();
      plan.assignees = [];
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('at least one assignee');
    });

    it('rejects invalid status', () => {
      const plan = makePlan();
      plan.status = 'bogus' as CodingPlanStatus;
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('status');
    });

    it('validates all valid statuses', () => {
      const validStatuses: CodingPlanStatus[] = [
        'draft', 'proposed', 'approved', 'executing',
        'completed', 'partial', 'failed', 'cancelled',
      ];

      for (const status of validStatuses) {
        const plan = makePlan({ status });
        const result = plan.validate();
        expect(result.success).toBe(true);
      }
    });

    it('rejects step with invalid stepNumber', () => {
      const plan = makePlan();
      plan.steps = [makeStep({ stepNumber: 0 })];
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('stepNumber');
    });

    it('rejects step with missing action', () => {
      const plan = makePlan();
      plan.steps = [makeStep({ action: '' as any })];
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('action');
    });

    it('rejects step with non-code toolCall', () => {
      const plan = makePlan();
      plan.steps = [makeStep({ toolCall: 'data/list' })];
      const result = plan.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('toolCall');
    });
  });

  describe('computed properties', () => {
    it('reports progress correctly', () => {
      const plan = makePlan();
      expect(plan.progress).toBe(0); // All pending

      plan.steps[0].status = 'completed';
      expect(plan.progress).toBeCloseTo(1 / 3);

      plan.steps[1].status = 'completed';
      expect(plan.progress).toBeCloseTo(2 / 3);

      plan.steps[2].status = 'completed';
      expect(plan.progress).toBe(1);
    });

    it('counts completed steps', () => {
      const plan = makePlan();
      expect(plan.stepsCompleted).toBe(0);

      plan.steps[0].status = 'completed';
      plan.steps[1].status = 'failed';
      plan.steps[2].status = 'skipped';
      expect(plan.stepsCompleted).toBe(1);
    });

    it('counts failed steps', () => {
      const plan = makePlan();
      plan.steps[0].status = 'completed';
      plan.steps[1].status = 'failed';
      plan.steps[2].status = 'failed';
      expect(plan.stepsFailed).toBe(2);
    });

    it('counts remaining steps', () => {
      const plan = makePlan();
      expect(plan.stepsRemaining).toBe(3); // All pending

      plan.steps[0].status = 'completed';
      plan.steps[1].status = 'executing';
      expect(plan.stepsRemaining).toBe(2); // 1 pending + 1 executing
    });

    it('progress is 0 for empty steps', () => {
      const plan = new CodingPlanEntity();
      expect(plan.progress).toBe(0);
    });
  });

  describe('hierarchical structure', () => {
    it('top-level plan has no parent', () => {
      const plan = makePlan();
      expect(plan.parentPlanId).toBeUndefined();
      expect(plan.isDelegated).toBe(false);
    });

    it('sub-plan references parent', () => {
      const plan = makePlan();
      plan.parentPlanId = 'parent-plan-id-1234' as UUID;
      expect(plan.isDelegated).toBe(true);
    });

    it('sub-plan can have different lead than creator', () => {
      const plan = makePlan();
      plan.createdById = 'lead-ai' as UUID;
      plan.leadId = 'lead-ai' as UUID;
      plan.assignees = ['specialist-ai' as UUID];
      // Sub-plan created by lead, assigned to specialist
      expect(plan.assignees).not.toContain(plan.leadId);
    });
  });

  describe('execution tracking', () => {
    it('tracks file modifications', () => {
      const plan = makePlan({ status: 'completed' });
      plan.filesModified = ['src/utils.ts', 'src/index.ts'];
      plan.filesCreated = ['src/greet.ts'];
      plan.changeIds = ['change-001', 'change-002'];

      expect(plan.filesModified).toHaveLength(2);
      expect(plan.filesCreated).toContain('src/greet.ts');
      expect(plan.changeIds).toContain('change-001');
    });

    it('tracks errors', () => {
      const plan = makePlan({ status: 'partial' });
      plan.errors = ['Step 2 (edit): Conflict', 'Step 3 (verify): Dependencies not met'];
      expect(plan.errors).toHaveLength(2);
    });

    it('tracks execution timing', () => {
      const plan = makePlan({ status: 'completed' });
      plan.executionStartedAt = 1000;
      plan.executionCompletedAt = 5000;
      plan.totalDurationMs = 4000;
      plan.totalToolCalls = 5;

      expect(plan.executionStartedAt).toBe(1000);
      expect(plan.executionCompletedAt).toBe(5000);
      expect(plan.totalDurationMs).toBe(4000);
      expect(plan.totalToolCalls).toBe(5);
    });
  });

  describe('risk and security', () => {
    it('defaults riskLevel to low', () => {
      const plan = new CodingPlanEntity();
      expect(plan.riskLevel).toBe('low');
    });

    it('defaults securityTier to write', () => {
      const plan = new CodingPlanEntity();
      expect(plan.securityTier).toBe('write');
    });

    it('stores risk assessment data', () => {
      const plan = makePlan();
      plan.riskLevel = 'high';
      plan.riskReason = 'Modifies API interfaces';
      plan.securityTier = 'write';

      expect(plan.riskLevel).toBe('high');
      expect(plan.riskReason).toBe('Modifies API interfaces');
      expect(plan.securityTier).toBe('write');
    });

    it('critical risk with system tier', () => {
      const plan = makePlan();
      plan.riskLevel = 'critical';
      plan.securityTier = 'system';

      expect(plan.riskLevel).toBe('critical');
      expect(plan.securityTier).toBe('system');
    });
  });

  describe('governance', () => {
    it('tracks proposal reference', () => {
      const plan = makePlan({ status: 'proposed' });
      plan.proposalId = 'proposal-abc-123' as UUID;
      expect(plan.proposalId).toBe('proposal-abc-123');
    });

    it('plan without proposal has no proposalId', () => {
      const plan = makePlan();
      expect(plan.proposalId).toBeUndefined();
    });
  });
});
