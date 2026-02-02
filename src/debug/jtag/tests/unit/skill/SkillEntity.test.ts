/**
 * SkillEntity Unit Tests
 *
 * Tests the self-modifying skill entity:
 * - Construction and default values
 * - Validation (required fields, naming convention, spec consistency)
 * - Status lifecycle transitions
 * - Computed properties (isActive, requiresApproval, canAdvance, nextStatus)
 * - Collection and pagination config
 */

import { describe, it, expect } from 'vitest';
import {
  SkillEntity,
  type SkillSpec,
  type SkillStatus,
  type SkillScope,
  type SkillParamSpec,
  type SkillResultSpec,
  type SkillValidationResults,
} from '../../../system/data/entities/SkillEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { COLLECTIONS } from '../../../system/shared/Constants';

function makeSpec(overrides?: Partial<SkillSpec>): SkillSpec {
  return {
    name: 'analysis/complexity',
    description: 'Analyzes code complexity metrics',
    params: [
      { name: 'filePath', type: 'string', description: 'Path to analyze' },
    ],
    results: [
      { name: 'complexity', type: 'number', description: 'Cyclomatic complexity score' },
      { name: 'message', type: 'string', description: 'Human-readable summary' },
    ],
    implementation: 'Parse the file AST and count decision branches for cyclomatic complexity.',
    accessLevel: 'ai-safe',
    ...overrides,
  };
}

function makeSkill(overrides?: Partial<SkillEntity>): SkillEntity {
  const entity = new SkillEntity();
  entity.name = 'analysis/complexity';
  entity.description = 'Analyzes code complexity metrics';
  entity.createdById = '11111111-2222-3333-4444-555555555555' as UUID;
  entity.spec = makeSpec();
  entity.scope = 'personal';
  entity.status = 'proposed';

  if (overrides) {
    for (const [key, value] of Object.entries(overrides)) {
      (entity as Record<string, unknown>)[key] = value;
    }
  }

  return entity;
}

