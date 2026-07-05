/**
 * Skill Lifecycle Tests
 *
 * Tests the skill lifecycle state machine:
 * - Personal skill: proposed → generated → validated → active
 * - Team skill: proposed → approved → generated → validated → active
 * - Failure paths at each stage
 * - Validation results tracking
 * - Scope and governance rules
 */

import { describe, it, expect } from 'vitest';
import {
  SkillEntity,
  type SkillSpec,
  type SkillStatus,
  type SkillValidationResults,
} from '../../../system/data/entities/SkillEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// ── Helpers ──────────────────────────────────────────────────

const PERSONA_ID = '11111111-2222-3333-4444-555555555555' as UUID;
const PROPOSAL_ID = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID;

function makeSpec(name = 'analysis/complexity'): SkillSpec {
  return {
    name,
    description: 'Analyzes code complexity',
    params: [{ name: 'filePath', type: 'string' }],
    results: [{ name: 'complexity', type: 'number' }],
    implementation: 'Count decision branches in AST',
    accessLevel: 'ai-safe',
  };
}

function makeSkillEntity(status: SkillStatus = 'proposed', scope: 'personal' | 'team' = 'personal'): SkillEntity {
  const entity = new SkillEntity();
  entity.name = 'analysis/complexity';
  entity.description = 'Analyzes code complexity';
  entity.createdById = PERSONA_ID;
  entity.spec = makeSpec();
  entity.scope = scope;
  entity.status = status;
  return entity;
}

// ── Tests ────────────────────────────────────────────────────

