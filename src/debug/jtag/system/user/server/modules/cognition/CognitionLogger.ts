/**
 * CognitionLogger - Observability for persona cognition system
 *
 * Logs complete cognition lifecycle for debugging and widgets:
 * - Self-state snapshots (focus, load, preoccupations, working memory)
 * - Plan formulation and execution
 * - Plan adjustments and replanning
 * - Evaluations and learning extraction
 *
 * Enables:
 * - Real-time monitoring widgets
 * - Time-travel debugging
 * - Cognitive load analysis
 * - Plan strategy analysis
 * - Training data for meta-learning
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { Commands } from '../../../../core/shared/Commands';
import { COLLECTIONS } from '../../../../shared/Constants';
import { DataDaemon } from '../../../../../daemons/data-daemon/shared/DataDaemon';
import { generateUUID } from '../../../../core/types/CrossPlatformUUID';
import type { DataListResult } from '../../../../../commands/data/list/shared/DataListTypes';
import type {
  CognitionStateEntity,
  FocusSnapshot,
  Preoccupation,
  WorkingMemorySnapshot
} from '../../../../data/entities/CognitionStateEntity';
import type {
  CognitionPlanEntity,
  PlanStepSnapshot,
  TaskSnapshot,
  PlanAdjustmentSnapshot,
  PlanEvaluation
} from '../../../../data/entities/CognitionPlanEntity';
import type { Task, Plan, PlanAdjustment, Evaluation } from './reasoning/types';
import type { SelfStateEntry, WorkingMemoryEntry } from './memory/InMemoryCognitionStorage';

/**
 * CognitionLogger - Static utility for logging cognition events
 */
export class CognitionLogger {
  private static stateSequenceCounters = new Map<UUID, number>();
  private static planSequenceCounters = new Map<UUID, number>();

