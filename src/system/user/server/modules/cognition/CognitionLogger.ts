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
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { Commands } from '../../../../core/shared/Commands';
import { COLLECTIONS } from '../../../../shared/Constants';
import { DataDaemon } from '../../../../../daemons/data-daemon/shared/DataDaemon';
import { generateUUID } from '../../../../core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../../../../../commands/data/create/shared/DataCreateTypes';
import type { DataListParams, DataListResult } from '../../../../../commands/data/list/shared/DataListTypes';
import type { BaseEntity } from '../../../../data/entities/BaseEntity';
import type { DataUpdateParams, DataUpdateResult } from '../../../../../commands/data/update/shared/DataUpdateTypes';
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
import type { ToolExecutionStatus } from '../../../../data/entities/ToolExecutionLogEntity';
import type { AdapterDecision, DecisionContextMetadata } from '../../../../data/entities/AdapterDecisionLogEntity';
import type { ResponseStatus } from '../../../../data/entities/ResponseGenerationLogEntity';

import { DataCreate } from '../../../../../commands/data/create/shared/DataCreateTypes';
import { DataList } from '../../../../../commands/data/list/shared/DataListTypes';
import { DataUpdate } from '../../../../../commands/data/update/shared/DataUpdateTypes';
/**
 * CognitionLogger - Static utility for logging cognition events
 */