describe('Skill Lifecycle', () => {
  describe('personal skill: full lifecycle', () => {
    it('proposed → generated → validated → active', () => {
      const entity = makeSkillEntity('proposed', 'personal');

      // Step 1: proposed
      expect(entity.status).toBe('proposed');
      expect(entity.canAdvance).toBe(true);
      expect(entity.nextStatus).toBe('generated');
      expect(entity.requiresApproval).toBe(false);

      // Step 2: generate
      entity.status = 'generated';
      entity.outputDir = '/tmp/skills/analysis/complexity';
      entity.generatedFiles = ['ServerCommand.ts', 'Types.ts', 'BrowserCommand.ts'];
      expect(entity.canAdvance).toBe(true);
      expect(entity.nextStatus).toBe('validated');
      expect(entity.generatedFiles).toHaveLength(3);

      // Step 3: validate
      entity.status = 'validated';
      entity.validationResults = {
        compiled: true,
        testsRun: 3,
        testsPassed: 3,
        errors: [],
        durationMs: 500,
      };
      expect(entity.canAdvance).toBe(true);
      expect(entity.nextStatus).toBe('active');

      // Step 4: activate
      entity.status = 'active';
      entity.activatedAt = Date.now();
      expect(entity.isActive).toBe(true);
      expect(entity.canAdvance).toBe(false);
      expect(entity.nextStatus).toBeUndefined();

      // Entity still validates at every stage
      expect(entity.validate().success).toBe(true);
    });
  });

  describe('team skill: full lifecycle with governance', () => {
    it('proposed → approved → generated → validated → active', () => {
      const entity = makeSkillEntity('proposed', 'team');

      // Step 1: proposed — cannot advance without proposal
      expect(entity.requiresApproval).toBe(true);
      expect(entity.canAdvance).toBe(false);
      expect(entity.nextStatus).toBe('approved');

      // Set proposal ID → now can advance
      entity.proposalId = PROPOSAL_ID;
      expect(entity.canAdvance).toBe(true);

      // Step 2: approved
      entity.status = 'approved';
      expect(entity.canAdvance).toBe(true);
      expect(entity.nextStatus).toBe('generated');

      // Step 3: generated
      entity.status = 'generated';
      entity.outputDir = '/tmp/commands/analysis/complexity';
      entity.generatedFiles = ['ServerCommand.ts', 'Types.ts'];
      expect(entity.nextStatus).toBe('validated');

      // Step 4: validated
      entity.status = 'validated';
      entity.validationResults = {
        compiled: true,
        testsRun: 5,
        testsPassed: 5,
        errors: [],
        durationMs: 1200,
      };

      // Step 5: activated
      entity.status = 'active';
      entity.activatedAt = Date.now();
      expect(entity.isActive).toBe(true);
      expect(entity.validate().success).toBe(true);
    });
  });

  describe('failure paths', () => {
    it('failure at generation stage', () => {
      const entity = makeSkillEntity('proposed', 'personal');

      entity.status = 'failed';
      entity.failureReason = 'CommandGenerator error: invalid spec';

      expect(entity.canAdvance).toBe(false);
      expect(entity.nextStatus).toBeUndefined();
      expect(entity.isActive).toBe(false);
      expect(entity.failureReason).toContain('CommandGenerator');
      expect(entity.validate().success).toBe(true);
    });

    it('failure at validation — compilation error', () => {
      const entity = makeSkillEntity('generated');
      entity.outputDir = '/tmp/skills/test';
      entity.generatedFiles = ['ServerCommand.ts'];

      entity.status = 'failed';
      entity.failureReason = 'Compilation failed: TS2345 - Argument type mismatch';
      entity.validationResults = {
        compiled: false,
        testsRun: 0,
        testsPassed: 0,
        errors: ['Compilation failed: TS2345 - Argument type mismatch'],
        durationMs: 200,
      };

      expect(entity.canAdvance).toBe(false);
      expect(entity.validationResults.compiled).toBe(false);
      expect(entity.validationResults.errors).toHaveLength(1);
    });

    it('failure at validation — tests fail', () => {
      const entity = makeSkillEntity('generated');
      entity.outputDir = '/tmp/skills/test';
      entity.generatedFiles = ['ServerCommand.ts'];

      entity.status = 'failed';
      entity.validationResults = {
        compiled: true,
        testsRun: 10,
        testsPassed: 7,
        errors: [
          'Test "edge case" failed: expected 0, got -1',
          'Test "null input" failed: TypeError',
          'Test "large input" failed: timeout after 60000ms',
        ],
        durationMs: 60500,
      };
      entity.failureReason = entity.validationResults.errors.join('; ');

      expect(entity.validationResults.compiled).toBe(true);
      expect(entity.validationResults.testsPassed).toBe(7);
      expect(entity.validationResults.testsRun).toBe(10);
      expect(entity.validationResults.errors).toHaveLength(3);
    });

    it('failure at activation', () => {
      const entity = makeSkillEntity('validated');
      entity.outputDir = '/tmp/skills/test';
      entity.generatedFiles = ['ServerCommand.ts'];
      entity.validationResults = {
        compiled: true, testsRun: 1, testsPassed: 1, errors: [], durationMs: 100,
      };

      entity.status = 'failed';
      entity.failureReason = 'Activation failed: dynamic import error';

      expect(entity.canAdvance).toBe(false);
      expect(entity.isActive).toBe(false);
    });
  });

  describe('deprecation', () => {
    it('active skill can be deprecated', () => {
      const entity = makeSkillEntity('active');
      entity.activatedAt = Date.now() - 86400000; // 1 day ago

      expect(entity.isActive).toBe(true);

      entity.status = 'deprecated';
      expect(entity.isActive).toBe(false);
      expect(entity.canAdvance).toBe(false);
      expect(entity.nextStatus).toBeUndefined();
      expect(entity.validate().success).toBe(true);
    });
  });

  describe('validation results tracking', () => {
    it('tracks successful validation with full metrics', () => {
      const results: SkillValidationResults = {
        compiled: true,
        testsRun: 10,
        testsPassed: 10,
        errors: [],
        durationMs: 2500,
      };

      const entity = makeSkillEntity('generated');
      entity.validationResults = results;
      entity.status = 'validated';

      expect(entity.validationResults.compiled).toBe(true);
      expect(entity.validationResults.testsRun).toBe(10);
      expect(entity.validationResults.testsPassed).toBe(10);
      expect(entity.validationResults.errors).toHaveLength(0);
      expect(entity.validationResults.durationMs).toBe(2500);
    });
  });

  describe('scope and governance rules', () => {
    it('personal skill does not require approval', () => {
      const entity = makeSkillEntity('proposed', 'personal');
      expect(entity.requiresApproval).toBe(false);
      expect(entity.canAdvance).toBe(true);
    });

    it('team skill requires approval and governance', () => {
      const entity = makeSkillEntity('proposed', 'team');
      expect(entity.requiresApproval).toBe(true);
      expect(entity.canAdvance).toBe(false); // No proposal yet

      entity.proposalId = PROPOSAL_ID;
      expect(entity.canAdvance).toBe(true);
    });

    it('team skills go through approved state', () => {
      const entity = makeSkillEntity('proposed', 'team');
      expect(entity.nextStatus).toBe('approved');
    });

    it('personal skills skip approved state', () => {
      const entity = makeSkillEntity('proposed', 'personal');
      expect(entity.nextStatus).toBe('generated');
    });
  });

  describe('entity validation consistency across all stages', () => {
    it('all lifecycle stages produce valid entities', () => {
      const stages: Array<{ status: SkillStatus; extras?: Record<string, unknown> }> = [
        { status: 'proposed' },
        { status: 'approved' },
        { status: 'generated', extras: { outputDir: '/tmp/out', generatedFiles: ['a.ts'] } },
        { status: 'validated', extras: {
          outputDir: '/tmp/out',
          generatedFiles: ['a.ts'],
          validationResults: { compiled: true, testsRun: 1, testsPassed: 1, errors: [], durationMs: 100 },
        }},
        { status: 'active', extras: {
          outputDir: '/tmp/out',
          generatedFiles: ['a.ts'],
          activatedAt: Date.now(),
        }},
        { status: 'failed', extras: { failureReason: 'Something went wrong' } },
        { status: 'deprecated' },
      ];

      for (const { status, extras } of stages) {
        const entity = makeSkillEntity(status);
        if (extras) {
          for (const [key, value] of Object.entries(extras)) {
            (entity as Record<string, unknown>)[key] = value;
          }
        }
        const result = entity.validate();
        expect(result.success).toBe(true);
      }
    });
  });

  describe('multiple skills with different names', () => {
    it('supports various command naming patterns', () => {
      const names = [
        'lint',
        'code/lint',
        'analysis/complexity',
        'code/analysis/deep-scan',
        'my-tool',
      ];

      for (const name of names) {
        const entity = new SkillEntity();
        entity.name = name;
        entity.description = `A skill called ${name}`;
        entity.createdById = PERSONA_ID;
        entity.spec = makeSpec(name);
        entity.scope = 'personal';
        entity.status = 'proposed';

        const result = entity.validate();
        expect(result.success).toBe(true);
      }
    });
  });
});
