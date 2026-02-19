/**
 * CognitionPlanReplanEntity - Logs plan replanning events
 *
 * Enables observability into:
 * - When plans are abandoned and replaced
 * - Reasons for replanning
 * - New plan structure
 * - Replanning frequency patterns
 *
 * Used for:
 * - Understanding plan instability
 * - Debugging replanning loops
 * - Optimizing plan formulation quality
 * - Training data for better initial plans
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, CompositeIndex } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';
import type { PlanStepSnapshot } from './CognitionPlanEntity';

/**
 * CognitionPlanReplanEntity - Complete record of plan replanning
 *
 * Composite indexes optimize common observability queries:
 * 1. Recent replans by persona: WHERE personaId = ? ORDER BY sequenceNumber DESC
 * 2. Replans for old plan: WHERE oldPlanId = ? ORDER BY sequenceNumber DESC
 */
@CompositeIndex({
  name: 'idx_cognition_replans_persona_sequence',
  fields: ['personaId', 'sequenceNumber'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_replans_old_plan',
  fields: ['oldPlanId', 'sequenceNumber'],
  direction: 'DESC'
})
export class CognitionPlanReplanEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.COGNITION_PLAN_REPLANS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  oldPlanId!: UUID;

  @TextField({ index: true })
  newPlanId!: UUID;

  @TextField()
  goal!: string;

  @JsonField()
  steps!: PlanStepSnapshot[];

  @TextField()
  reason!: string;

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
    this.oldPlanId = '' as UUID;
    this.newPlanId = '' as UUID;
    this.goal = '';
    this.steps = [];
    this.reason = '';
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CognitionPlanReplanEntity.collection;
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
   * Implement BaseEntity abstract method - validate plan replan data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'CognitionPlanReplan personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'CognitionPlanReplan personaName is required' };
    }

    if (!this.oldPlanId?.trim()) {
      return { success: false, error: 'CognitionPlanReplan oldPlanId is required' };
    }

    if (!this.newPlanId?.trim()) {
      return { success: false, error: 'CognitionPlanReplan newPlanId is required' };
    }

    if (!this.goal?.trim()) {
      return { success: false, error: 'CognitionPlanReplan goal is required' };
    }

    if (!Array.isArray(this.steps)) {
      return { success: false, error: 'CognitionPlanReplan steps must be an array' };
    }

    if (!this.reason?.trim()) {
      return { success: false, error: 'CognitionPlanReplan reason is required' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'CognitionPlanReplan domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'CognitionPlanReplan contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'CognitionPlanReplan sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
