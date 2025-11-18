/**
 * CognitionPlanStepExecutionEntity - Logs individual plan step execution
 *
 * Enables observability into:
 * - Which step is currently executing
 * - Step completion status (started/completed/failed)
 * - Step results and errors
 * - Step execution timing
 *
 * Used for:
 * - Debugging plan execution failures
 * - Understanding bottlenecks in planning
 * - Training data for plan optimization
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Plan step execution status
 */
export type PlanStepStatus = 'started' | 'completed' | 'failed';

/**
 * CognitionPlanStepExecutionEntity - Complete record of plan step execution
 */
export class CognitionPlanStepExecutionEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.COGNITION_PLAN_STEP_EXECUTIONS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId!: UUID;

  @NumberField()
  stepNumber!: number;

  @TextField()
  action!: string;

  @EnumField({ index: true })
  status!: PlanStepStatus;

  @JsonField({ nullable: true })
  result?: unknown;

  @TextField({ nullable: true })
  errorMessage?: string;

  @NumberField({ nullable: true })
  durationMs?: number;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;

  constructor() {
    super();

    // Default values
    this.personaId = '' as UUID;
    this.personaName = '';
    this.planId = '' as UUID;
    this.stepNumber = 0;
    this.action = '';
    this.status = 'started';
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CognitionPlanStepExecutionEntity.collection;
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
      defaultPageSize: 50,
      cursorField: 'sequenceNumber'
    };
  }

  /**
   * Implement BaseEntity abstract method - validate plan step execution data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'CognitionPlanStepExecution personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'CognitionPlanStepExecution personaName is required' };
    }

    if (!this.planId?.trim()) {
      return { success: false, error: 'CognitionPlanStepExecution planId is required' };
    }

    if (typeof this.stepNumber !== 'number' || this.stepNumber < 0) {
      return { success: false, error: 'CognitionPlanStepExecution stepNumber must be a non-negative number' };
    }

    if (!this.action?.trim()) {
      return { success: false, error: 'CognitionPlanStepExecution action is required' };
    }

    const validStatuses: PlanStepStatus[] = ['started', 'completed', 'failed'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `CognitionPlanStepExecution status must be one of: ${validStatuses.join(', ')}` };
    }

    if (this.durationMs !== undefined && (typeof this.durationMs !== 'number' || this.durationMs < 0)) {
      return { success: false, error: 'CognitionPlanStepExecution durationMs must be a non-negative number if provided' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'CognitionPlanStepExecution domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'CognitionPlanStepExecution contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'CognitionPlanStepExecution sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