export class CognitionLogger {
  private static stateSequenceCounters = new Map<UUID, number>();
  private static planSequenceCounters = new Map<UUID, number>();
  private static toolSequenceCounters = new Map<UUID, number>();
  private static adapterSequenceCounters = new Map<UUID, number>();
  private static responseSequenceCounters = new Map<UUID, number>();
  private static stepSequenceCounters = new Map<UUID, number>();
  private static stateUpdateSequenceCounters = new Map<UUID, number>();
  private static memoryOpSequenceCounters = new Map<UUID, number>();
  private static adapterReasoningSequenceCounters = new Map<UUID, number>();
  private static replanSequenceCounters = new Map<UUID, number>();

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

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.COGNITION_STATE_SNAPSHOTS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger state snapshot write failed:', err));
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

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger plan formulation write failed:', err));
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log plan formulation:`, error);
    }
  }

  /**
   * Log plan completion
   * Called when a plan finishes (success or failure)
   */
  static logPlanCompletion(
    planId: UUID,
    status: 'completed' | 'failed' | 'aborted',
    steps: PlanStepSnapshot[],
    evaluation?: Evaluation
  ): void {
    // Fire-and-forget: run the async chain but don't block caller
    (async () => {
      try {
        // Find the plan record in database
        const planRecords = await DataList.execute({
          collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
          filter: { planId },
          limit: 1,
          backend: 'server',
          context: DataDaemon.jtagContext!,
          sessionId: DataDaemon.jtagContext!.uuid
        }) as DataListResult<CognitionPlanEntity>;

        if (!planRecords.items || planRecords.items.length === 0) {
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

        await DataUpdate.execute({
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
      } catch (error) {
        console.error(`❌ CognitionLogger: Failed to log plan completion:`, error);
      }
    })();
  }

  /**
   * Log plan adjustment
   * Called when a plan is adjusted mid-execution
   */
  static logPlanAdjustment(
    planId: UUID,
    adjustment: PlanAdjustment
  ): void {
    // Fire-and-forget: run the async chain but don't block caller
    (async () => {
      try {
        // Find the plan record in database
        const planRecords = await DataList.execute({
          collection: COLLECTIONS.COGNITION_PLAN_RECORDS,
          filter: { planId },
          limit: 1,
          backend: 'server',
          context: DataDaemon.jtagContext!,
          sessionId: DataDaemon.jtagContext!.uuid
        }) as DataListResult<CognitionPlanEntity>;

        if (!planRecords.items || planRecords.items.length === 0) {
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

        await DataUpdate.execute({
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
      } catch (error) {
        console.error(`❌ CognitionLogger: Failed to log plan adjustment:`, error);
      }
    })();
  }

  /**
   * Log tool execution
   * Called before/after every tool or command execution
   */
  static async logToolExecution(
    personaId: UUID,
    personaName: string,
    toolName: string,
    toolParams: unknown,
    executionStatus: ToolExecutionStatus,
    durationMs: number,
    domain: string,
    contextId: UUID,
    options: {
      planId?: UUID;
      toolResult?: unknown;
      errorMessage?: string;
      triggeredBy?: string;
      storedResultId?: UUID;  // Phase 3B: Link to stored result in working memory
    } = {}
  ): Promise<void> {
    try {
      // Get sequence number
      const currentSeq = this.toolSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.toolSequenceCounters.set(personaId, sequenceNumber);

      const completedAt = Date.now();
      const startedAt = completedAt - durationMs;

      // Create entity data
      const entityData = {
        id: generateUUID(),
        timestamp: new Date(),
        createdAt: new Date(),
        updatedAt: new Date(),
        version: 0,

        // Identity
        personaId,
        personaName,
        planId: options.planId,

        // Tool info
        toolName,
        toolParams,
        executionStatus,
        toolResult: options.toolResult,
        errorMessage: options.errorMessage,

        // Timing
        durationMs,
        startedAt,
        completedAt,

        // Metadata
        domain,
        contextId,
        triggeredBy: options.triggeredBy,
        storedResultId: options.storedResultId,  // Phase 3B: Link to stored result
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.TOOL_EXECUTION_LOGS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger tool execution write failed:', err));
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log tool execution:`, error);
    }
  }

  /**
   * Log adapter decision
   * Called for each adapter in the decision chain
   */
  static async logAdapterDecision(
    personaId: UUID,
    personaName: string,
    adapterName: string,
    decision: AdapterDecision,
    confidence: number,
    reasoning: string,
    decisionContext: DecisionContextMetadata,
    evaluationDurationMs: number,
    domain: string,
    contextId: UUID,
    planId?: UUID
  ): Promise<void> {
    try {
      // Get sequence number
      const currentSeq = this.adapterSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.adapterSequenceCounters.set(personaId, sequenceNumber);

      // Create entity data
      const entityData = {
        id: generateUUID(),
        timestamp: Date.now(),
        createdAt: new Date(),
        updatedAt: new Date(),
        version: 0,

        // Identity
        personaId,
        personaName,
        planId,

        // Decision info
        adapterName,
        decision,
        confidence,
        reasoning,
        decisionContext,

        // Timing
        evaluationDurationMs,

        // Metadata
        domain,
        contextId,
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.ADAPTER_DECISION_LOGS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger adapter decision write failed:', err));
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log adapter decision:`, error);
    }
  }

  /**
   * Log AI response generation
   * Called for every AI model invocation
   */
  static async logResponseGeneration(
    personaId: UUID,
    personaName: string,
    provider: string,
    model: string,
    promptSummary: string,
    promptTokens: number,
    completionTokens: number,
    estimatedCost: number,
    responseSummary: string,
    durationMs: number,
    status: ResponseStatus,
    temperature: number,
    domain: string,
    contextId: UUID,
    options: {
      planId?: UUID;
      errorMessage?: string;
    } = {}
  ): Promise<void> {
    try {
      // Get sequence number
      const currentSeq = this.responseSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.responseSequenceCounters.set(personaId, sequenceNumber);

      const totalTokens = promptTokens + completionTokens;

      // Create entity data
      const entityData = {
        id: generateUUID(),
        timestamp: Date.now(),
        createdAt: new Date(),
        updatedAt: new Date(),
        version: 0,

        // Identity
        personaId,
        personaName,
        planId: options.planId,

        // Model info
        provider,
        model,

        // Content (truncated)
        promptSummary,
        responseSummary,

        // Token usage
        promptTokens,
        completionTokens,
        totalTokens,
        estimatedCost,

        // Generation info
        durationMs,
        status,
        errorMessage: options.errorMessage,
        temperature,

        // Metadata
        domain,
        contextId,
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.RESPONSE_GENERATION_LOGS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger response generation write failed:', err));
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log response generation:`, error);
    }
  }

  /**
   * Log individual plan step execution
   * Called on step start/completion
   */
  static async logPlanStepExecution(
    personaId: UUID,
    personaName: string,
    planId: UUID,
    stepNumber: number,
    action: string,
    status: 'started' | 'completed' | 'failed',
    domain: string,
    contextId: UUID,
    options: {
      result?: unknown;
      errorMessage?: string;
      durationMs?: number;
    } = {}
  ): Promise<void> {
    try {
      const currentSeq = this.stepSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.stepSequenceCounters.set(personaId, sequenceNumber);

      const timestamp = new Date();
      const entityData = {
        id: generateUUID(),
        timestamp,
        createdAt: timestamp,
        updatedAt: timestamp,
        version: 0,
        personaId,
        personaName,
        planId,
        stepNumber,
        action,
        status,
        result: options.result,
        errorMessage: options.errorMessage,
        durationMs: options.durationMs,
        domain,
        contextId,
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.COGNITION_PLAN_STEP_EXECUTIONS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger plan step write failed:', err));

      // Success log removed - data already persisted
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log plan step execution:`, error);
    }
  }

  /**
   * Log self-state update
   * Tracks incremental changes to beliefs/goals/preoccupations
   */
  static async logSelfStateUpdate(
    personaId: UUID,
    personaName: string,
    updateType: 'belief' | 'goal' | 'preoccupation',
    previousValue: unknown,
    newValue: unknown,
    reason: string,
    domain: string,
    contextId: UUID,
    planId?: UUID
  ): Promise<void> {
    try {
      const currentSeq = this.stateUpdateSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.stateUpdateSequenceCounters.set(personaId, sequenceNumber);

      const timestamp = new Date();
      const entityData = {
        id: generateUUID(),
        timestamp,
        createdAt: timestamp,
        updatedAt: timestamp,
        version: 0,
        personaId,
        personaName,
        planId,
        updateType,
        previousValue,
        newValue,
        reason,
        domain,
        contextId,
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.COGNITION_SELF_STATE_UPDATES,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger self-state update write failed:', err));

      // Success log removed - data already persisted
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log self-state update:`, error);
    }
  }

  /**
   * Log memory operation
   * Tracks add/remove/evict in working memory
   */
  static async logMemoryOperation(
    personaId: UUID,
    personaName: string,
    operation: 'add' | 'remove' | 'evict',
    memoryId: UUID,
    thoughtType: string,
    thoughtContent: string,
    importance: number,
    reason: string,
    domain: string,
    contextId: UUID,
    planId?: UUID
  ): Promise<void> {
    try {
      const currentSeq = this.memoryOpSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.memoryOpSequenceCounters.set(personaId, sequenceNumber);

      const timestamp = new Date();
      const entityData = {
        id: generateUUID(),
        timestamp,
        createdAt: timestamp,
        updatedAt: timestamp,
        version: 0,
        personaId,
        personaName,
        planId,
        operation,
        memoryId,
        thoughtType,
        thoughtContent,
        importance,
        reason,
        domain,
        contextId,
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.COGNITION_MEMORY_OPERATIONS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger memory operation write failed:', err));

      // Success log removed - data already persisted
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log memory operation:`, error);
    }
  }

  /**
   * Log adapter reasoning
   * Captures intermediate evaluation steps
   */
  static async logAdapterReasoning(
    personaId: UUID,
    personaName: string,
    adapterName: string,
    stepDescription: string,
    intermediateResult: unknown,
    confidence: number,
    durationMs: number,
    domain: string,
    contextId: UUID,
    planId?: UUID
  ): Promise<void> {
    try {
      const currentSeq = this.adapterReasoningSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.adapterReasoningSequenceCounters.set(personaId, sequenceNumber);

      const timestamp = new Date();
      const entityData = {
        id: generateUUID(),
        timestamp,
        createdAt: timestamp,
        updatedAt: timestamp,
        version: 0,
        personaId,
        personaName,
        planId,
        adapterName,
        stepDescription,
        intermediateResult,
        confidence,
        durationMs,
        domain,
        contextId,
        sequenceNumber
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.ADAPTER_REASONING_LOGS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger adapter reasoning write failed:', err));

      // Success log removed - data already persisted
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log adapter reasoning:`, error);
    }
  }

  /**
   * Log plan replan
   * Tracks full replanning events
   */
  static async logPlanReplan(
    personaId: UUID,
    personaName: string,
    oldPlanId: UUID,
    newPlan: Plan,
    reason: string,
    domain: string,
    contextId: UUID,
    modelUsed?: string
  ): Promise<void> {
    try {
      const currentSeq = this.replanSequenceCounters.get(personaId) ?? 0;
      const sequenceNumber = currentSeq + 1;
      this.replanSequenceCounters.set(personaId, sequenceNumber);

      // Build new plan snapshot similar to logPlanFormulation
      const stepsSnapshot: PlanStepSnapshot[] = newPlan.steps.map(s => ({
        stepNumber: s.stepNumber,
        action: s.action,
        expectedOutcome: s.expectedOutcome,
        completed: s.completed,
        completedAt: s.completedAt,
        result: s.result
      }));

      const timestamp = new Date();
      const entityData = {
        id: generateUUID(),
        timestamp,
        createdAt: timestamp,
        updatedAt: timestamp,
        version: 0,
        personaId,
        personaName,
        oldPlanId,
        newPlanId: newPlan.id,
        goal: newPlan.goal,
        steps: stepsSnapshot,
        reason,
        domain,
        contextId,
        sequenceNumber,
        modelUsed
      };

      // Fire-and-forget: cognition logs are observability, not user-facing
      DataCreate.execute({
        collection: COLLECTIONS.COGNITION_PLAN_REPLANS,
        data: entityData,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid,
        suppressEvents: true
      }).catch(err => console.error('CognitionLogger plan replan write failed:', err));

      // Success log removed - data already persisted
    } catch (error) {
      console.error(`❌ CognitionLogger: Failed to log plan replan:`, error);
    }
  }
}
