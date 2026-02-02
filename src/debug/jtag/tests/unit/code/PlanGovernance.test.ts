/**
 * PlanGovernance Unit Tests
 *
 * Tests risk-based approval routing:
 * - shouldRequireApproval: risk level + multi-agent logic
 * - resolveDecision: governance outcome → plan status mapping
 * - proposePlan: governance proposal creation (integration tested separately)
 */

import { describe, it, expect } from 'vitest';
import { PlanGovernance, type GovernanceDecision, type GovernanceOutcome } from '../../../system/code/server/PlanGovernance';
import { CodingPlanEntity } from '../../../system/data/entities/CodingPlanEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { RiskLevel, SecurityTierLevel } from '../../../system/code/shared/CodingTypes';

// ── Helpers ──────────────────────────────────────────────────

const PERSONA_A = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID;
const PERSONA_B = 'bbbbbbbb-cccc-dddd-eeee-ffffffffffff' as UUID;
const TASK_ID = '11111111-2222-3333-4444-555555555555' as UUID;

function makePlan(overrides?: {
  riskLevel?: RiskLevel;
  securityTier?: SecurityTierLevel;
  assignees?: UUID[];
}): CodingPlanEntity {
  const plan = new CodingPlanEntity();
  plan.taskId = TASK_ID;
  plan.createdById = PERSONA_A;
  plan.leadId = PERSONA_A;
  plan.summary = 'Test plan';
  plan.taskDescription = 'Test task description';
  plan.assignees = overrides?.assignees ?? [PERSONA_A];
  plan.riskLevel = overrides?.riskLevel ?? 'low';
  plan.securityTier = overrides?.securityTier ?? 'write';
  plan.generatedBy = { provider: 'test', model: 'test-model', temperature: 0, durationMs: 0 };
  plan.steps = [{
    stepNumber: 1,
    action: 'read',
    description: 'Read main.ts',
    targetFiles: ['src/main.ts'],
    toolCall: 'code/read',
    toolParams: { filePath: 'src/main.ts' },
    dependsOn: [],
    verification: 'File content returned',
    status: 'pending',
  }];
  return plan;
}

function makeDecision(outcome: GovernanceOutcome): GovernanceDecision {
  return {
    proposalId: '99999999-8888-7777-6666-555555555555' as UUID,
    outcome,
    reasoning: `Decision: ${outcome}`,
  };
}

// ── Tests ────────────────────────────────────────────────────

describe('PlanGovernance', () => {
  const governance = new PlanGovernance();

  describe('shouldRequireApproval', () => {
    describe('single-agent plans', () => {
      it('low risk → no approval required', () => {
        const plan = makePlan({ riskLevel: 'low' });
        expect(governance.shouldRequireApproval(plan)).toBe(false);
      });

      it('medium risk → no approval required', () => {
        const plan = makePlan({ riskLevel: 'medium' });
        expect(governance.shouldRequireApproval(plan)).toBe(false);
      });

      it('high risk → approval required', () => {
        const plan = makePlan({ riskLevel: 'high' });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });

      it('critical risk → approval required', () => {
        const plan = makePlan({ riskLevel: 'critical' });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });
    });

    describe('multi-agent plans', () => {
      it('low risk + multi-agent → approval required', () => {
        const plan = makePlan({ riskLevel: 'low', assignees: [PERSONA_A, PERSONA_B] });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });

      it('medium risk + multi-agent → approval required', () => {
        const plan = makePlan({ riskLevel: 'medium', assignees: [PERSONA_A, PERSONA_B] });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });

      it('high risk + multi-agent → approval required', () => {
        const plan = makePlan({ riskLevel: 'high', assignees: [PERSONA_A, PERSONA_B] });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });
    });

    describe('system tier', () => {
      it('system tier always requires approval regardless of risk', () => {
        const plan = makePlan({ riskLevel: 'low', securityTier: 'system' });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });

      it('system tier + single agent still requires approval', () => {
        const plan = makePlan({ riskLevel: 'low', securityTier: 'system', assignees: [PERSONA_A] });
        expect(governance.shouldRequireApproval(plan)).toBe(true);
      });
    });
  });

  describe('resolveDecision', () => {
    it('approved → approved', () => {
      const result = governance.resolveDecision(makeDecision('approved'));
      expect(result).toBe('approved');
    });

    it('approved_with_changes → approved', () => {
      const result = governance.resolveDecision(makeDecision('approved_with_changes'));
      expect(result).toBe('approved');
    });

    it('changes_requested → draft', () => {
      const result = governance.resolveDecision(makeDecision('changes_requested'));
      expect(result).toBe('draft');
    });

    it('rejected → cancelled', () => {
      const result = governance.resolveDecision(makeDecision('rejected'));
      expect(result).toBe('cancelled');
    });
  });

  describe('all outcomes map to valid plan statuses', () => {
    const outcomes: GovernanceOutcome[] = ['approved', 'approved_with_changes', 'changes_requested', 'rejected'];
    const validStatuses = ['draft', 'proposed', 'approved', 'executing', 'completed', 'partial', 'failed', 'cancelled'];

    for (const outcome of outcomes) {
      it(`${outcome} maps to a valid CodingPlanStatus`, () => {
        const result = governance.resolveDecision(makeDecision(outcome));
        expect(validStatuses).toContain(result);
      });
    }
  });

  describe('approval matrix (exhaustive)', () => {
    const riskLevels: RiskLevel[] = ['low', 'medium', 'high', 'critical'];
    const tiers: SecurityTierLevel[] = ['discovery', 'read', 'write', 'system'];

    for (const risk of riskLevels) {
      for (const tier of tiers) {
        for (const multiAgent of [false, true]) {
          it(`risk=${risk}, tier=${tier}, multiAgent=${multiAgent}`, () => {
            const assignees = multiAgent ? [PERSONA_A, PERSONA_B] : [PERSONA_A];
            const plan = makePlan({ riskLevel: risk, securityTier: tier, assignees });
            const result = governance.shouldRequireApproval(plan);
            expect(typeof result).toBe('boolean');

            // Verify specific cases
            if (tier === 'system') expect(result).toBe(true);
            if (multiAgent) expect(result).toBe(true);
            if (risk === 'high' || risk === 'critical') expect(result).toBe(true);
            if (risk === 'low' && tier !== 'system' && !multiAgent) expect(result).toBe(false);
          });
        }
      }
    }
  });
});