describe('SkillEntity', () => {
  describe('construction and defaults', () => {
    it('creates with default values', () => {
      const skill = new SkillEntity();

      expect(skill.name).toBe('');
      expect(skill.description).toBe('');
      expect(skill.createdById).toBe('');
      expect(skill.scope).toBe('personal');
      expect(skill.status).toBe('proposed');
      expect(skill.generatedFiles).toEqual([]);
      expect(skill.proposalId).toBeUndefined();
      expect(skill.outputDir).toBeUndefined();
      expect(skill.validationResults).toBeUndefined();
      expect(skill.activatedAt).toBeUndefined();
      expect(skill.failureReason).toBeUndefined();
    });

    it('has default spec with empty fields', () => {
      const skill = new SkillEntity();

      expect(skill.spec.name).toBe('');
      expect(skill.spec.description).toBe('');
      expect(skill.spec.params).toEqual([]);
      expect(skill.spec.results).toEqual([]);
      expect(skill.spec.implementation).toBe('');
    });
  });

  describe('collection and pagination', () => {
    it('has correct static collection', () => {
      expect(SkillEntity.collection).toBe(COLLECTIONS.SKILLS);
    });

    it('has correct instance collection', () => {
      const skill = new SkillEntity();
      expect(skill.collection).toBe(COLLECTIONS.SKILLS);
    });

    it('returns pagination config', () => {
      const config = SkillEntity.getPaginationConfig();
      expect(config.defaultSortField).toBe('createdAt');
      expect(config.defaultSortDirection).toBe('desc');
      expect(config.defaultPageSize).toBe(20);
      expect(config.cursorField).toBe('createdAt');
    });
  });

  describe('validation', () => {
    it('validates a well-formed personal skill', () => {
      const skill = makeSkill();
      const result = skill.validate();
      expect(result.success).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it('validates a well-formed team skill', () => {
      const skill = makeSkill({ scope: 'team' });
      const result = skill.validate();
      expect(result.success).toBe(true);
    });

    it('rejects missing name', () => {
      const skill = makeSkill({ name: '' });
      skill.spec = makeSpec({ name: '' });
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('name');
    });

    it('rejects invalid naming convention', () => {
      const skill = makeSkill({ name: 'InvalidName' });
      skill.spec = makeSpec({ name: 'InvalidName' });
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('naming convention');
    });

    it('accepts simple names without slashes', () => {
      const skill = makeSkill({ name: 'lint' });
      skill.spec = makeSpec({ name: 'lint' });
      const result = skill.validate();
      expect(result.success).toBe(true);
    });

    it('accepts multi-level names', () => {
      const skill = makeSkill({ name: 'code/analysis/deep' });
      skill.spec = makeSpec({ name: 'code/analysis/deep' });
      const result = skill.validate();
      expect(result.success).toBe(true);
    });

    it('rejects missing description', () => {
      const skill = makeSkill({ description: '' });
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('description');
    });

    it('rejects missing createdById', () => {
      const skill = makeSkill({ createdById: '' as UUID });
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('createdById');
    });

    it('rejects mismatched spec.name and entity name', () => {
      const skill = makeSkill();
      skill.spec = makeSpec({ name: 'different/name' });
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('must match');
    });

    it('rejects missing implementation in spec', () => {
      const skill = makeSkill();
      skill.spec = makeSpec({ implementation: '' });
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('implementation');
    });

    it('rejects invalid scope', () => {
      const skill = makeSkill();
      (skill as Record<string, unknown>).scope = 'invalid';
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('scope');
    });

    it('rejects invalid status', () => {
      const skill = makeSkill();
      (skill as Record<string, unknown>).status = 'invalid';
      const result = skill.validate();
      expect(result.success).toBe(false);
      expect(result.error).toContain('status');
    });

    it('validates all valid statuses', () => {
      const statuses: SkillStatus[] = [
        'proposed', 'approved', 'generated', 'validated', 'active', 'failed', 'deprecated',
      ];
      for (const status of statuses) {
        const skill = makeSkill({ status });
        const result = skill.validate();
        expect(result.success).toBe(true);
      }
    });

    it('validates all valid scopes', () => {
      const scopes: SkillScope[] = ['personal', 'team'];
      for (const scope of scopes) {
        const skill = makeSkill({ scope });
        const result = skill.validate();
        expect(result.success).toBe(true);
      }
    });
  });

  describe('computed properties', () => {
    it('isActive returns true for active skills', () => {
      const skill = makeSkill({ status: 'active' });
      expect(skill.isActive).toBe(true);
    });

    it('isActive returns false for non-active skills', () => {
      const statuses: SkillStatus[] = ['proposed', 'approved', 'generated', 'validated', 'failed', 'deprecated'];
      for (const status of statuses) {
        const skill = makeSkill({ status });
        expect(skill.isActive).toBe(false);
      }
    });

    it('requiresApproval returns true for team scope', () => {
      const skill = makeSkill({ scope: 'team' });
      expect(skill.requiresApproval).toBe(true);
    });

    it('requiresApproval returns false for personal scope', () => {
      const skill = makeSkill({ scope: 'personal' });
      expect(skill.requiresApproval).toBe(false);
    });

    describe('canAdvance', () => {
      it('personal proposed can advance', () => {
        const skill = makeSkill({ status: 'proposed', scope: 'personal' });
        expect(skill.canAdvance).toBe(true);
      });

      it('team proposed without proposal cannot advance', () => {
        const skill = makeSkill({ status: 'proposed', scope: 'team' });
        expect(skill.canAdvance).toBe(false);
      });

      it('team proposed with proposal can advance', () => {
        const skill = makeSkill({
          status: 'proposed',
          scope: 'team',
          proposalId: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID,
        });
        expect(skill.canAdvance).toBe(true);
      });

      it('approved can advance', () => {
        const skill = makeSkill({ status: 'approved' });
        expect(skill.canAdvance).toBe(true);
      });

      it('generated can advance', () => {
        const skill = makeSkill({ status: 'generated' });
        expect(skill.canAdvance).toBe(true);
      });

      it('validated can advance', () => {
        const skill = makeSkill({ status: 'validated' });
        expect(skill.canAdvance).toBe(true);
      });

      it('active cannot advance', () => {
        const skill = makeSkill({ status: 'active' });
        expect(skill.canAdvance).toBe(false);
      });

      it('failed cannot advance', () => {
        const skill = makeSkill({ status: 'failed' });
        expect(skill.canAdvance).toBe(false);
      });

      it('deprecated cannot advance', () => {
        const skill = makeSkill({ status: 'deprecated' });
        expect(skill.canAdvance).toBe(false);
      });
    });

    describe('nextStatus', () => {
      it('personal proposed → generated', () => {
        const skill = makeSkill({ status: 'proposed', scope: 'personal' });
        expect(skill.nextStatus).toBe('generated');
      });

      it('team proposed → approved', () => {
        const skill = makeSkill({ status: 'proposed', scope: 'team' });
        expect(skill.nextStatus).toBe('approved');
      });

      it('approved → generated', () => {
        const skill = makeSkill({ status: 'approved' });
        expect(skill.nextStatus).toBe('generated');
      });

      it('generated → validated', () => {
        const skill = makeSkill({ status: 'generated' });
        expect(skill.nextStatus).toBe('validated');
      });

      it('validated → active', () => {
        const skill = makeSkill({ status: 'validated' });
        expect(skill.nextStatus).toBe('active');
      });

      it('active has no next status', () => {
        const skill = makeSkill({ status: 'active' });
        expect(skill.nextStatus).toBeUndefined();
      });

      it('failed has no next status', () => {
        const skill = makeSkill({ status: 'failed' });
        expect(skill.nextStatus).toBeUndefined();
      });

      it('deprecated has no next status', () => {
        const skill = makeSkill({ status: 'deprecated' });
        expect(skill.nextStatus).toBeUndefined();
      });
    });
  });

  describe('spec types', () => {
    it('supports param specs with optional fields', () => {
      const params: SkillParamSpec[] = [
        { name: 'required', type: 'string' },
        { name: 'optional', type: 'number', optional: true, description: 'An optional param' },
      ];
      const skill = makeSkill();
      skill.spec = makeSpec({ params });
      const result = skill.validate();
      expect(result.success).toBe(true);
      expect(skill.spec.params).toHaveLength(2);
      expect(skill.spec.params[1].optional).toBe(true);
    });

    it('supports result specs', () => {
      const results: SkillResultSpec[] = [
        { name: 'output', type: 'string', description: 'The output' },
      ];
      const skill = makeSkill();
      skill.spec = makeSpec({ results });
      const result = skill.validate();
      expect(result.success).toBe(true);
      expect(skill.spec.results).toHaveLength(1);
    });

    it('supports examples in spec', () => {
      const skill = makeSkill();
      skill.spec = makeSpec({
        examples: [
          {
            description: 'Analyze a simple file',
            command: 'skill/execute --name=analysis/complexity --filePath=utils.ts',
            expectedResult: 'Complexity: 3',
          },
        ],
      });
      const result = skill.validate();
      expect(result.success).toBe(true);
      expect(skill.spec.examples).toHaveLength(1);
    });

    it('supports different access levels', () => {
      for (const level of ['ai-safe', 'internal', 'system'] as const) {
        const skill = makeSkill();
        skill.spec = makeSpec({ accessLevel: level });
        const result = skill.validate();
        expect(result.success).toBe(true);
      }
    });
  });

  describe('validation results', () => {
    it('stores validation results', () => {
      const validation: SkillValidationResults = {
        compiled: true,
        testsRun: 5,
        testsPassed: 4,
        errors: ['Test 3 failed: expected 42 got 41'],
        durationMs: 1200,
      };
      const skill = makeSkill({ validationResults: validation });
      expect(skill.validationResults).toEqual(validation);
      expect(skill.validationResults!.compiled).toBe(true);
      expect(skill.validationResults!.testsRun).toBe(5);
      expect(skill.validationResults!.testsPassed).toBe(4);
      expect(skill.validationResults!.errors).toHaveLength(1);
    });
  });

  describe('lifecycle tracking fields', () => {
    it('tracks generated files', () => {
      const files = ['/path/to/ServerCommand.ts', '/path/to/Types.ts'];
      const skill = makeSkill({ generatedFiles: files });
      expect(skill.generatedFiles).toEqual(files);
    });

    it('tracks output directory', () => {
      const skill = makeSkill({ outputDir: '/tmp/generated/analysis/complexity' });
      expect(skill.outputDir).toBe('/tmp/generated/analysis/complexity');
    });

    it('tracks activation timestamp', () => {
      const now = Date.now();
      const skill = makeSkill({ activatedAt: now });
      expect(skill.activatedAt).toBe(now);
    });

    it('tracks failure reason', () => {
      const skill = makeSkill({
        status: 'failed',
        failureReason: 'Compilation error: missing import',
      });
      expect(skill.failureReason).toBe('Compilation error: missing import');
    });

    it('tracks proposal ID for team skills', () => {
      const proposalId = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID;
      const skill = makeSkill({ scope: 'team', proposalId });
      expect(skill.proposalId).toBe(proposalId);
    });
  });
});