  /**
   * Log a cognition state snapshot
   * Called periodically or on significant state changes
   */
  static async logStateSnapshot(
    personaId: UUID,
    personaName: string,
    selfState: SelfStateEntry,
    workingMemory: WorkingMemoryEntry[],
    workingMemoryCapacity: { used: number; max: number; byDomain: Record<string, number> },
    options: {
      domain: string;
      contextId?: UUID;
      triggerEvent?: string;
    }
  ): Promise<void> {
    try {
      // Get sequence number
      const currentSeq = this.stateSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.stateSequenceCounters.set(personaId, sequenceNumber);

      // Build working memory snapshot
      const workingMemorySnapshot: WorkingMemorySnapshot[] = workingMemory.map(m => ({
        id: m.id,
        domain: m.domain,
        contextId: m.contextId,
        thoughtType: m.thoughtType,
        thoughtContent: m.thoughtContent,
        importance: m.importance,
        createdAt: m.createdAt,
        lastAccessedAt: m.lastAccessedAt
      }));

      // Create entity data (let DataDaemon handle BaseEntity fields)
      const entityData = {
        id: generateUUID(),
        timestamp: new Date(),
        createdAt: new Date(),
        updatedAt: new Date(),
        version: 0,

        // Identity
        personaId,
        personaName,

        // Current focus
        currentFocus: selfState.currentFocus,

        // Cognitive resources
        cognitiveLoad: selfState.cognitiveLoad,
        availableCapacity: selfState.availableCapacity,

        // Active preoccupations
        activePreoccupations: selfState.activePreoccupations,

        // Working memory
        workingMemory: workingMemorySnapshot,
        workingMemoryCapacity,

        // Metadata
        domain: options.domain,
        contextId: options.contextId,
        triggerEvent: options.triggerEvent,
        sequenceNumber
      };

      // Store to database (fire-and-forget)
      await Commands.execute('data/create', {
        collection: COLLECTIONS.COGNITION_STATE_SNAPSHOTS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      console.log(`📊 CognitionLogger: Logged state snapshot (seq=${sequenceNumber}, load=${selfState.cognitiveLoad.toFixed(2)}, memories=${workingMemory.length})`);
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log state snapshot:`, error);
      // Don't throw - logging failures shouldn't break persona functionality
    }
  }

  /**
   * Log plan formulation
   * Called when a new plan is created
   */
  static async logPlanFormulation(
    personaId: UUID,
    personaName: string,
    task: Task,
    plan: Plan,
    domain: string,
    contextId: UUID,
    modelUsed?: string
  ): Promise<void> {
    try {
      // Get sequence number
      const currentSeq = this.planSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.planSequenceCounters.set(personaId, sequenceNumber);

      // Build task snapshot
      const taskSnapshot: TaskSnapshot = {
        id: task.id,
        domain: task.domain,
        contextId: task.contextId,
        description: task.description,
        priority: task.priority,
        triggeredBy: task.triggeredBy,
        createdAt: task.createdAt
      };

      // Build plan steps snapshot
      const stepsSnapshot: PlanStepSnapshot[] = plan.steps.map(s => ({
        stepNumber: s.stepNumber,
        action: s.action,
        expectedOutcome: s.expectedOutcome,
        completed: s.completed,
        completedAt: s.completedAt,
        result: s.result
      }));

      // Create entity data (let DataDaemon handle BaseEntity fields)
      const entityData = {
        id: generateUUID(),
        timestamp: new Date(),
        createdAt: new Date(),
        updatedAt: new Date(),
        version: 0,

        // Identity
        personaId,
        personaName,
        planId: plan.id,

        // Task context
        task: taskSnapshot,

        // Plan formulation
        goal: plan.goal,
        learnings: plan.learnings,
        risks: plan.risks,
        steps: stepsSnapshot,
        currentStep: plan.currentStep,
        contingencies: plan.contingencies,
        successCriteria: plan.successCriteria,

        // Execution lifecycle
        status: 'active',
        startedAt: Date.now(),

        // Adjustments
        adjustments: [],
        previousAttempts: plan.previousAttempts,

        // Metadata
        domain,
        contextId,
        sequenceNumber,
        modelUsed
      };

      // Store to database (fire-and-forget)
      await Commands.execute('data/create', {
        collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      console.log(`📋 CognitionLogger: Logged plan formulation (seq=${sequenceNumber}, planId=${plan.id}, steps=${plan.steps.length})`);
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log plan formulation:`, error);
    }
  }

  /**
   * Log plan completion
   * Called when a plan finishes (success or failure)
   */
  static async logPlanCompletion(
    planId: UUID,
    status: 'completed' | 'failed' | 'aborted',
    steps: PlanStepSnapshot[],
    evaluation?: Evaluation
  ): Promise<void> {
    try {
      // Find the plan record in database
      const planRecords = await Commands.execute<any, DataListResult<CognitionPlanEntity>>('data/list', {
        collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
        filter: { planId },
        limit: 1,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      if (!planRecords.items || planRecords.items.length === 0) {
        console.warn(`⚠️ CognitionLogger: No plan record found for planId=${planId}`);
        return;
      }

      const planRecord = planRecords.items[0];

      // Build evaluation snapshot if provided
      let evaluationSnapshot: PlanEvaluation | undefined;
      if (evaluation) {
        evaluationSnapshot = {
          meetsSuccessCriteria: evaluation.meetsSuccessCriteria,
          criteriaBreakdown: evaluation.criteriaBreakdown,
          whatWorked: evaluation.whatWorked,
          mistakes: evaluation.mistakes,
          improvements: evaluation.improvements,
          extractedPattern: evaluation.extractedPattern,
          evaluatedAt: evaluation.evaluatedAt,
          duration: evaluation.duration,
          stepsExecuted: evaluation.stepsExecuted,
          replansRequired: evaluation.replansRequired
        };
      }

      // Update plan record
      const completedAt = Date.now();
      const totalDuration = completedAt - planRecord.startedAt;

      await Commands.execute('data/update', {
        collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
        id: planRecord.id,
        data: {
          status,
          steps,
          completedAt,
          totalDuration,
          evaluation: evaluationSnapshot
        },
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      console.log(`✅ CognitionLogger: Logged plan completion (planId=${planId}, status=${status}, duration=${totalDuration}ms)`);
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log plan completion:`, error);
    }
  }

  /**
   * Log plan adjustment
   * Called when a plan is adjusted mid-execution
   */
  static async logPlanAdjustment(
    planId: UUID,
    adjustment: PlanAdjustment
  ): Promise<void> {
    try {
      // Find the plan record in database
      const planRecords = await Commands.execute<any, DataListResult<CognitionPlanEntity>>('data/list', {
        collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
        filter: { planId },
        limit: 1,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      if (!planRecords.items || planRecords.items.length === 0) {
        console.warn(`⚠️ CognitionLogger: No plan record found for planId=${planId}`);
        return;
      }

      const planRecord = planRecords.items[0];

      // Build adjustment snapshot
      const adjustmentSnapshot: PlanAdjustmentSnapshot = {
        timestamp: Date.now(),
        reason: `Plan adjustment: ${adjustment.action}`,
        action: adjustment.action,
        updatedSteps: adjustment.updatedPlan.steps.map(s => ({
          stepNumber: s.stepNumber,
          action: s.action,
          expectedOutcome: s.expectedOutcome,
          completed: s.completed,
          completedAt: s.completedAt,
          result: s.result
        })),
        reasoning: adjustment.reasoning
      };

      // Update plan record
      const updatedAdjustments = [...planRecord.adjustments, adjustmentSnapshot];

      await Commands.execute('data/update', {
        collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
        id: planRecord.id,
        data: {
          adjustments: updatedAdjustments,
          steps: adjustmentSnapshot.updatedSteps,
          currentStep: adjustment.updatedPlan.currentStep
        },
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      console.log(`🔄 CognitionLogger: Logged plan adjustment (planId=${planId}, action=${adjustment.action})`);
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log plan adjustment:`, error);
    }
  }
}
