/**
 * CodeTaskDelegator Unit Tests
 *
 * Tests plan decomposition and multi-agent assignment:
 * - decompose: step DAG → file clusters (union-find)
 * - assign: clusters → agents (load-balanced)
 * - createSubPlans: assignments → CodingPlanEntity sub-plans
 * - consolidate: sub-plan results → parent CodingResult
 */

import { describe, it, expect } from 'vitest';
import { CodeTaskDelegator, type FileCluster, type AgentAssignment } from '../../../system/code/server/CodeTaskDelegator';
import { CodingPlanEntity, type CodingStepSnapshot } from '../../../system/data/entities/CodingPlanEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { AgentCapability } from '../../../system/code/shared/CodingTypes';

// ── Helpers ──────────────────────────────────────────────────

const TASK_ID = '11111111-2222-3333-4444-555555555555' as UUID;
const LEAD_ID = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID;
const AGENT_A = 'aaaaaaaa-1111-2222-3333-444444444444' as UUID;
const AGENT_B = 'bbbbbbbb-1111-2222-3333-444444444444' as UUID;
const AGENT_C = 'cccccccc-1111-2222-3333-444444444444' as UUID;

function makeStep(
  stepNumber: number,
  targetFiles: string[],
  dependsOn: number[] = [],
  action: string = 'edit',
): CodingStepSnapshot {
  return {
    stepNumber,
    action: action as any,
    description: `Step ${stepNumber}: ${action} ${targetFiles.join(', ')}`,
    targetFiles,
    toolCall: `code/${action}`,
    toolParams: {},
    dependsOn,
    verification: 'Verify step',
    status: 'pending',
  };
}

function makePlan(steps: CodingStepSnapshot[]): CodingPlanEntity {
  const plan = new CodingPlanEntity();
  plan.taskId = TASK_ID;
  plan.createdById = LEAD_ID;
  plan.leadId = LEAD_ID;
  plan.summary = 'Test plan for delegation';
  plan.taskDescription = 'Multi-file refactoring task';
  plan.steps = steps;
  plan.estimatedToolCalls = steps.length;
  plan.assignees = [LEAD_ID];
  plan.generatedBy = { provider: 'test', model: 'test-model', temperature: 0, durationMs: 0 };
  plan.riskLevel = 'medium';
  plan.securityTier = 'write';
  plan.status = 'approved';
  return plan;
}

function makeAgent(id: UUID, name: string, load: number = 0): AgentCapability {
  return {
    personaId: id,
    name,
    specialties: ['typescript'],
    currentLoad: load,
    securityTier: 'write',
  };
}

// ── Tests ────────────────────────────────────────────────────

