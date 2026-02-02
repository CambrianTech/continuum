/**
 * CodeTaskDelegator - Decomposes plans into sub-plans for parallel multi-agent execution
 *
 * A lead AI creates a top-level plan, then the delegator:
 * 1. Analyzes the step DAG for independent file clusters
 * 2. Assigns clusters to available agents based on capabilities
 * 3. Creates sub-plan entities (parentPlanId = parent)
 * 4. After execution, consolidates results from sub-plans
 *
 * File clusters: Groups of steps that share file dependencies.
 * Two steps that touch the same file MUST be in the same cluster.
 * Steps in different clusters CAN execute in parallel.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type {
  AgentCapability,
  DelegationResult,
  CodingResult,
  CodingResultStatus,
} from '../shared/CodingTypes';
import {
  CodingPlanEntity,
  type CodingStepSnapshot,
} from '../../data/entities/CodingPlanEntity';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodeTaskDelegator', 'code');

// ────────────────────────────────────────────────────────────
// File cluster — a group of steps that share file dependencies
// ────────────────────────────────────────────────────────────

export interface FileCluster {
  /** Unique cluster index */
  readonly index: number;

  /** Step numbers in this cluster (topologically ordered) */
  readonly stepNumbers: number[];

  /** All files touched by steps in this cluster */
  readonly files: string[];

  /** Step numbers from other clusters that this cluster depends on */
  readonly externalDeps: number[];
}

// ────────────────────────────────────────────────────────────
// Agent assignment — which agent gets which cluster
// ────────────────────────────────────────────────────────────

export interface AgentAssignment {
  readonly agentId: UUID;
  readonly agentName: string;
  readonly clusters: FileCluster[];
  readonly totalSteps: number;
  readonly files: string[];
}

// ────────────────────────────────────────────────────────────
// Implementation
// ────────────────────────────────────────────────────────────

export class CodeTaskDelegator {

  /**
   * Decompose a plan's step DAG into independent file clusters.
   *
   * Algorithm (union-find on files):
   * 1. Each step has a set of target files
   * 2. Steps that share ANY file belong to the same cluster
   * 3. Steps connected via dependsOn also belong to the same cluster
   * 4. Result: disjoint clusters that can execute in parallel
   */
  decompose(plan: CodingPlanEntity): FileCluster[] {
    if (plan.steps.length === 0) return [];

    // Union-Find on step indices
    const parent = new Map<number, number>();
    const rank = new Map<number, number>();

    const find = (x: number): number => {
      if (!parent.has(x)) { parent.set(x, x); rank.set(x, 0); }
      if (parent.get(x) !== x) parent.set(x, find(parent.get(x)!));
      return parent.get(x)!;
    };

    const union = (a: number, b: number): void => {
      const ra = find(a), rb = find(b);
      if (ra === rb) return;
      const rkA = rank.get(ra) ?? 0, rkB = rank.get(rb) ?? 0;
      if (rkA < rkB) { parent.set(ra, rb); }
      else if (rkA > rkB) { parent.set(rb, ra); }
      else { parent.set(rb, ra); rank.set(ra, rkA + 1); }
    };

    // Initialize all steps
    for (const step of plan.steps) {
      find(step.stepNumber);
    }

    // Union steps that share files
    const fileToStep = new Map<string, number>();
    for (const step of plan.steps) {
      for (const file of step.targetFiles) {
        const existing = fileToStep.get(file);
        if (existing !== undefined) {
          union(existing, step.stepNumber);
        } else {
          fileToStep.set(file, step.stepNumber);
        }
      }
    }

    // Union steps connected by dependencies
    for (const step of plan.steps) {
      for (const dep of step.dependsOn) {
        union(step.stepNumber, dep);
      }
    }

    // Group steps by root
    const clusterMap = new Map<number, number[]>();
    for (const step of plan.steps) {
      const root = find(step.stepNumber);
      const group = clusterMap.get(root) ?? [];
      group.push(step.stepNumber);
      clusterMap.set(root, group);
    }

    // Build FileCluster objects
    const stepByNumber = new Map<number, CodingStepSnapshot>();
    for (const step of plan.steps) {
      stepByNumber.set(step.stepNumber, step);
    }

    const clusters: FileCluster[] = [];
    let clusterIndex = 0;

    for (const [, stepNumbers] of clusterMap) {
      // Collect all files in this cluster
      const files = new Set<string>();
      for (const sn of stepNumbers) {
        const step = stepByNumber.get(sn)!;
        for (const f of step.targetFiles) files.add(f);
      }

      // Identify external dependencies (deps outside this cluster)
      const stepSet = new Set(stepNumbers);
      const externalDeps: number[] = [];
      for (const sn of stepNumbers) {
        const step = stepByNumber.get(sn)!;
        for (const dep of step.dependsOn) {
          if (!stepSet.has(dep) && !externalDeps.includes(dep)) {
            externalDeps.push(dep);
          }
        }
      }

      // Sort steps topologically within cluster
      stepNumbers.sort((a, b) => a - b);

      clusters.push({
        index: clusterIndex++,
        stepNumbers,
        files: Array.from(files).sort(),
        externalDeps,
      });
    }

    log.info(`Decomposed ${plan.steps.length} steps into ${clusters.length} clusters`);
    return clusters;
  }

