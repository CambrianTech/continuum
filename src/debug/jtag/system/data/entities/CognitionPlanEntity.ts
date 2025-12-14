/**
 * CognitionPlanEntity - Captures complete plan lifecycle for observability
 *
 * Enables observability into:
 * - How persona formulates plans
 * - Plan execution progress
 * - Plan adjustments and replanning
 * - Success/failure outcomes
 * - Learning extraction
 *
 * Used for:
 * - Real-time plan execution monitoring
 * - Debugging reasoning failures
 * - Analyzing planning strategies
 * - Training plan formulation models
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField, CompositeIndex } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Plan step with execution details
 */
export interface PlanStepSnapshot {
  stepNumber: number;
  action: string;
  expectedOutcome: string;
  completed: boolean;
  completedAt?: number;
  result?: any;
  error?: string;
  duration?: number;
}

/**
 * Task that triggered the plan
 */
export interface TaskSnapshot {
  id: UUID;
  domain: string;
  contextId: UUID;
  description: string;
  priority: number;
  triggeredBy: UUID;
  createdAt: number;
}

/**
 * Plan adjustment decision
 */
export interface PlanAdjustmentSnapshot {
  timestamp: number;
  reason: string;              // Why adjustment was needed
  action: 'CONTINUE' | 'CONTINGENCY' | 'REPLAN' | 'ABORT';
  updatedSteps?: PlanStepSnapshot[];
  reasoning: string;
}

/**
 * Plan evaluation/outcome
 */
export interface PlanEvaluation {
  meetsSuccessCriteria: boolean;
  criteriaBreakdown: Record<string, boolean>;
  whatWorked: string[];
  mistakes: string[];
  improvements: string[];
  extractedPattern?: string;    // Learning extracted
  evaluatedAt: number;
  duration: number;
  stepsExecuted: number;
  replansRequired: number;
}

/**
 * Plan status type
 */
export type PlanStatus = 'active' | 'completed' | 'failed' | 'aborted';

/**
 * CognitionPlanEntity - Complete plan lifecycle record
 *
 * Composite indexes optimize common observability queries:
 * 1. Active plans by persona: WHERE personaId = ? AND status = 'active' ORDER BY startedAt DESC
 * 2. Recent plans by persona: WHERE personaId = ? ORDER BY startedAt DESC
 * 3. Plans in conversation: WHERE domain = ? AND contextId = ? ORDER BY sequenceNumber DESC
 */
@CompositeIndex({
  name: 'idx_cognition_plans_persona_status',
  fields: ['personaId', 'status', 'startedAt'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_plans_persona_started',
  fields: ['personaId', 'startedAt'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_plans_context',
  fields: ['domain', 'contextId', 'sequenceNumber'],
  direction: 'DESC'
})
export class CognitionPlanEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.COGNITION_PLAN_RECORDS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId!: UUID;

  @JsonField()
  task!: TaskSnapshot;

  @TextField()
  goal!: string;

  @JsonField()
  learnings!: string[];

  @JsonField()
  risks!: string[];

  @JsonField()
  steps!: PlanStepSnapshot[];

  @NumberField()
  currentStep!: number;

  @JsonField()
  contingencies!: Record<string, string[]>;

  @JsonField()
  successCriteria!: string[];

  @EnumField({ index: true })
  status!: PlanStatus;

  @NumberField()
  startedAt!: number;

  @NumberField({ nullable: true })
  completedAt?: number;

  @NumberField({ nullable: true })
  totalDuration?: number;

  @JsonField()
  adjustments!: PlanAdjustmentSnapshot[];

  @NumberField()
  previousAttempts!: number;

  @JsonField({ nullable: true })
  evaluation?: PlanEvaluation;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;

  @TextField({ nullable: true })
  modelUsed?: string;

  constructor() {
    super();

    // Default values
    this.personaId = '' as UUID;
    this.personaName = '';
    this.planId = '' as UUID;
    this.task = {
      id: '' as UUID,
      domain: '',
      contextId: '' as UUID,
      description: '',
      priority: 0,
      triggeredBy: '' as UUID,
      createdAt: 0
    };
    this.goal = '';
    this.learnings = [];
    this.risks = [];
    this.steps = [];
    this.currentStep = 0;
    this.contingencies = {};
    this.successCriteria = [];
    this.status = 'active';
    this.startedAt = 0;
    this.adjustments = [];
    this.previousAttempts = 0;
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CognitionPlanEntity.collection;
  }

  /**
   * Override pagination config - sort by sequence number DESC (newest first)
   */
  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'sequenceNumber',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 30,
      cursorField: 'sequenceNumber'
    };
  }

  /**
   * Implement BaseEntity abstract method - validate plan data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'CognitionPlan personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'CognitionPlan personaName is required' };
    }

    if (!this.planId?.trim()) {
      return { success: false, error: 'CognitionPlan planId is required' };
    }

    if (!this.task || !this.task.id || !this.task.description) {
      return { success: false, error: 'CognitionPlan task is required with id and description' };
    }

    if (!this.goal?.trim()) {
      return { success: false, error: 'CognitionPlan goal is required' };
    }

    if (!Array.isArray(this.learnings)) {
      return { success: false, error: 'CognitionPlan learnings must be an array' };
    }

    if (!Array.isArray(this.risks)) {
      return { success: false, error: 'CognitionPlan risks must be an array' };
    }

    if (!Array.isArray(this.steps)) {
      return { success: false, error: 'CognitionPlan steps must be an array' };
    }

    if (!Array.isArray(this.successCriteria)) {
      return { success: false, error: 'CognitionPlan successCriteria must be an array' };
    }

    const validStatuses: PlanStatus[] = ['active', 'completed', 'failed', 'aborted'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `CognitionPlan status must be one of: ${validStatuses.join(', ')}` };
    }

    if (typeof this.startedAt !== 'number' || this.startedAt < 0) {
      return { success: false, error: 'CognitionPlan startedAt must be a non-negative number' };
    }

    if (!Array.isArray(this.adjustments)) {
      return { success: false, error: 'CognitionPlan adjustments must be an array' };
    }

    if (typeof this.previousAttempts !== 'number' || this.previousAttempts < 0) {
      return { success: false, error: 'CognitionPlan previousAttempts must be a non-negative number' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'CognitionPlan domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'CognitionPlan contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'CognitionPlan sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