describe('CodeTaskDelegator', () => {
  const delegator = new CodeTaskDelegator();

  describe('decompose', () => {
    it('empty plan produces no clusters', () => {
      const plan = makePlan([]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(0);
    });

    it('single step produces one cluster', () => {
      const plan = makePlan([
        makeStep(1, ['src/main.ts']),
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(1);
      expect(clusters[0].stepNumbers).toEqual([1]);
      expect(clusters[0].files).toEqual(['src/main.ts']);
    });

    it('independent files produce separate clusters', () => {
      const plan = makePlan([
        makeStep(1, ['src/moduleA.ts']),
        makeStep(2, ['src/moduleB.ts']),
        makeStep(3, ['src/moduleC.ts']),
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(3);

      const allFiles = clusters.flatMap(c => c.files);
      expect(allFiles).toContain('src/moduleA.ts');
      expect(allFiles).toContain('src/moduleB.ts');
      expect(allFiles).toContain('src/moduleC.ts');
    });

    it('shared file merges steps into one cluster', () => {
      const plan = makePlan([
        makeStep(1, ['src/shared.ts', 'src/a.ts']),
        makeStep(2, ['src/shared.ts', 'src/b.ts']),
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(1);
      expect(clusters[0].stepNumbers).toContain(1);
      expect(clusters[0].stepNumbers).toContain(2);
      expect(clusters[0].files).toContain('src/shared.ts');
      expect(clusters[0].files).toContain('src/a.ts');
      expect(clusters[0].files).toContain('src/b.ts');
    });

    it('dependencies merge steps into one cluster', () => {
      const plan = makePlan([
        makeStep(1, ['src/a.ts']),
        makeStep(2, ['src/b.ts'], [1]), // depends on step 1
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(1);
      expect(clusters[0].stepNumbers).toContain(1);
      expect(clusters[0].stepNumbers).toContain(2);
    });

    it('transitive file sharing merges all into one cluster', () => {
      // A shares file with B, B shares file with C → all in one cluster
      const plan = makePlan([
        makeStep(1, ['src/a.ts', 'src/shared-ab.ts']),
        makeStep(2, ['src/b.ts', 'src/shared-ab.ts', 'src/shared-bc.ts']),
        makeStep(3, ['src/c.ts', 'src/shared-bc.ts']),
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(1);
    });

    it('mixed independent and dependent steps', () => {
      const plan = makePlan([
        // Cluster 1: steps 1, 2 share moduleA.ts
        makeStep(1, ['src/moduleA.ts'], []),
        makeStep(2, ['src/moduleA.ts'], [1]),
        // Cluster 2: step 3 is independent
        makeStep(3, ['src/moduleB.ts'], []),
        // Cluster 3: steps 4, 5 share moduleC.ts
        makeStep(4, ['src/moduleC.ts'], []),
        makeStep(5, ['src/moduleC.ts', 'src/moduleC-test.ts'], [4]),
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(3);
    });

    it('external dependencies are tracked', () => {
      // Step 2 depends on step 1, but they touch different files
      // If we force them into different clusters (no shared files, no deps),
      // they'd be separate. But dependsOn forces merge.
      // Test external deps by having step 3 depend on step 1 from a different cluster
      const plan = makePlan([
        makeStep(1, ['src/a.ts']),
        makeStep(2, ['src/a.ts'], [1]), // Same cluster as 1
        makeStep(3, ['src/b.ts']),       // Different cluster
      ]);
      const clusters = delegator.decompose(plan);
      // Steps 1 and 2 in one cluster (shared file + dependency)
      // Step 3 in separate cluster (no shared files, no deps)
      expect(clusters).toHaveLength(2);

      const clusterB = clusters.find(c => c.files.includes('src/b.ts'));
      expect(clusterB).toBeDefined();
      expect(clusterB!.externalDeps).toEqual([]); // No external deps
    });

    it('steps are sorted within clusters', () => {
      const plan = makePlan([
        makeStep(3, ['src/shared.ts']),
        makeStep(1, ['src/shared.ts']),
        makeStep(2, ['src/shared.ts']),
      ]);
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(1);
      expect(clusters[0].stepNumbers).toEqual([1, 2, 3]);
    });
  });

  describe('assign', () => {
    it('empty clusters produces empty assignments', () => {
      const agents = [makeAgent(AGENT_A, 'Agent A')];
      const assignments = delegator.assign([], agents, makePlan([]));
      expect(assignments).toHaveLength(0);
    });

    it('empty agents produces empty assignments', () => {
      const clusters: FileCluster[] = [{
        index: 0, stepNumbers: [1], files: ['a.ts'], externalDeps: [],
      }];
      const assignments = delegator.assign(clusters, [], makePlan([]));
      expect(assignments).toHaveLength(0);
    });

    it('single cluster assigned to single agent', () => {
      const clusters: FileCluster[] = [{
        index: 0, stepNumbers: [1, 2], files: ['src/main.ts'], externalDeps: [],
      }];
      const agents = [makeAgent(AGENT_A, 'Agent A')];
      const assignments = delegator.assign(clusters, agents, makePlan([]));

      expect(assignments).toHaveLength(1);
      expect(assignments[0].agentId).toBe(AGENT_A);
      expect(assignments[0].totalSteps).toBe(2);
      expect(assignments[0].files).toContain('src/main.ts');
    });

    it('distributes clusters across agents evenly', () => {
      const clusters: FileCluster[] = [
        { index: 0, stepNumbers: [1], files: ['a.ts'], externalDeps: [] },
        { index: 1, stepNumbers: [2], files: ['b.ts'], externalDeps: [] },
        { index: 2, stepNumbers: [3], files: ['c.ts'], externalDeps: [] },
      ];
      const agents = [
        makeAgent(AGENT_A, 'Agent A', 0.1),
        makeAgent(AGENT_B, 'Agent B', 0.2),
        makeAgent(AGENT_C, 'Agent C', 0.3),
      ];
      const assignments = delegator.assign(clusters, agents, makePlan([]));

      expect(assignments).toHaveLength(3);
      // Each agent gets one cluster (evenly distributed)
      for (const a of assignments) {
        expect(a.totalSteps).toBe(1);
      }
    });

    it('prefers least-loaded agents', () => {
      const clusters: FileCluster[] = [
        { index: 0, stepNumbers: [1, 2, 3], files: ['big.ts'], externalDeps: [] },
      ];
      const agents = [
        makeAgent(AGENT_A, 'Agent A', 0.8), // Heavily loaded
        makeAgent(AGENT_B, 'Agent B', 0.1), // Least loaded
      ];
      const assignments = delegator.assign(clusters, agents, makePlan([]));

      expect(assignments).toHaveLength(1);
      expect(assignments[0].agentId).toBe(AGENT_B); // Least loaded gets it
    });

    it('handles more clusters than agents', () => {
      const clusters: FileCluster[] = [
        { index: 0, stepNumbers: [1], files: ['a.ts'], externalDeps: [] },
        { index: 1, stepNumbers: [2], files: ['b.ts'], externalDeps: [] },
        { index: 2, stepNumbers: [3], files: ['c.ts'], externalDeps: [] },
        { index: 3, stepNumbers: [4], files: ['d.ts'], externalDeps: [] },
      ];
      const agents = [
        makeAgent(AGENT_A, 'Agent A'),
        makeAgent(AGENT_B, 'Agent B'),
      ];
      const assignments = delegator.assign(clusters, agents, makePlan([]));

      // 4 clusters, 2 agents → each gets 2
      expect(assignments).toHaveLength(2);
      const totalSteps = assignments.reduce((sum, a) => sum + a.totalSteps, 0);
      expect(totalSteps).toBe(4);
    });
  });

  describe('createSubPlans', () => {
    it('creates sub-plans from assignments', () => {
      const plan = makePlan([
        makeStep(1, ['src/a.ts']),
        makeStep(2, ['src/b.ts']),
      ]);

      const assignments: AgentAssignment[] = [
        {
          agentId: AGENT_A,
          agentName: 'Agent A',
          clusters: [{ index: 0, stepNumbers: [1], files: ['src/a.ts'], externalDeps: [] }],
          totalSteps: 1,
          files: ['src/a.ts'],
        },
        {
          agentId: AGENT_B,
          agentName: 'Agent B',
          clusters: [{ index: 1, stepNumbers: [2], files: ['src/b.ts'], externalDeps: [] }],
          totalSteps: 1,
          files: ['src/b.ts'],
        },
      ];

      const subPlans = delegator.createSubPlans(plan, assignments);
      expect(subPlans).toHaveLength(2);

      // Sub-plan for Agent A
      const subA = subPlans.find(s => s.leadId === AGENT_A);
      expect(subA).toBeDefined();
      expect(subA!.steps).toHaveLength(1);
      expect(subA!.steps[0].stepNumber).toBe(1);
      expect(subA!.assignees).toEqual([AGENT_A]);
      expect(subA!.status).toBe('approved');

      // Sub-plan for Agent B
      const subB = subPlans.find(s => s.leadId === AGENT_B);
      expect(subB).toBeDefined();
      expect(subB!.steps).toHaveLength(1);
      expect(subB!.steps[0].stepNumber).toBe(2);
    });

    it('sub-plans inherit parent metadata', () => {
      const plan = makePlan([makeStep(1, ['src/a.ts'])]);
      plan.riskLevel = 'high';
      plan.securityTier = 'write';

      const assignments: AgentAssignment[] = [{
        agentId: AGENT_A, agentName: 'Agent A',
        clusters: [{ index: 0, stepNumbers: [1], files: ['src/a.ts'], externalDeps: [] }],
        totalSteps: 1, files: ['src/a.ts'],
      }];

      const subPlans = delegator.createSubPlans(plan, assignments);
      expect(subPlans[0].taskId).toBe(plan.taskId);
      expect(subPlans[0].riskLevel).toBe('high');
      expect(subPlans[0].securityTier).toBe('write');
      expect(subPlans[0].taskDescription).toBe(plan.taskDescription);
    });

    it('sub-plans filter dependsOn to only internal steps', () => {
      const plan = makePlan([
        makeStep(1, ['src/a.ts']),
        makeStep(2, ['src/a.ts'], [1]),  // Depends on step 1
        makeStep(3, ['src/b.ts'], [1]),   // Depends on step 1 (external dep)
      ]);

      // Steps 1 and 2 go to Agent A (shared file), step 3 to Agent B
      const assignments: AgentAssignment[] = [
        {
          agentId: AGENT_A, agentName: 'Agent A',
          clusters: [{ index: 0, stepNumbers: [1, 2], files: ['src/a.ts'], externalDeps: [] }],
          totalSteps: 2, files: ['src/a.ts'],
        },
        {
          agentId: AGENT_B, agentName: 'Agent B',
          clusters: [{ index: 1, stepNumbers: [3], files: ['src/b.ts'], externalDeps: [1] }],
          totalSteps: 1, files: ['src/b.ts'],
        },
      ];

      const subPlans = delegator.createSubPlans(plan, assignments);
      const subB = subPlans.find(s => s.leadId === AGENT_B)!;

      // Step 3's dependency on step 1 should be filtered out (step 1 is not in this sub-plan)
      expect(subB.steps[0].dependsOn).toEqual([]);
    });
  });

  describe('consolidate', () => {
    it('all completed → completed', () => {
      const plan = makePlan([]);
      const sub1 = makePlan([makeStep(1, ['a.ts'])]);
      sub1.status = 'completed';
      sub1.filesModified = ['a.ts'];
      sub1.totalToolCalls = 3;
      sub1.totalDurationMs = 1000;
      sub1.steps[0].status = 'completed';

      const sub2 = makePlan([makeStep(2, ['b.ts'])]);
      sub2.status = 'completed';
      sub2.filesModified = ['b.ts'];
      sub2.totalToolCalls = 2;
      sub2.totalDurationMs = 800;
      sub2.steps[0].status = 'completed';

      const result = delegator.consolidate(plan, [sub1, sub2]);
      expect(result.status).toBe('completed');
      expect(result.filesModified).toContain('a.ts');
      expect(result.filesModified).toContain('b.ts');
      expect(result.totalToolCalls).toBe(5);
      // Duration is max (parallel), not sum
      expect(result.totalDurationMs).toBe(1000);
    });

    it('some completed → partial', () => {
      const plan = makePlan([]);
      const sub1 = makePlan([makeStep(1, ['a.ts'])]);
      sub1.status = 'completed';
      sub1.steps[0].status = 'completed';

      const sub2 = makePlan([makeStep(2, ['b.ts'])]);
      sub2.status = 'failed';
      sub2.errors = ['Compilation failed'];
      sub2.steps[0].status = 'failed';

      const result = delegator.consolidate(plan, [sub1, sub2]);
      expect(result.status).toBe('partial');
      expect(result.errors).toContain('Compilation failed');
    });

    it('all failed → failed', () => {
      const plan = makePlan([]);
      const sub1 = makePlan([makeStep(1, ['a.ts'])]);
      sub1.status = 'failed';
      sub1.steps[0].status = 'failed';

      const sub2 = makePlan([makeStep(2, ['b.ts'])]);
      sub2.status = 'failed';
      sub2.steps[0].status = 'failed';

      const result = delegator.consolidate(plan, [sub1, sub2]);
      expect(result.status).toBe('failed');
    });

    it('detects file conflicts across sub-plans', () => {
      const plan = makePlan([]);
      const sub1 = makePlan([makeStep(1, ['shared.ts'])]);
      sub1.status = 'completed';
      sub1.filesModified = ['shared.ts'];
      sub1.steps[0].status = 'completed';

      const sub2 = makePlan([makeStep(2, ['shared.ts'])]);
      sub2.status = 'completed';
      sub2.filesModified = ['shared.ts'];
      sub2.steps[0].status = 'completed';

      const result = delegator.consolidate(plan, [sub1, sub2]);
      expect(result.errors.some(e => e.includes('conflict'))).toBe(true);
      expect(result.errors.some(e => e.includes('shared.ts'))).toBe(true);
    });

    it('aggregates change IDs from all sub-plans', () => {
      const plan = makePlan([]);
      const sub1 = makePlan([makeStep(1, ['a.ts'])]);
      sub1.status = 'completed';
      sub1.changeIds = ['change-1', 'change-2'];
      sub1.steps[0].status = 'completed';

      const sub2 = makePlan([makeStep(2, ['b.ts'])]);
      sub2.status = 'completed';
      sub2.changeIds = ['change-3'];
      sub2.steps[0].status = 'completed';

      const result = delegator.consolidate(plan, [sub1, sub2]);
      expect(result.changeIds).toEqual(['change-1', 'change-2', 'change-3']);
    });

    it('deduplicates modified files', () => {
      const plan = makePlan([]);
      const sub1 = makePlan([makeStep(1, ['shared.ts'])]);
      sub1.status = 'completed';
      sub1.filesModified = ['shared.ts'];
      sub1.steps[0].status = 'completed';

      const sub2 = makePlan([makeStep(2, ['shared.ts'])]);
      sub2.status = 'completed';
      sub2.filesModified = ['shared.ts'];
      sub2.steps[0].status = 'completed';

      const result = delegator.consolidate(plan, [sub1, sub2]);
      // Set-based dedup: shared.ts appears once
      expect(result.filesModified.filter(f => f === 'shared.ts')).toHaveLength(1);
    });

    it('empty sub-plans → failed', () => {
      const plan = makePlan([]);
      const result = delegator.consolidate(plan, []);
      expect(result.status).toBe('failed');
    });
  });

  describe('full pipeline: decompose → assign → createSubPlans', () => {
    it('end-to-end with 3 independent file groups', () => {
      const plan = makePlan([
        // Group A: src/auth/*
        makeStep(1, ['src/auth/login.ts'], [], 'read'),
        makeStep(2, ['src/auth/login.ts'], [1], 'edit'),
        // Group B: src/api/*
        makeStep(3, ['src/api/routes.ts'], [], 'read'),
        makeStep(4, ['src/api/routes.ts'], [3], 'edit'),
        // Group C: src/utils/*
        makeStep(5, ['src/utils/helpers.ts'], [], 'read'),
        makeStep(6, ['src/utils/helpers.ts'], [5], 'edit'),
      ]);

      const agents = [
        makeAgent(AGENT_A, 'Auth Specialist', 0.1),
        makeAgent(AGENT_B, 'API Specialist', 0.2),
        makeAgent(AGENT_C, 'Utils Specialist', 0.3),
      ];

      // Step 1: Decompose
      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(3);

      // Step 2: Assign
      const assignments = delegator.assign(clusters, agents, plan);
      expect(assignments).toHaveLength(3);

      // Step 3: Create sub-plans
      const subPlans = delegator.createSubPlans(plan, assignments);
      expect(subPlans).toHaveLength(3);

      // Each sub-plan has exactly 2 steps
      for (const sub of subPlans) {
        expect(sub.steps).toHaveLength(2);
        expect(sub.status).toBe('approved');
      }

      // All 6 steps are accounted for
      const allSteps = subPlans.flatMap(s => s.steps.map(st => st.stepNumber));
      expect(allSteps.sort()).toEqual([1, 2, 3, 4, 5, 6]);
    });

    it('single monolithic plan stays as one cluster', () => {
      const plan = makePlan([
        makeStep(1, ['src/index.ts']),
        makeStep(2, ['src/index.ts', 'src/types.ts'], [1]),
        makeStep(3, ['src/types.ts', 'src/index.ts'], [2]),
      ]);

      const clusters = delegator.decompose(plan);
      expect(clusters).toHaveLength(1);
      expect(clusters[0].stepNumbers).toEqual([1, 2, 3]);
    });
  });
});