  /**
   * Assign file clusters to available agents.
   *
   * Strategy:
   * - Sort agents by load (least loaded first)
   * - Sort clusters by size (largest first — greedy bin packing)
   * - Assign each cluster to the least-loaded agent that has capacity
   * - Respect agent security tier (cluster needs write → agent needs write+)
   */
  assign(
    clusters: FileCluster[],
    agents: AgentCapability[],
    plan: CodingPlanEntity,
  ): AgentAssignment[] {
    if (clusters.length === 0 || agents.length === 0) return [];

    // Sort agents by load ascending (least loaded first)
    const sortedAgents = [...agents].sort((a, b) => a.currentLoad - b.currentLoad);

    // Sort clusters by step count descending (largest first)
    const sortedClusters = [...clusters].sort((a, b) => b.stepNumbers.length - a.stepNumbers.length);

    // Track assignments
    const assignments = new Map<UUID, { agent: AgentCapability; clusters: FileCluster[] }>();

    for (const cluster of sortedClusters) {
      // Find the least-loaded agent that hasn't been given too many clusters
      let assigned = false;
      for (const agent of sortedAgents) {
        const existing = assignments.get(agent.personaId);
        const currentClusterCount = existing?.clusters.length ?? 0;

        // Simple load balancing: distribute evenly
        const maxClustersPerAgent = Math.ceil(sortedClusters.length / sortedAgents.length);
        if (currentClusterCount >= maxClustersPerAgent) continue;

        if (!existing) {
          assignments.set(agent.personaId, { agent, clusters: [cluster] });
        } else {
          existing.clusters.push(cluster);
        }
        assigned = true;
        break;
      }

      // If no agent available, assign to least loaded
      if (!assigned && sortedAgents.length > 0) {
        const fallback = sortedAgents[0];
        const existing = assignments.get(fallback.personaId);
        if (!existing) {
          assignments.set(fallback.personaId, { agent: fallback, clusters: [cluster] });
        } else {
          existing.clusters.push(cluster);
        }
      }
    }

    // Build AgentAssignment objects
    const result: AgentAssignment[] = [];
    for (const [, { agent, clusters: agentClusters }] of assignments) {
      const allSteps: number[] = [];
      const allFiles = new Set<string>();
      for (const cluster of agentClusters) {
        allSteps.push(...cluster.stepNumbers);
        for (const f of cluster.files) allFiles.add(f);
      }

      result.push({
        agentId: agent.personaId,
        agentName: agent.name,
        clusters: agentClusters,
        totalSteps: allSteps.length,
        files: Array.from(allFiles).sort(),
      });
    }

    log.info(`Assigned ${clusters.length} clusters to ${result.length} agents`);
    return result;
  }

  /**
   * Create sub-plan entities from agent assignments.
   * Each sub-plan contains only the steps assigned to that agent.
   */
  createSubPlans(
    parentPlan: CodingPlanEntity,
    assignments: AgentAssignment[],
  ): CodingPlanEntity[] {
    const stepByNumber = new Map<number, CodingStepSnapshot>();
    for (const step of parentPlan.steps) {
      stepByNumber.set(step.stepNumber, step);
    }

    const subPlans: CodingPlanEntity[] = [];

    for (const assignment of assignments) {
      const subPlan = new CodingPlanEntity();
      subPlan.taskId = parentPlan.taskId;
      subPlan.parentPlanId = parentPlan.id as UUID;
      subPlan.createdById = parentPlan.leadId;
      subPlan.leadId = assignment.agentId;
      subPlan.summary = `Sub-plan for ${assignment.agentName}: ${assignment.files.slice(0, 3).join(', ')}${assignment.files.length > 3 ? '...' : ''}`;
      subPlan.taskDescription = parentPlan.taskDescription;
      subPlan.estimatedToolCalls = assignment.totalSteps;
      subPlan.assignees = [assignment.agentId];
      subPlan.generatedBy = parentPlan.generatedBy;
      subPlan.riskLevel = parentPlan.riskLevel;
      subPlan.riskReason = parentPlan.riskReason;
      subPlan.securityTier = parentPlan.securityTier;
      subPlan.status = 'approved'; // Sub-plans inherit parent approval

      // Copy only the assigned steps, renumber sequentially
      const assignedStepNumbers = new Set<number>();
      for (const cluster of assignment.clusters) {
        for (const sn of cluster.stepNumbers) {
          assignedStepNumbers.add(sn);
        }
      }

      subPlan.steps = Array.from(assignedStepNumbers)
        .sort((a, b) => a - b)
        .map(sn => {
          const original = stepByNumber.get(sn)!;
          return {
            ...original,
            // Filter dependsOn to only include steps within this sub-plan
            dependsOn: original.dependsOn.filter(d => assignedStepNumbers.has(d)),
          };
        });

      subPlans.push(subPlan);
    }

    log.info(`Created ${subPlans.length} sub-plans from parent ${parentPlan.id}`);
    return subPlans;
  }

  /**
   * Consolidate results from sub-plans into the parent plan's CodingResult.
   */
  consolidate(
    parentPlan: CodingPlanEntity,
    subPlans: CodingPlanEntity[],
  ): CodingResult {
    const filesModified = new Set<string>();
    const filesCreated = new Set<string>();
    const changeIds: string[] = [];
    const errors: string[] = [];
    let totalToolCalls = 0;
    let totalDurationMs = 0;

    for (const sub of subPlans) {
      for (const f of sub.filesModified) filesModified.add(f);
      for (const f of sub.filesCreated) filesCreated.add(f);
      changeIds.push(...sub.changeIds);
      errors.push(...sub.errors);
      totalToolCalls += sub.totalToolCalls;
      totalDurationMs = Math.max(totalDurationMs, sub.totalDurationMs); // Parallel = max, not sum
    }

    // Detect conflicts: same file modified by multiple sub-plans
    const fileToSubPlan = new Map<string, UUID[]>();
    for (const sub of subPlans) {
      for (const f of sub.filesModified) {
        const existing = fileToSubPlan.get(f) ?? [];
        existing.push(sub.id as UUID);
        fileToSubPlan.set(f, existing);
      }
    }
    const conflicts = Array.from(fileToSubPlan.entries())
      .filter(([, ids]) => ids.length > 1)
      .map(([file]) => file);

    if (conflicts.length > 0) {
      errors.push(`File conflicts detected: ${conflicts.join(', ')}`);
    }

    // Determine overall status
    if (subPlans.length === 0) {
      return {
        taskId: parentPlan.taskId,
        status: 'failed',
        summary: 'No sub-plans to consolidate',
        stepResults: [],
        filesModified: [],
        filesCreated: [],
        totalToolCalls: 0,
        totalDurationMs: 0,
        changeIds: [],
        errors: ['No sub-plans were executed'],
      };
    }

    const allCompleted = subPlans.every(s => s.status === 'completed');
    const anyCompleted = subPlans.some(s => s.status === 'completed');
    const status: CodingResultStatus = allCompleted
      ? 'completed'
      : anyCompleted
        ? 'partial'
        : 'failed';

    // Build step results from all sub-plans
    const stepResults = subPlans.flatMap(sub =>
      sub.steps.map(step => ({
        stepNumber: step.stepNumber,
        status: step.status === 'completed' ? 'completed' as const
          : step.status === 'skipped' ? 'skipped' as const
          : step.status === 'failed' ? 'failed' as const
          : 'pending' as const,
        output: step.output,
        error: step.error,
        durationMs: step.durationMs ?? 0,
        toolCall: step.toolCall,
      })),
    );

    const summary = allCompleted
      ? `All ${subPlans.length} sub-plans completed`
      : `${subPlans.filter(s => s.status === 'completed').length}/${subPlans.length} sub-plans completed`;

    return {
      taskId: parentPlan.taskId,
      status,
      summary,
      stepResults,
      filesModified: Array.from(filesModified),
      filesCreated: Array.from(filesCreated),
      totalToolCalls,
      totalDurationMs,
      changeIds,
      errors,
    };
  }
}
